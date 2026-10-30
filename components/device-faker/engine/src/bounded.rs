//! Bounded platform process adapter shared by boot recovery and the native companion.
use anyhow::{Result, bail};
use std::{
    io::{Read, Seek},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub fn output(command: Command) -> Result<Vec<u8>> {
    capture(command, terminate)
}

fn capture(mut command: Command, terminate_owned: impl FnOnce(&mut Child)) -> Result<Vec<u8>> {
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
    let mut owned_child = true;
    let waited = (|| -> Result<_> {
        loop {
            if stdout.metadata()?.len() > 64 * 1024 || stderr.metadata()?.len() > 64 * 1024 {
                bail!("platform output exceeds 64 KiB");
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    owned_child = false;
                    return Ok(status);
                }
                Ok(None) => (),
                // Another reaper or SIGCHLD=SIG_IGN can release the PID. Never
                // signal a numeric PID/process group after ownership is lost.
                Err(error) => {
                    owned_child = false;
                    return Err(error.into());
                }
            }
            if Instant::now() >= deadline {
                bail!("platform command timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if owned_child {
        terminate_owned(&mut child);
    }
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

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(group) = i32::try_from(child.id())
        && group > 0
    {
        // The capture loop must still own an unreaped child before signalling.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix, not(target_os = "android")))]
mod tests {
    use super::*;
    #[test]
    fn an_externally_reaped_child_fails_without_signalling_an_unowned_pid() {
        const CHILD: &str = "DEVICE_FAKER_TEST_EXTERNAL_REAPER";
        if std::env::var_os(CHILD).is_some() {
            // Isolate signal disposition from every other test and JVM thread.
            unsafe {
                libc::signal(libc::SIGCHLD, libc::SIG_IGN);
            }
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "exit 0"]);
            let error = capture(command, |_| panic!("must not signal after ECHILD")).unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(libc::ECHILD)
            );
            return;
        }
        assert!(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bounded::tests::an_externally_reaped_child_fails_without_signalling_an_unowned_pid"])
            .env(CHILD, "1").status().unwrap().success());
    }
}
