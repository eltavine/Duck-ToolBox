use crate::document::{self, EMPTY, MAX_CONFIG, inspect, revision, snapshot};
use crate::error::Error;
use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct Paths {
    root: PathBuf,
    data: PathBuf,
}
struct TransactionLock(File);
impl Drop for TransactionLock {
    fn drop(&mut self) {
        // A fork may retain this descriptor after the transaction has finished.
        // Release ownership explicitly instead of waiting for every copy to close.
        let _ = self.0.unlock();
    }
}
impl Paths {
    fn discover() -> Result<Self> {
        let root =
            PathBuf::from(std::env::var_os("DEVICE_FAKER_SYSROOT").unwrap_or_else(|| "/".into()));
        let data = root.join("data/adb/duck-device-faker");
        if !root.is_absolute() {
            bail!("sysroot must be absolute");
        }
        Ok(Self { root, data })
    }
    fn file(&self, name: &str) -> PathBuf {
        self.data.join(name)
    }
    fn module(&self, id: &str) -> PathBuf {
        self.root.join("data/adb/modules").join(id)
    }
    fn active(&self, id: &str) -> bool {
        let dir = self.module(id);
        dir.join("module.prop").is_file()
            && !dir.join("disable").exists()
            && !dir.join("remove").exists()
    }
    fn content(&self) -> Result<String> {
        Ok(read(&self.file("config/config.toml"), MAX_CONFIG)?.unwrap_or_else(|| EMPTY.into()))
    }
    fn stop_revision(&self) -> Result<String> {
        Ok(read(&self.file("stop"), 256)?.unwrap_or_else(|| "initial".into()))
    }
    fn enabled(&self) -> Result<bool> {
        Ok(read(&self.file("enabled"), 256)?.is_some_and(|token| {
            !token.is_empty() && self.stop_revision().is_ok_and(|stop| stop == token)
        }))
    }
    fn lock(&self) -> Result<TransactionLock> {
        private_dir(&self.data)?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.file("lock"))?;
        lock.try_lock()
            .context("another Device Faker write is running")?;
        Ok(TransactionLock(lock))
    }
}
fn read(path: &Path, limit: usize) -> Result<Option<String>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("file exceeds {limit} bytes: {}", path.display());
    }
    Ok(Some(String::from_utf8(bytes)?))
}
fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().context("missing parent directory")?;
    private_dir(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn required<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key]
        .as_str()
        .with_context(|| format!("missing string: {key}"))
}
fn matches_revision(input: &Value, content: &str) -> Result<()> {
    if required(input, "revision")? != revision(content) {
        bail!(Error::new(
            "configuration_changed",
            "Saved configuration changed; preview again"
        ));
    }
    Ok(())
}
fn status(paths: &Paths) -> Result<Value> {
    let config = snapshot(&paths.content()?);
    let installed = paths.module("duck-toolbox");
    let boot = read(&paths.root.join("proc/sys/kernel/random/boot_id"), 64)
        .ok()
        .flatten();
    let startup_failed = read(&paths.file("startup-failed"), 64)?
        .is_some_and(|failed| boot.as_ref().is_some_and(|boot| failed == boot.trim()));
    // Diagnostic damage must not prevent status reads or affect the activation gate.
    let startup_error = startup_failed
        .then(|| {
            let diagnostic: Value =
                serde_json::from_str(&read(&paths.file("startup-error.json"), 16 * 1024).ok()??)
                    .ok()?;
            (diagnostic["boot"].as_str()? == boot.as_ref()?.trim())
                .then(|| diagnostic["message"].as_str().map(str::to_owned))?
        })
        .flatten();
    Ok(json!({
        "enabled": paths.enabled()?, "stop_revision": paths.stop_revision()?,
        "cleanup_pending": paths.file("cleanup-failed").exists() || (!paths.enabled()? && paths.file("density.json").exists()) || (crate::global_marker::path(&paths.root).exists() && !crate::gate::enabled_at(&paths.root)),
        "global_recovery_requires_reboot": crate::global_marker::requires_reboot(&paths.root),
        "parent_active": paths.active("duck-toolbox"),
        "startup_ready": crate::gate::startup_ready_at(&paths.root),
        "startup_failed": startup_failed, "startup_error": startup_error,
        "payload_installed": installed.join("zygisk/arm64-v8a.so").is_file(),
        "payload_active": paths.active("duck-toolbox"),
        "payload_pending": paths.root.join("data/adb/modules_update/duck-toolbox/module.prop").is_file(),
        "conflict": crate::gate::standalone_conflict_at(&paths.root), "injection_verified": false,
        "legacy_available": paths.root.join("data/adb/device_faker/config/config.toml").is_file(),
        "config": config, "upstream_revision": include_str!("../../UPSTREAM_REVISION").trim(),
    }))
}
pub fn setup() -> Result<Value> {
    let paths = Paths::discover()?;
    let _lock = paths.lock()?;
    if paths.file("ready").exists() {
        fs::remove_file(paths.file("ready"))?;
    }
    if !paths.file("config/config.toml").exists() {
        write(&paths.file("config/config.toml"), EMPTY)?;
    }
    Ok(json!({"ready": true}))
}
pub fn blocked() -> Result<Value> {
    blocked_at(&Paths::discover()?)
}
fn blocked_at(paths: &Paths) -> Result<Value> {
    if paths.file("ready").exists() {
        fs::remove_file(paths.file("ready"))?;
    }
    let boot = read(&paths.root.join("proc/sys/kernel/random/boot_id"), 64)?
        .context("boot identity unavailable")?;
    write(&paths.file("startup-failed"), boot.trim())?;
    Ok(json!({"startup_ready":false}))
}

