use std::{
    io::{Read, Seek, SeekFrom},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use duck_core::{Context, envelope::API_VERSION};
use serde::Deserialize;

use crate::{
    ModuleError, inventory,
    mutation::{self, OperationResult, Request},
};

struct WorkerProcess(Child);

impl WorkerProcess {
    fn wait(&mut self, budget: Duration) -> Result<ExitStatus> {
        let deadline = Instant::now() + budget;
        loop {
            if let Some(status) = self
                .0
                .try_wait()
                .map_err(|e| ModuleError::OutcomeUnknown(e.to_string()))?
            {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(ModuleError::Timeout.into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn terminate(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        // Reap ordinary exits without waiting indefinitely for an uninterruptible syscall.
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        self.terminate()
    }
}

pub fn execute(ctx: &Context, request: &Request) -> Result<OperationResult> {
    request.check(ctx)?;
    let directory = ctx.paths.var_dir.join("kernel-modules");
    mutation::private_dir(&directory)?;
    // A regular private input file avoids a blocking pipe write before the timeout starts.
    let mut input = tempfile::tempfile_in(&directory)?;
    serde_json::to_writer(&mut input, request)?;
    input.seek(SeekFrom::Start(0))?;
    let mut output = tempfile::tempfile_in(&directory)?;
    let child = Command::new(std::env::current_exe()?)
        .args(["kernel-modules", "worker", "--stdin-json"])
        .stdin(input)
        .stdout(output.try_clone()?)
        .stderr(Stdio::null())
        .spawn()
        .context("start kernel module worker")?;
    let mut worker = WorkerProcess(child);
    // Terminating userspace cannot undo or reliably interrupt a kernel init/exit routine.
    let exit = worker.wait(Duration::from_secs(30))?;
    output
        .seek(SeekFrom::Start(0))
        .map_err(|e| ModuleError::OutcomeUnknown(e.to_string()))?;
    let mut raw = String::new();
    output
        .take(65_537)
        .read_to_string(&mut raw)
        .map_err(|e| ModuleError::OutcomeUnknown(e.to_string()))?;
    ensure!(
        raw.len() <= 65_536,
        ModuleError::OutcomeUnknown("worker response exceeds limit".into())
    );
    decode_response(&raw, exit.success(), request)
}

#[derive(Deserialize)]
struct WorkerError {
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    api: u32,
    command: String,
    data: Option<OperationResult>,
    error: Option<WorkerError>,
}

fn decode_response(
    raw: &str,
    exited_successfully: bool,
    expected: &Request,
) -> Result<OperationResult> {
    let reply: Reply =
        serde_json::from_str(raw).map_err(|e| ModuleError::OutcomeUnknown(e.to_string()))?;
    ensure!(
        reply.api == API_VERSION && reply.command == "kernel-modules.worker",
        ModuleError::OutcomeUnknown("unexpected worker protocol".into())
    );
    if !reply.ok {
        let error = reply
            .error
            .ok_or_else(|| ModuleError::OutcomeUnknown("missing failure details".into()))?;
        ensure!(
            reply.data.is_none() && !error.code.is_empty() && !error.message.is_empty(),
            ModuleError::OutcomeUnknown("inconsistent failure response".into())
        );
        return Err(ModuleError::WorkerFailure {
            code: error.code,
            message: error.message,
        }
        .into());
    }
    let data = reply
        .data
        .ok_or_else(|| ModuleError::OutcomeUnknown("missing operation result".into()))?;
    ensure!(
        exited_successfully
            && reply.error.is_none()
            && data.action == expected.action()
            && match expected {
                Request::Unload(request) => data.name == request.name,
                Request::Load(_) => true,
            }
            && inventory::valid_name(&data.name),
        ModuleError::OutcomeUnknown("inconsistent operation result or abnormal worker exit".into())
    );
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::{WorkerProcess, decode_response};
    use crate::{
        code_of,
        mutation::{LoadRequest, Request, UnloadRequest},
    };

    fn load_request() -> Request {
        Request::Load(LoadRequest {
            path: "/tmp/sample.ko".into(),
            sha256: "0".repeat(64),
            parameters: String::new(),
            confirmed: true,
        })
    }

    fn unload_request(name: &str) -> Request {
        Request::Unload(UnloadRequest {
            name: name.into(),
            confirmed: true,
        })
    }

    #[test]
    fn absent_and_inconsistent_worker_results_preserve_unknown_outcome() {
        for raw in [
            "",
            "{",
            "{}",
            r#"{"command":"kernel-modules.worker","api":1,"ok":true}"#,
            r#"{"command":"kernel-modules.worker","api":2,"ok":true,"data":{"name":"sample","action":"load","observed_loaded":true}}"#,
            r#"{"command":"kernel-modules.worker","api":1,"ok":false,"error":{}}"#,
        ] {
            assert_eq!(
                code_of(&decode_response(raw, false, &load_request()).unwrap_err()),
                "operation_unknown"
            );
        }
        let reply = r#"{"command":"kernel-modules.worker","api":1,"ok":true,"data":{"name":"sample","action":"load","observed_loaded":true}}"#;
        assert_eq!(
            decode_response(reply, true, &load_request()).unwrap().name,
            "sample"
        );
        assert_eq!(
            code_of(&decode_response(reply, false, &load_request()).unwrap_err()),
            "operation_unknown"
        );
        assert_eq!(
            code_of(&decode_response(reply, true, &unload_request("sample")).unwrap_err()),
            "operation_unknown"
        );
    }

    #[test]
    fn worker_result_must_match_the_requested_removal_target() {
        let reply = r#"{"command":"kernel-modules.worker","api":1,"ok":true,"data":{"name":"other","action":"unload","observed_loaded":false}}"#;
        assert_eq!(
            code_of(&decode_response(reply, true, &unload_request("sample")).unwrap_err()),
            "operation_unknown"
        );
        assert_eq!(
            decode_response(reply, true, &unload_request("other"))
                .unwrap()
                .name,
            "other"
        );
    }

    #[test]
    fn worker_rejection_keeps_its_original_code_and_reason() {
        let reply = r#"{"command":"kernel-modules.worker","api":1,"ok":false,"error":{"code":"file_changed","message":"module file changed"}}"#;
        let error = decode_response(reply, false, &load_request()).unwrap_err();
        assert_eq!(code_of(&error), "file_changed");
        assert_eq!(error.to_string(), "module file changed");
    }

    #[test]
    #[cfg(unix)]
    fn timeout_remains_bounded_and_an_interruptible_worker_is_reaped() {
        let child = std::process::Command::new("sh")
            .args(["-c", "exec sleep 10"])
            .spawn()
            .unwrap();
        let mut worker = WorkerProcess(child);
        let start = std::time::Instant::now();
        assert_eq!(
            code_of(
                &worker
                    .wait(std::time::Duration::from_millis(5))
                    .unwrap_err()
            ),
            "operation_timeout"
        );
        worker.terminate();
        assert!(worker.0.try_wait().unwrap().is_some());
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
}
