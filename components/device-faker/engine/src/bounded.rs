//! Bounded platform process adapter shared by boot recovery and the native companion.
use anyhow::{Result, bail};
use std::{
    io::{Read, Seek},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub fn output(command: Command) -> Result<Vec<u8>> {
    output_with_timeout(command, Duration::from_secs(5))
}

pub fn output_with_timeout(command: Command, timeout: Duration) -> Result<Vec<u8>> {
    capture(command, timeout, terminate)
}

fn exited(child: &mut Child) -> std::io::Result<bool> {
    #[cfg(unix)]
    loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == 0 {
            return Ok(unsafe { info.si_pid() } != 0);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    #[cfg(not(unix))]
    child.try_wait().map(|status| status.is_some())
}

fn capture(
    mut command: Command,
    timeout: Duration,
    terminate_owned: impl FnOnce(&mut Child),
) -> Result<Vec<u8>> {
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
    let deadline = Instant::now() + timeout;
    let mut owned_child = true;
    let waited = (|| -> Result<_> {
        loop {
            if stdout.metadata()?.len() > 64 * 1024 || stderr.metadata()?.len() > 64 * 1024 {
                bail!("platform output exceeds 64 KiB");
            }
            match exited(&mut child) {
                Ok(true) => return Ok(()),
                Ok(false) => (),
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
    // An external reaper can release the PID. Never signal it after ECHILD.
    // A waitable child retains its identity until group cleanup and reaping finish.
    if owned_child && exited(&mut child).is_ok() {
        terminate_owned(&mut child);
    }
    waited?;
    let status = child.wait()?;
    if !status.success() {
        bail!("platform command failed with {status}");
    }
    if stdout.metadata()?.len() > 64 * 1024 || stderr.metadata()?.len() > 64 * 1024 {
        bail!("platform output exceeds 64 KiB");
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
    fn successful_commands_do_not_leave_background_writers() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("late-write");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "(sleep 0.3; touch \"$LATE_MARKER\") & printf done"])
            .env("LATE_MARKER", &marker);
        assert_eq!(output(command).unwrap(), b"done");
        std::thread::sleep(Duration::from_millis(500));
        assert!(!marker.exists());
    }
    #[test]
    fn a_custom_deadline_still_terminates_an_owned_blocked_command() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let mut terminated = false;
        assert!(
            capture(command, Duration::from_millis(50), |child| {
                terminated = true;
                terminate(child);
            })
            .is_err()
        );
        assert!(terminated);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
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
            let error = capture(command, Duration::from_secs(5), |_| {
                panic!("must not signal after ECHILD")
            })
            .unwrap_err();
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