pub fn ready() -> Result<Value> {
    ready_at(&Paths::discover()?)
}
fn ready_at(paths: &Paths) -> Result<Value> {
    let boot = read(&paths.root.join("proc/sys/kernel/random/boot_id"), 64)?
        .context("boot identity unavailable")?;
    if !paths.active("duck-toolbox") {
        bail!("parent module is not active");
    }
    crate::density::recover(&paths.root, &mut crate::density::AndroidBridge)?;
    let token = format!("{}:{}", boot.trim(), crate::gate::BUILD);
    write(&paths.file("ready"), &token)?;
    if paths.file("startup-failed").exists() {
        fs::remove_file(paths.file("startup-failed"))?;
    }
    let _ = fs::remove_file(paths.file("startup-error.json"));
    Ok(json!({"startup_ready":true}))
}

pub fn boot() -> Result<Value> {
    let paths = Paths::discover()?;
    run_boot(&paths, || {
        crate::boot::stages(&paths.module("duck-toolbox"))
    })
}
fn run_boot(paths: &Paths, stages: impl FnOnce() -> Result<()>) -> Result<Value> {
    private_dir(&paths.data)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.file("boot.lock"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => return Ok(json!({"boot_in_progress":true})),
        Err(error) => return Err(error.into()),
    }
    let _lock = TransactionLock(lock);
    if crate::gate::startup_ready_at(&paths.root) {
        return Ok(json!({"startup_ready":true,"already_completed":true}));
    }
    let result = (|| {
        // Remove stale readiness before any global mutation or recovery.
        if paths.file("ready").exists() {
            fs::remove_file(paths.file("ready"))?;
        }
        crate::density::recover(&paths.root, &mut crate::density::AndroidBridge)
            .context("display-density recovery failed")?;
        crate::global_marker::prepare_boot(&paths.root)
            .context("global-property ownership check failed")?;
        stages()?;
        // Direct nonpersistent properties are recreated by init on each boot.
        // A same-boot failure remains owned by its companion and must not be cleared here.
        let boot = read(&paths.root.join("proc/sys/kernel/random/boot_id"), 64)?
            .context("boot identity unavailable")?;
        if read(&paths.file("cleanup-failed"), 256)?.is_some_and(|marker| {
            marker
                .split_once(':')
                .is_some_and(|(owner, _)| !owner.is_empty() && owner != boot.trim())
        }) {
            fs::remove_file(paths.file("cleanup-failed"))?;
        }
        ready_at(paths)
    })();
    if let Err(error) = &result {
        blocked_at(paths)?;
        // Keep a bounded, boot-scoped cause even when the boot hook discards stdout.
        // Recording diagnostics is best effort; the original failure remains authoritative.
        let message: String = format!("{error:#}").chars().take(2048).collect();
        if let Ok(Some(boot)) = read(&paths.file("startup-failed"), 64) {
            let _ = write(
                &paths.file("startup-error.json"),
                &json!({"boot":boot,"message":message}).to_string(),
            );
        }
    }
    result
}

