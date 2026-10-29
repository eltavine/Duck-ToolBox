use crate::{
    fail,
    model::{MAX_LOG, Runtime},
    mounts, native, scope, store,
};
use anyhow::{Result, ensure};
use duck_core::Context;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn run(ctx: &Context, id: &str, instance: &str) -> Result<()> {
    native::require(ctx)?;
    let base = store::base(ctx);
    let dir = store::env_dir(&base, id)?;
    native::check_lease(&dir.join("environment.lock"))?;
    let environment = store::environment(&base, id)?;
    ensure!(
        environment.instance == instance,
        fail("state_changed", "environment instance changed")
    );
    let path = dir.join("runtime.json");
    let queued: Runtime = store::load(&path)?;
    ensure!(
        queued.instance == instance && queued.phase == "starting" && queued.supervisor.is_none(),
        fail("state_changed", "start request no longer queued")
    );
    queued.config.validate()?;
    let mut runtime = Runtime {
        instance: instance.into(),
        supervisor: Some(native::identity(std::process::id())?),
        init: None,
        phase: "starting".into(),
        message: String::new(),
        pid_namespace: false,
        config: queued.config,
    };
    store::save(&path, &runtime)?;
    let result = (|| -> Result<()> {
        let helper = mounts::helper(&ctx.paths.root.join("bin/tini-arm64"))?;
        let signals = native::Signals::new()?;
        let pid_namespace = native::isolate()?;
        runtime.pid_namespace = pid_namespace;
        let scope = if pid_namespace {
            None
        } else {
            Some(scope::create(instance)?)
        };
        let root = dir.join("rootfs");
        mounts::setup(&root, &runtime.config, &helper, pid_namespace)?;
        let setup = native::ChildSetup::new(&root)?.in_scope(scope);
        let mut command = Command::new("/run/duck-toolbox/tini");
        if !pid_namespace {
            command.arg("-s");
        }
        command
            .args(["-g", "--", "/bin/sh", "/run/duck-toolbox/init.sh"])
            .env_clear()
            .env(
                "PATH",
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            )
            .env("HOME", "/root")
            .env("LANG", "C.UTF-8")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        prepare(&mut command, setup, pid_namespace);
        let mut child = command.spawn()?;
        let init_identity = native::identity(child.id())?;
        let init_fd = native::checked_pidfd(&init_identity)?;
        runtime.init = Some(init_identity);
        runtime.phase = "running".into();
        store::save(&path, &runtime)?;
        let logs = dir.join("logs");
        store::private_dir(&logs)?;
        let mut stdout = Capture::new(
            child
                .stdout
                .take()
                .ok_or_else(|| fail("state_unavailable", "missing stdout"))?,
            logs.join("stdout.log"),
        )?;
        let mut stderr = Capture::new(
            child
                .stderr
                .take()
                .ok_or_else(|| fail("state_unavailable", "missing stderr"))?,
            logs.join("stderr.log"),
        )?;
        let mut stopping = None;
        loop {
            stdout.drain()?;
            stderr.drain()?;
            if stopping.is_none() && signals.requested()? {
                runtime.phase = "stopping".into();
                store::save(&path, &runtime)?;
                native::signal(&init_fd, libc::SIGTERM)?;
                stopping = Some(Instant::now());
            }
            if stopping.is_some_and(|time: Instant| time.elapsed() >= Duration::from_secs(3))
                && !native::exited(&init_fd)
            {
                native::signal(&init_fd, libc::SIGKILL)?;
            }
            if let Some(status) = child.try_wait()? {
                stdout.drain_limit(MAX_LOG as usize)?;
                stderr.drain_limit(MAX_LOG as usize)?;
                ensure!(
                    stopping.is_some(),
                    fail(
                        "runtime_failed",
                        format!("guest init exited unexpectedly: {status}")
                    )
                );
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        runtime.phase = "stopped".into();
        Ok(())
    })();
    let result = match (result, scope::finish(instance)) {
        (Ok(()), cleanup) => cleanup,
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("guest cleanup failed: {cleanup:#}")))
        }
    };
    if let Err(e) = &result {
        runtime.phase = "failed".into();
        runtime.message = format!("{e:#}");
    }
    store::save(&path, &runtime)?;
    result
}
#[allow(unsafe_code)]
pub fn prepare(command: &mut Command, setup: native::ChildSetup, mount_proc: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(move || setup.apply(mount_proc));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (command, setup, mount_proc);
    }
}
struct Capture {
    reader: Box<dyn Read>,
    file: std::fs::File,
    total: u64,
}
impl Capture {
    fn new(
        reader: impl Read + std::os::fd::AsRawFd + 'static,
        path: std::path::PathBuf,
    ) -> Result<Self> {
        native::nonblocking(reader.as_raw_fd())?;
        let file = store::open(&path, true)?;
        file.set_len(0)?;
        Ok(Self {
            reader: Box::new(reader),
            file,
            total: 0,
        })
    }
    fn drain(&mut self) -> Result<()> {
        self.drain_limit(131072)
    }
    fn drain_limit(&mut self, budget: usize) -> Result<()> {
        let mut bytes = [0u8; 8192];
        let mut read = 0;
        while read < budget {
            match self.reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    let keep = (MAX_LOG.saturating_sub(self.total) as usize).min(count);
                    self.file.write_all(&bytes[..keep])?;
                    self.total = self.total.saturating_add(count as u64);
                    read += count;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_output_is_retained_up_to_the_cap_without_unbounded_reads() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("log");
        let mut output = vec![b'a'; 100000];
        output.extend_from_slice(b"last line\n");
        let mut capture = Capture {
            reader: Box::new(std::io::Cursor::new(output.clone())),
            file: store::open(&path, true).unwrap(),
            total: 0,
        };
        capture.drain_limit(MAX_LOG as usize).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), output);
        capture.reader = Box::new(std::io::Cursor::new(vec![b'b'; 2 * MAX_LOG as usize]));
        for _ in 0..20 {
            capture.drain().unwrap();
        }
        assert_eq!(std::fs::metadata(path).unwrap().len(), MAX_LOG);
        assert!(capture.total > MAX_LOG);
    }
}
