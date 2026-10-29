//! A cgroup v2 fallback for Android kernels built without CONFIG_PID_NS.
use crate::{fail, model::valid_digest};
use anyhow::{Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
fn path(instance: &str) -> Result<PathBuf> {
    ensure!(
        valid_digest(instance),
        fail("state_unavailable", "invalid environment instance")
    );
    Ok(PathBuf::from("/sys/fs/cgroup").join(format!("duck-toolbox-linux-{instance}")))
}
pub fn populated(instance: &str) -> Result<bool> {
    let path = path(instance)?;
    if !path.try_exists()? {
        return Ok(false);
    }
    let events = fs::read_to_string(path.join("cgroup.events"))?;
    match events.lines().find_map(|l| l.strip_prefix("populated ")) {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(fail("state_unavailable", "invalid cgroup population state")),
    }
}
pub fn create(instance: &str) -> Result<File> {
    ensure!(
        !populated(instance)?,
        fail("operation_busy", "previous environment cgroup is populated")
    );
    let path = path(instance)?;
    if !path.try_exists()? {
        fs::create_dir(&path)?;
    }
    ensure!(
        path.join("cgroup.kill").is_file(),
        fail(
            "runtime_unavailable",
            "kernel requires PID namespaces or writable cgroup v2 with cgroup.kill"
        )
    );
    attach(instance)
}
pub fn attach(instance: &str) -> Result<File> {
    Ok(OpenOptions::new()
        .write(true)
        .open(path(instance)?.join("cgroup.procs"))?)
}
pub fn kill(instance: &str) -> Result<()> {
    if populated(instance)? {
        fs::write(path(instance)?.join("cgroup.kill"), b"1\n")?;
    }
    Ok(())
}
pub fn finish(instance: &str) -> Result<()> {
    kill(instance)?;
    let until = Instant::now() + Duration::from_secs(2);
    while populated(instance)? && Instant::now() < until {
        thread::sleep(Duration::from_millis(40));
    }
    ensure!(
        !populated(instance)?,
        fail(
            "operation_unknown",
            "environment cgroup still contains processes"
        )
    );
    remove(instance)
}
pub fn remove(instance: &str) -> Result<()> {
    let path = path(instance)?;
    if path.try_exists()? {
        ensure!(
            !populated(instance)?,
            fail("operation_busy", "environment cgroup is populated")
        );
        // Guest /sys is a non-recursive read-only bind; nested cgroups are not managed.
        fs::remove_dir(path)?;
    }
    Ok(())
}