pub fn uninstall() -> Result<Value> {
    uninstall_at(&Paths::discover()?)
}
fn uninstall_at(paths: &Paths) -> Result<Value> {
    stop(paths)?;
    crate::density::recover(&paths.root, &mut crate::density::AndroidBridge)?;
    if paths.file("cleanup-failed").exists() || crate::global_marker::path(&paths.root).exists() {
        bail!(Error::new(
            "recovery_pending",
            "Global cleanup remains pending during uninstall"
        ));
    }
    // Runtime state is retained until the root manager has removed the native payload.
    Ok(json!({"enabled": false}))
}
fn stop(paths: &Paths) -> Result<Value> {
    private_dir(&paths.data)?;
    let token = tempfile::NamedTempFile::new_in(&paths.data)?;
    let token_name = token
        .path()
        .file_name()
        .context("missing stop token")?
        .to_string_lossy()
        .into_owned();
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let revision = format!("{token_name}-{clock:x}");
    write(&paths.file("stop"), &revision)?;
    let cleanup_pending = crate::density::recover(&paths.root, &mut crate::density::AndroidBridge)
        .is_err()
        || paths.file("cleanup-failed").exists()
        || crate::global_marker::path(&paths.root).exists();
    Ok(json!({"enabled": false, "stop_revision": revision,"cleanup_pending":cleanup_pending}))
}
pub fn request(operation: &str, input: &Value) -> Result<Value> {
    let paths = Paths::discover()?;
    if operation == "disable" {
        return stop(&paths);
    }
    if operation == "status" {
        return status(&paths);
    }
    if operation == "config" {
        return Ok(snapshot(&paths.content()?));
    }
    if operation == "import-preview" {
        let content = read(
            &paths.root.join("data/adb/device_faker/config/config.toml"),
            MAX_CONFIG,
        )?
        .context("upstream configuration does not exist")?;
        return preview(&paths, &content);
    }
    if operation == "preview" {
        return preview(&paths, required(input, "content")?);
    }
    if operation == "restore-preview" {
        let current = paths.content()?;
        let before = backup_content(&paths, &current)?;
        return preview(&paths, &before);
    }
    let _lock = paths.lock()?;
    match operation {
        "apply" => {
            let content = required(input, "content")?;
            let summary = inspect(content)?;
            if summary["global_effects"] == true && input["acknowledge_global"] != true {
                bail!(Error::new(
                    "global_ack_required",
                    "Global changes require acknowledgement"
                ));
            }
            let before = paths.content()?;
            matches_revision(input, &before)?;
            write(
                &paths.file("backup.json"),
                &json!({"before": before, "after": revision(content)}).to_string(),
            )?;
            write(&paths.file("config/config.toml"), content)?;
            Ok(snapshot(content))
        }
        "restore" => {
            let current = paths.content()?;
            matches_revision(input, &current)?;
            let before = backup_content(&paths, &current)?;
            if inspect(&before)?["global_effects"] == true && input["acknowledge_global"] != true {
                bail!(Error::new(
                    "global_ack_required",
                    "Global changes require acknowledgement"
                ));
            }
            write(&paths.file("config/config.toml"), &before)?;
            Ok(snapshot(&before))
        }
        "enable" => {
            if paths.file("cleanup-failed").exists()
                || paths.file("density.json").exists()
                || crate::global_marker::path(&paths.root).exists()
            {
                bail!(Error::new(
                    "recovery_pending",
                    "Global recovery is still pending"
                ));
            }
            let stop = paths.stop_revision()?;
            if stop.is_empty() || required(input, "stop_revision")? != stop {
                bail!(Error::new(
                    "power_changed",
                    "The stop state changed; refresh before enabling"
                ));
            }
            let content = paths.content()?;
            matches_revision(input, &content)?;
            if inspect(&content)?["targets"]
                .as_array()
                .is_none_or(|targets| targets.is_empty())
            {
                bail!(Error::new(
                    "no_targets",
                    "Configure an application before enabling"
                ));
            }
            if crate::gate::standalone_conflict_at(&paths.root) {
                bail!(Error::new(
                    "standalone_conflict",
                    "Disable standalone Device Faker and reboot first"
                ));
            }
            if !paths.active("duck-toolbox")
                || !crate::gate::startup_ready_at(&paths.root)
                || paths
                    .root
                    .join("data/adb/modules_update/duck-toolbox/module.prop")
                    .is_file()
                || !paths
                    .module("duck-toolbox")
                    .join("zygisk/arm64-v8a.so")
                    .is_file()
            {
                bail!(Error::new(
                    "startup_not_ready",
                    "The embedded engine is not ready; finish installation and reboot first"
                ));
            }
            write(&paths.file("enabled"), &stop)?;
            // A concurrent disable changes stop after this snapshot; the native gate compares both files.
            Ok(json!({"enabled": paths.enabled()?, "stop_revision": paths.stop_revision()?}))
        }
        _ => bail!("unsupported operation: {operation}"),
    }
}
fn backup_content(paths: &Paths, current: &str) -> Result<String> {
    let backup: Value = serde_json::from_str(
        &read(&paths.file("backup.json"), MAX_CONFIG * 2 + 4096)?.ok_or_else(|| {
            Error::new("backup_missing", "No previous configuration is available")
        })?,
    )?;
    if backup["after"] != revision(current) {
        bail!(Error::new(
            "backup_stale",
            "The backup belongs to another configuration"
        ));
    }
    let content = required(&backup, "before")?;
    inspect(content)?;
    Ok(content.into())
}

fn preview(paths: &Paths, content: &str) -> Result<Value> {
    let summary = document::inspect(content)?;
    Ok(json!({"content": content, "revision": revision(&paths.content()?), "summary": summary}))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
