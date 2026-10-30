//! Bounded platform process adapter shared by boot recovery and the native companion.
use anyhow::{Result, bail};
use std::{
    io::{Read, Seek},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn output(mut command: Command) -> Result<Vec<u8>> {
    let mut stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let waited = (|| -> Result<_> {
        loop {
            if stdout.metadata()?.len() > 64 * 1024 || stderr.metadata()?.len() > 64 * 1024 {
                bail!("platform output exceeds 64 KiB");
            }
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                bail!("platform command timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    #[cfg(unix)]
    {
        if let Ok(group) = i32::try_from(child.id()) {
            // SAFETY: only the positive owned process-group id is negated.
            if group > 0 {
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let status = waited?;
    if !status.success() {
        bail!("platform command failed");
    }
    stdout.rewind()?;
    let mut bytes = Vec::new();
    stdout.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        bail!("platform output exceeds 64 KiB");
    }
    Ok(bytes)
}
