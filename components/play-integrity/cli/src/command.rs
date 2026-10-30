// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: bounded local platform commands with isolated process groups.
// Modified 2026-10-31: retain waitable ownership until group cleanup precedes reaping.
use anyhow::{Result, bail};
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Child, Command, ExitStatus, Output, Stdio},
    time::{Duration, Instant},
};

// Each local operation owns a separate group, including descendants that ignore SIGTERM.
pub struct Process {
    child: Child,
    stopped: bool,
}
impl Process {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Ok(Self {
            child: command.spawn()?,
            stopped: false,
        })
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        if self.stopped {
            return self.child.try_wait();
        }
        #[cfg(unix)]
        {
            loop {
                // WNOWAIT retains the child's PID until descendants are stopped.
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.child.id(),
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result != 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    // An external reaper or SIGCHLD=SIG_IGN can release ownership.
                    self.stopped = true;
                    return Err(error);
                }
                if unsafe { info.si_pid() } == 0 {
                    return Ok(None);
                }
                self.kill_owned();
                let status = self.child.wait();
                self.stopped = true;
                return status.map(Some);
            }
        }
        #[cfg(not(unix))]
        {
            let status = self.child.try_wait()?;
            if status.is_some() {
                self.stopped = true;
            }
            Ok(status)
        }
    }
    pub fn terminate(&mut self) {
        if self.stopped {
            return;
        }
        // Poll before signalling so ECHILD never becomes a kill of a reused PID.
        if matches!(self.try_wait(), Ok(None)) {
            self.kill_owned();
            let _ = self.child.wait();
            self.stopped = true;
        }
    }
    fn kill_owned(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.terminate();
    }
}

pub fn run(command: &mut Command, timeout: Duration) -> Result<Output> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    let mut child = Process::spawn(command)?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if stdout.metadata()?.len() > 256 * 1024
            || stderr.metadata()?.len() > 256 * 1024
            || Instant::now() >= deadline
        {
            bail!("platform command exceeded its time or output limit");
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    child.terminate();
    let read = |file: &mut std::fs::File| -> Result<Vec<u8>> {
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 256 * 1024 {
            bail!("platform command output too large");
        }
        Ok(bytes)
    };
    Ok(Output {
        status,
        stdout: read(&mut stdout)?,
        stderr: read(&mut stderr)?,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn externally_reaped_children_never_leave_a_signalable_process_id() {
        const CHILD: &str = "PIF_TEST_EXTERNAL_REAPER";
        if std::env::var_os(CHILD).is_some() {
            unsafe {
                libc::signal(libc::SIGCHLD, libc::SIG_IGN);
            }
            let mut child = Process::spawn(Command::new("/bin/sh").args(["-c", "exit 0"])).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Err(error) => {
                        assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
                        assert!(child.stopped);
                        child.terminate();
                        break;
                    }
                    Ok(None) => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(Some(_)) => panic!("SIGCHLD=SIG_IGN must release the child's status"),
                }
            }
            return;
        }
        assert!(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "command::tests::externally_reaped_children_never_leave_a_signalable_process_id"
                ])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
    }
    #[test]
    fn a_blocked_platform_tool_does_not_block_shutdown_indefinitely() {
        let start = Instant::now();
        assert!(
            run(
                Command::new("sh").args(["-c", "sleep 30"]),
                Duration::from_millis(100)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
