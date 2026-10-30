//! Durable, explicit-user display-density ownership and recovery.
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Value {
    pub user: u32,
    #[serde(rename = "override")]
    pub density: Option<u32>,
}
#[derive(Serialize, Deserialize)]
struct Lease {
    before: Value,
    after: u32,
}

pub trait Bridge {
    fn read(&mut self, user: Option<u32>) -> Result<Value>;
    fn set(&mut self, user: u32, density: Option<u32>) -> Result<()>;
}
pub struct AndroidBridge;
impl Bridge for AndroidBridge {
    fn read(&mut self, user: Option<u32>) -> Result<Value> {
        let mut args = vec!["read".to_string()];
        if let Some(user) = user {
            args.push(user.to_string());
        }
        invoke(&args)
    }
    fn set(&mut self, user: u32, density: Option<u32>) -> Result<()> {
        let value = invoke(&[
            "set".into(),
            user.to_string(),
            density.map_or("reset".into(), |v| v.to_string()),
        ])?;
        if value.user != user || value.density != density {
            bail!("density readback differs from request");
        }
        Ok(())
    }
}
fn invoke(args: &[String]) -> Result<Value> {
    if !cfg!(target_os = "android") {
        bail!("density changes require Android");
    }
    let mut command = Command::new("/system/bin/app_process");
    command
        .env(
            "CLASSPATH",
            "/data/adb/modules/duck-toolbox/play-integrity/classes.dex",
        )
        .args(["/system/bin", "dev.duck.toolbox.devicefaker.DensityBridge"])
        .args(args);
    let output = crate::bounded::output(command)?;
    let envelope: serde_json::Value = serde_json::from_slice(&output)?;
    if envelope["ok"] != true {
        bail!("density bridge failed");
    }
    Ok(serde_json::from_value(envelope["data"].clone())?)
}
fn paths(root: &Path) -> (PathBuf, PathBuf) {
    let data = root.join("data/adb/duck-device-faker");
    (data.join("density.json"), data.join("density.lock"))
}
fn lock(path: &Path) -> Result<File> {
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().context("density recovery is busy")?;
    Ok(file)
}
fn load(path: &Path) -> Result<Option<Lease>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        bail!("invalid density recovery journal");
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}
fn restore_locked(path: &Path, bridge: &mut impl Bridge) -> Result<()> {
    let Some(lease) = load(path)? else {
        return Ok(());
    };
    let current = bridge.read(Some(lease.before.user))?;
    // Preserve a later settings edit, including changes made by another root tool.
    if current.user == lease.before.user && current.density == Some(lease.after) {
        bridge.set(lease.before.user, lease.before.density)?;
    }
    fs::remove_file(path)?;
    File::open(path.parent().context("journal parent missing")?)?.sync_all()?;
    Ok(())
}
pub fn recover(root: &Path, bridge: &mut impl Bridge) -> Result<()> {
    let (path, lock_path) = paths(root);
    if !path.exists() {
        return Ok(());
    }
    let _lock = lock(&lock_path)?;
    restore_locked(&path, bridge)
}
pub fn apply(root: &Path, density: u32, bridge: &mut impl Bridge) -> Result<()> {
    let (path, lock_path) = paths(root);
    let _lock = lock(&lock_path)?;
    restore_locked(&path, bridge)?;
    if !crate::gate::enabled_at(root) {
        bail!("density activation is disabled");
    }
    let before = bridge.read(None)?;
    if !crate::gate::enabled_at(root) {
        bail!("density activation was cancelled");
    }
    let mut record =
        tempfile::NamedTempFile::new_in(path.parent().context("journal parent missing")?)?;
    record.write_all(&serde_json::to_vec(&Lease {
        before: before.clone(),
        after: density,
    })?)?;
    record.as_file().sync_all()?;
    record.persist(&path)?;
    File::open(path.parent().context("journal parent missing")?)?.sync_all()?;
    if !crate::gate::enabled_at(root) {
        bail!("density activation was cancelled");
    }
    bridge.set(before.user, Some(density))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    struct Fake {
        current: u32,
        values: BTreeMap<u32, Option<u32>>,
        fail_after_write: bool,
    }
    impl Bridge for Fake {
        fn read(&mut self, user: Option<u32>) -> Result<Value> {
            let user = user.unwrap_or(self.current);
            Ok(Value {
                user,
                density: *self.values.get(&user).unwrap_or(&None),
            })
        }
        fn set(&mut self, user: u32, density: Option<u32>) -> Result<()> {
            self.values.insert(user, density);
            if std::mem::take(&mut self.fail_after_write) {
                bail!("simulated post-write crash");
            }
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, Fake) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("data/adb/duck-device-faker")).unwrap();
        let data = root.path().join("data/adb/duck-device-faker");
        fs::create_dir_all(root.path().join("data/adb/modules/duck-toolbox")).unwrap();
        fs::write(
            root.path()
                .join("data/adb/modules/duck-toolbox/module.prop"),
            "id=duck-toolbox",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("proc/sys/kernel/random")).unwrap();
        fs::write(
            root.path().join("proc/sys/kernel/random/boot_id"),
            "boot-test",
        )
        .unwrap();
        fs::write(
            data.join("ready"),
            format!("boot-test:{}", crate::gate::BUILD),
        )
        .unwrap();
        fs::write(data.join("enabled"), "initial").unwrap();
        (
            root,
            Fake {
                current: 0,
                values: BTreeMap::from([(0, Some(280)), (10, Some(360))]),
                fail_after_write: false,
            },
        )
    }
    #[test]
    fn recovery_uses_the_original_user_after_a_user_switch() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 420, &mut bridge).unwrap();
        bridge.current = 10;
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(280));
        assert_eq!(bridge.values[&10], Some(360));
        assert!(!paths(root.path()).0.exists());
    }
    #[test]
    fn a_post_write_failure_retains_a_durable_recovery_record() {
        let (root, mut bridge) = fixture();
        bridge.fail_after_write = true;
        assert!(apply(root.path(), 420, &mut bridge).is_err());
        assert!(paths(root.path()).0.exists());
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(280));
    }
    #[test]
    fn recovery_preserves_an_external_density_edit() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 420, &mut bridge).unwrap();
        bridge.values.insert(0, Some(480));
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(480));
    }
    #[test]
    fn reset_and_recovery_survive_a_new_process_instance() {
        let (root, mut bridge) = fixture();
        bridge.values.insert(0, None);
        apply(root.path(), 420, &mut bridge).unwrap();
        let mut next = Fake {
            current: 10,
            values: bridge.values,
            fail_after_write: false,
        };
        recover(root.path(), &mut next).unwrap();
        assert_eq!(next.values[&0], None);
    }
    #[test]
    fn corrupt_and_oversized_journals_are_preserved_without_mutation() {
        let (root, mut bridge) = fixture();
        let (path, _) = paths(root.path());
        for content in ["invalid".to_string(), "x".repeat(4097)] {
            fs::write(&path, &content).unwrap();
            assert!(recover(root.path(), &mut bridge).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
            assert_eq!(bridge.values[&0], Some(280));
        }
    }
    #[test]
    fn the_recovery_record_is_private() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 420, &mut bridge).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(paths(root.path()).0)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
