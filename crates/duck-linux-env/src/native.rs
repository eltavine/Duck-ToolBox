//! Android permission and Linux process-identity boundary.
use crate::{fail, model::Identity};
use anyhow::{Result, ensure};
use duck_core::Context;
use std::{fs, path::Path};

#[cfg(any(target_os = "linux", target_os = "android"))]
#[path = "native_linux.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
#[path = "native_unsupported.rs"]
mod platform;
pub use platform::*;

pub fn supported(ctx: &Context) -> bool {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        ctx.sysroot.root() == Path::new("/") && unsafe { libc::geteuid() } == 0
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = ctx;
        false
    }
}
pub fn require(ctx: &Context) -> Result<()> {
    ensure!(
        supported(ctx),
        fail(
            "unsupported_environment",
            "changes require Android ARM64 root outside a test sysroot"
        )
    );
    Ok(())
}
pub fn parse_start(text: &str) -> Result<u64> {
    let tail = text
        .rsplit_once(") ")
        .ok_or_else(|| fail("state_unavailable", "invalid process stat"))?
        .1;
    tail.split_whitespace()
        .nth(19)
        .ok_or_else(|| fail("state_unavailable", "missing process start time"))?
        .parse()
        .map_err(Into::into)
}
pub fn boot() -> Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .into())
}
pub fn identity(pid: u32) -> Result<Identity> {
    ensure!(pid > 1, fail("state_unavailable", "invalid host PID"));
    Ok(Identity {
        pid,
        start: parse_start(&fs::read_to_string(format!("/proc/{pid}/stat"))?)?,
        boot: boot()?,
    })
}
pub fn alive(identity: &Identity) -> Result<bool> {
    ensure!(
        identity.pid > 1,
        fail("state_unavailable", "invalid host PID")
    );
    if identity.boot != boot()? {
        return Ok(false);
    }
    match fs::read_to_string(format!("/proc/{}/stat", identity.pid)) {
        Ok(text) => Ok(parse_start(&text)? == identity.start),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub fn check_lease(path: &Path) -> Result<()> {
    use std::os::{fd::AsFd, unix::fs::MetadataExt};
    let input = fs::File::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let actual = input.metadata()?;
    let expected = fs::symlink_metadata(path)?;
    ensure!(
        actual.is_file()
            && expected.is_file()
            && actual.dev() == expected.dev()
            && actual.ino() == expected.ino(),
        fail(
            "invalid_input",
            "worker requires an inherited operation lease"
        )
    );
    input.try_lock().map_err(|_| {
        fail(
            "operation_busy",
            "worker lease is owned by another operation",
        )
    })?;
    Ok(())
}
