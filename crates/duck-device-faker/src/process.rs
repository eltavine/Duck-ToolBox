//! Bounded process boundary for the independent engine's public protocol.
use anyhow::{Context as _, Result, bail};
use duck_core::Context;
use serde_json::Value;
use std::{
    io::{Read, Seek, Write},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct EngineError(pub Value);
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0["message"].as_str().unwrap_or("engine failed")
        )
    }
}
impl std::error::Error for EngineError {}

const OUTPUT_LIMIT: u64 = 2 * 1024 * 1024;

#[cfg(unix)]
#[allow(unsafe_code)]
fn kill_owned_group(id: u32) {
    let Ok(id) = i32::try_from(id) else {
        return;
    };
    if id <= 0 {
        return;
    }
    // SAFETY: kill takes no pointers. Only a positive, owned process-group ID
    // reaches this call; negation never signals this process's group or all PIDs.
    unsafe {
        libc::kill(-id, libc::SIGKILL);
    }
}

fn exchange(
    mut command: Command,
    input: &[u8],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    if input.len() > 128 * 1024 {
        bail!("engine input exceeds 128 KiB");
    }
    // Anonymous files keep stdin writes and output collection independent of pipe
    // capacity and EOF held by descendants. No request data remains at a named path.
    let mut request = tempfile::tempfile()?;
    request.write_all(input)?;
    request.rewind()?;
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::from(request))
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()?;
    let deadline = Instant::now() + timeout;
    let waited = (|| -> Result<ExitStatus> {
        loop {
            if stdout.metadata()?.len() > OUTPUT_LIMIT || stderr.metadata()?.len() > OUTPUT_LIMIT {
                bail!("engine output exceeds 2 MiB");
            }
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                bail!(
                    "engine request timed out after {} seconds",
                    timeout.as_secs()
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    // Background jobs explicitly create their own groups. Reap only descendants
    // belonging to this exchange, including when the engine exits before them.
    #[cfg(unix)]
    kill_owned_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
    let status = waited?;
    let capture = |file: &mut std::fs::File| -> Result<Vec<u8>> {
        file.rewind()?;
        let mut bytes = Vec::new();
        file.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            bail!("engine output exceeds 2 MiB");
        }
        Ok(bytes)
    };
    Ok((status, capture(&mut stdout)?, capture(&mut stderr)?))
}
pub fn request(ctx: &Context, operation: &str, input: &Value) -> Result<Value> {
    let module = ctx.paths.root.join("device-faker");
    let executable = module.join("device-faker-engine");
    let mut command = Command::new(&executable);
    command
        .args(["request", operation])
        .env("DEVICE_FAKER_SYSROOT", ctx.sysroot.root());
    let (status, stdout, stderr) = exchange(
        command,
        &serde_json::to_vec(input)?,
        Duration::from_secs(15),
    )
    .with_context(|| format!("engine {}", executable.display()))?;
    let response: Value = serde_json::from_slice(&stdout).with_context(|| {
        format!(
            "invalid engine response: {}",
            String::from_utf8_lossy(&stderr)
        )
    })?;
    if response["api"] != 1 {
        bail!("unsupported engine protocol version");
    }
    if response["ok"] != true {
        return Err(EngineError(response["error"].clone()).into());
    }
    if !status.success() {
        bail!("engine exited with {status}");
    }
    response
        .get("data")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("engine response is missing data"))
}
