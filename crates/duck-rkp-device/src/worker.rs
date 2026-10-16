use std::{
    fs,
    io::Read as _,
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use duck_core::AppPaths;
use serde::{Deserialize, Serialize};

use crate::{
    error::DeviceRkpError,
    protocol::{self, BUFFER_SIZE, MAX_REQUEST_SIZE},
};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REPLY_FILE: usize = BUFFER_SIZE * 6;
// Bionic reads this before main and permits these directories in the worker's
// default namespace. Keeping the default namespace retains system dependency links
// (e.g. libcrypto), which the exported sphal namespace may not provide on a ROM.
// Only this child receives the setting; the parent's loader state is untouched.
const VENDOR_LIBRARY_PATH: &str =
    "/system/lib64:/vendor/lib64:/odm/lib64:/vendor/lib64/hw:/odm/lib64/hw";

#[derive(Serialize, Deserialize)]
enum Reply {
    Payload(Vec<u8>),
    Error(DeviceRkpError),
}

impl Reply {
    fn from_result(result: Result<Vec<u8>>) -> Self {
        match result {
            Ok(payload) => Self::Payload(payload),
            Err(error) => Self::Error(
                error
                    .downcast::<DeviceRkpError>()
                    .unwrap_or_else(|error| DeviceRkpError::Transport(format!("{error:#}"))),
            ),
        }
    }

    fn into_result(self) -> Result<Vec<u8>> {
        match self {
            Self::Payload(payload) if payload.len() <= BUFFER_SIZE => Ok(payload),
            Self::Payload(_) => {
                Err(DeviceRkpError::InvalidResponse("payload too large".into()).into())
            }
            Self::Error(error) => Err(error.into()),
        }
    }
}

/// Vendor calls cannot be interrupted safely inside the CLI process. A child owns
/// the session, stdout is discarded, and the parent kills and reaps it on timeout.
pub(crate) fn sign(paths: &AppPaths, request: &[u8]) -> Result<Vec<u8>> {
    // Anything around the child process is a local transport failure, not an internal
    // error: the WebUI has a message for `ta_transport_error` and none for the latter.
    let local = |error: anyhow::Error| -> anyhow::Error {
        if error.is::<DeviceRkpError>() {
            error
        } else {
            DeviceRkpError::Transport(format!("device RKP worker failed: {error:#}")).into()
        }
    };
    fs::create_dir_all(&paths.tmp_dir)
        .with_context(|| format!("create {}", paths.tmp_dir.display()))
        .map_err(local)?;
    let exchange = tempfile::Builder::new()
        .prefix("rkp-device-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&paths.tmp_dir)
        .context("create the worker exchange directory")
        .map_err(local)?;
    fs::write(exchange.path().join("request.bin"), request)
        .context("write the worker request")
        .map_err(local)?;
    let executable = std::env::current_exe()
        .context("locate this executable")
        .map_err(local)?;
    let mut command = Command::new(&executable);
    command
        .env("LD_LIBRARY_PATH", VENDOR_LIBRARY_PATH)
        .args(["rkp-device", "ta-csr", "--exchange"])
        .arg(exchange.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach_from_terminal(&mut command);
    let child = command
        .spawn()
        .with_context(|| format!("start the KeyMint worker ({})", executable.display()))
        .map_err(local)?;
    let mut worker = Worker(child);
    let status = worker.wait(TIMEOUT)?;
    let reply_path = exchange.path().join("reply.json");
    if !status.success() || !reply_path.is_file() {
        return Err(DeviceRkpError::Transport(format!(
            "worker exited with {status}; vendor ABI may be incompatible"
        ))
        .into());
    }
    let reply = read_exchange_file(&reply_path, MAX_REPLY_FILE).map_err(local)?;
    serde_json::from_slice::<Reply>(&reply)
        .context("parse the worker reply")
        .map_err(|error| DeviceRkpError::InvalidResponse(format!("{error:#}")))?
        .into_result()
}

/// Puts the child in its own session, so a Ctrl-C aimed at the CLI (or anything else
/// signalling the process group) cannot interrupt the vendor call halfway through. The
/// null stdio above leaves the new session with nothing open on the terminal.
///
/// There is no safe API for `setsid` between fork and exec.
#[cfg(unix)]
#[allow(unsafe_code)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;

    // SAFETY: `setsid` is async-signal-safe and one of the few calls documented as safe
    // between fork and exec. A failure aborts the child before it touches vendor code.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_from_terminal(_command: &mut Command) {}

pub(crate) fn execute(exchange: &Path) -> Result<()> {
    let result = (|| {
        let request = read_exchange_file(&exchange.join("request.bin"), MAX_REQUEST_SIZE)?;
        protocol::ta_payload(&request)?;
        native_request(&request)
    })();
    let reply = Reply::from_result(result);
    let encoded = serde_json::to_vec(&reply).context("encode the worker reply")?;
    fs::write(exchange.join("reply.json"), encoded)
        .context("write the worker reply")
        .map_err(|error| DeviceRkpError::InvalidResponse(format!("{error:#}")))?;
    Ok(())
}

/// Read one regular exchange file through the same descriptor used for its size
/// check. The read itself is bounded too, even if the file grows after metadata().
fn read_exchange_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("stat {}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(DeviceRkpError::InvalidResponse(format!(
            "{} must be a regular file of at most {limit} bytes",
            path.display()
        ))
        .into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    if bytes.len() > limit {
        return Err(DeviceRkpError::InvalidResponse(
            "worker exchange file grew beyond its limit".into(),
        )
        .into());
    }
    Ok(bytes)
}

fn native_request(request: &[u8]) -> Result<Vec<u8>> {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        crate::keymint::request(request)
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = request;
        Err(DeviceRkpError::UnsupportedDevice("Android arm64 required".into()).into())
    }
}

