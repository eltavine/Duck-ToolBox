use crate::{
    fail, kernel,
    mutation::{OperationResult, Request},
    store::Store,
};
use anyhow::{Result, ensure};
use duck_core::{Context, envelope::API_VERSION};
use serde::Deserialize;
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn execute(ctx: &Context, request: &Request) -> Result<OperationResult> {
    request.check(ctx)?;
    // Directory and state share the operation lock. Release it before the worker acquires it.
    drop(Store::open(ctx)?);
    let directory = ctx.paths.var_dir.join("zram");
    let mut input = tempfile::tempfile_in(&directory)?;
    serde_json::to_writer(&mut input, request)?;
    input.seek(SeekFrom::Start(0))?;
    let mut output = tempfile::tempfile_in(&directory)?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.args(["zram", "worker", "--stdin-json"])
        .stdin(input)
        .stdout(output.try_clone()?)
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut process = Worker(cmd.spawn()?);
    let exit = wait(&mut process.0, Duration::from_secs(45))?;
    output.seek(SeekFrom::Start(0)).map_err(|e| {
        fail(
            "operation_unknown",
            format!("cannot read worker result: {e}"),
        )
    })?;
    let mut raw = String::new();
    output.take(65_537).read_to_string(&mut raw).map_err(|e| {
        fail(
            "operation_unknown",
            format!("cannot read worker result: {e}"),
        )
    })?;
    ensure!(
        raw.len() <= 65_536,
        fail("operation_unknown", "worker response exceeds limit")
    );
    decode(&raw, exit.success(), request)
}
struct Worker(std::process::Child);
impl Drop for Worker {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        kernel::terminate_group(self.0.id());
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
fn wait(child: &mut std::process::Child, budget: Duration) -> Result<std::process::ExitStatus> {
    let deadline = Instant::now() + budget;
    loop {
        if let Some(exit) = child.try_wait().map_err(|e| {
            fail(
                "operation_unknown",
                format!("worker status unavailable: {e}"),
            )
        })? {
            return Ok(exit);
        }
        ensure!(
            Instant::now() < deadline,
            fail(
                "operation_timeout",
                "operation timed out; outcome unknown; refresh before taking further action"
            )
        );
        thread::sleep(Duration::from_millis(25));
    }
}
#[derive(Deserialize)]
struct Reply {
    api: u32,
    command: String,
    ok: bool,
    data: Option<OperationResult>,
    error: Option<Failure>,
}
#[derive(Deserialize)]
struct Failure {
    code: String,
    message: String,
}
fn decode(raw: &str, success: bool, request: &Request) -> Result<OperationResult> {
    let reply: Reply = serde_json::from_str(raw)
        .map_err(|_| fail("operation_unknown", "worker returned no reliable result"))?;
    ensure!(
        reply.api == API_VERSION && reply.command == "zram.worker",
        fail("operation_unknown", "unexpected worker protocol")
    );
    if !reply.ok {
        let error = reply
            .error
            .ok_or_else(|| fail("operation_unknown", "worker failure has no details"))?;
        ensure!(
            !success && reply.data.is_none() && !error.message.is_empty(),
            fail("operation_unknown", "contradictory worker failure")
        );
        let code = match error.code.as_str() {
            "confirmation_required" => "confirmation_required",
            "invalid_input" => "invalid_input",
            "state_changed" => "state_changed",
            "device_protected" => "device_protected",
            "state_unavailable" => "state_unavailable",
            "operation_busy" => "operation_busy",
            "insufficient_memory" => "insufficient_memory",
            "original_unavailable" => "original_unavailable",
            "kernel_rejected" => "kernel_rejected",
            "format_failed" => "format_failed",
            "formatter_unavailable" => "formatter_unavailable",
            "unsupported_environment" => "unsupported_environment",
            "operation_unknown" => "operation_unknown",
            _ => "zram_error",
        };
        return Err(fail(code, error.message));
    }
    let data = reply
        .data
        .ok_or_else(|| fail("operation_unknown", "worker result missing"))?;
    ensure!(
        success
            && reply.error.is_none()
            && data.device == request.device
            && data.action == request.action(),
        fail("operation_unknown", "contradictory worker result")
    );
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{code_of, mutation::Operation};
    fn request() -> Request {
        Request {
            device: "zram0".into(),
            revision: "a".repeat(64),
            confirmed: true,
            operation: Operation::Restore,
        }
    }
    #[test]
    fn worker_results_must_match_the_requested_device_action_and_exit() {
        let r = request();
        for raw in ["", "{}", r#"{"api":1,"command":"zram.worker","ok":true}"#] {
            assert_eq!(
                code_of(&decode(raw, true, &r).unwrap_err()),
                "operation_unknown"
            );
        }
        let stats = serde_json::to_value(crate::model::Stats::default()).unwrap();
        let mut value = serde_json::json!({ "api":1,"command":"zram.worker","ok":true,"data":{"device":"zram0","action":"restore","before":stats,"after":stats,"active":true} });
        assert!(decode(&value.to_string(), true, &r).is_ok());
        assert!(decode(&value.to_string(), false, &r).is_err());
        value["data"]["device"] = "zram1".into();
        assert!(decode(&value.to_string(), true, &r).is_err());
        value["data"]["device"] = "zram0".into();
        value["data"]["action"] = "apply".into();
        assert!(decode(&value.to_string(), true, &r).is_err());
        let failed = r#"{"api":1,"command":"zram.worker","ok":false,"error":{"code":"insufficient_memory","message":"no memory"}}"#;
        assert_eq!(
            code_of(&decode(failed, false, &r).unwrap_err()),
            "insufficient_memory"
        );
        assert!(decode(failed, true, &r).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn timeout_terminates_the_worker_group_without_an_unbounded_wait() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("sh")
            .args(["-c", "exec sleep 10"])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut worker = Worker(child);
        let start = Instant::now();
        assert_eq!(
            code_of(&wait(&mut worker.0, Duration::from_millis(1)).unwrap_err()),
            "operation_timeout"
        );
        drop(worker);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