struct Worker(Child);

impl Worker {
    fn wait(&mut self, timeout: Duration) -> Result<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().map_err(|error| {
                DeviceRkpError::Transport(format!("wait for the KeyMint worker: {error}"))
            })? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(DeviceRkpError::Timeout.into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn exchange_reads_are_bounded_and_reject_nonregular_files() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("request.bin");
        fs::write(&file, [1, 2, 3]).unwrap();
        assert_eq!(read_exchange_file(&file, 3).unwrap(), [1, 2, 3]);
        let error = read_exchange_file(&file, 2).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "invalid_ta_response");
        assert!(read_exchange_file(dir.path(), 100).is_err());
        let link = dir.path().join("link.bin");
        symlink(&file, &link).unwrap();
        assert!(read_exchange_file(&link, 100).is_err());
    }

    #[test]
    fn worker_errors_keep_their_type_across_the_process_boundary() {
        for error in [
            DeviceRkpError::Rejected(-18),
            DeviceRkpError::Rejected(-10001),
            DeviceRkpError::InvalidResponse("invalid payload length".into()),
            DeviceRkpError::UnsupportedDevice("Android arm64 required".into()),
            DeviceRkpError::Transport("library cannot be loaded".into()),
        ] {
            let expected_message = error.to_string();
            let error: anyhow::Error = error.into();
            let expected_code = crate::error::code_of(&error);
            let reply = Reply::from_result(Err(error.context("vendor operation")));
            let encoded = serde_json::to_vec(&reply).unwrap();
            let decoded: Reply = serde_json::from_slice(&encoded).unwrap();
            let returned = decoded.into_result().unwrap_err();
            assert_eq!(crate::error::code_of(&returned), expected_code);
            assert_eq!(returned.to_string(), expected_message);
        }
    }

    #[test]
    fn untyped_worker_errors_preserve_their_diagnostics() {
        let error = anyhow::anyhow!("dependency missing").context("load vendor library");
        let returned = Reply::from_result(Err(error)).into_result().unwrap_err();
        assert_eq!(crate::error::code_of(&returned), "ta_transport_error");
        assert!(
            returned
                .to_string()
                .contains("load vendor library: dependency missing")
        );
    }

    #[test]
    fn worker_reply_payloads_are_bounded() {
        assert_eq!(
            Reply::Payload(vec![1, 2, 3]).into_result().unwrap(),
            [1, 2, 3]
        );
        let error = Reply::Payload(vec![0; BUFFER_SIZE + 1])
            .into_result()
            .unwrap_err();
        assert_eq!(crate::error::code_of(&error), "invalid_ta_response");
    }

    /// A Ctrl-C aimed at the CLI reaches the whole foreground process group, so the worker
    /// must leave it or the vendor call would be interrupted halfway through. `setsid`
    /// that succeeds makes the child its own session *and* group leader, which `ps` can
    /// report on every platform this runs on.
    #[test]
    fn the_worker_leaves_the_callers_process_group() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 30"]);
        detach_from_terminal(&mut command);
        let child = command.spawn().unwrap();
        let pid = child.id();
        let _reaper = Worker(child);

        let listing = Command::new("ps")
            .args(["-o", "pgid=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let group = String::from_utf8_lossy(&listing.stdout).trim().to_owned();

        assert_eq!(
            group,
            pid.to_string(),
            "the worker must lead its own process group"
        );
    }

    /// A setup failure must use the transport error code.
    #[test]
    fn a_broken_exchange_directory_is_a_transport_error() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the temporary directory should be.
        let blocked = dir.path().join("tmp");
        fs::write(&blocked, b"not a directory").unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let paths = AppPaths {
            tmp_dir: blocked,
            ..paths
        };

        let error = sign(&paths, &[0x22, 0x31, 0x00, 0x00]).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "ta_transport_error");
        assert!(error.to_string().contains("worker failed"));
    }

    /// A worker that exits cleanly without a reply is a transport failure too.
    #[test]
    fn a_worker_that_writes_no_reply_is_a_transport_error() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        fs::create_dir_all(&paths.tmp_dir).unwrap();

        // The real executable rejects this request before it ever writes a reply, which is
        // exactly the "worker exited without a reply" path.
        let error = sign(&paths, &[0x22, 0x31, 0x00, 0x00]).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "ta_transport_error");
    }

    #[test]
    fn hung_worker_times_out_and_is_reaped() {
        let child = Command::new("sh")
            .args(["-c", "exec sleep 30"])
            .spawn()
            .unwrap();
        let pid = child.id();
        {
            let mut worker = Worker(child);
            let error = worker.wait(Duration::from_millis(10)).unwrap_err();
            assert_eq!(crate::error::code_of(&error), "ta_timeout");
        }
        let output = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}
