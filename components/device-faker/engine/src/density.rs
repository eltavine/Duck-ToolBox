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
struct DensityLock(File);
impl Drop for DensityLock {
    fn drop(&mut self) {
        // An unrelated fork may inherit the descriptor until exec. Only this
        // guard owns the transaction, so descriptor lifetime must not extend it.
        let _ = self.0.unlock();
    }
}
fn lock(path: &Path) -> Result<DensityLock> {
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().context("density recovery is busy")?;
    Ok(DensityLock(file))
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
    let mut lease: Lease = serde_json::from_slice(&bytes)?;
    if lease.before.user > i32::MAX as u32
        || lease
            .before
            .density
            .is_some_and(|value| value > i32::MAX as u32)
        || !(crate::config::DPI_MIN..=crate::config::DPI_MAX).contains(&lease.after)
    {
        bail!("invalid density recovery journal values");
    }
    // Earlier records could capture Android's zero/unset sentinel verbatim.
    if lease.before.density == Some(0) {
        lease.before.density = None;
    }
    Ok(Some(lease))
}
fn restore_locked(path: &Path, bridge: &mut impl Bridge) -> Result<()> {
    let Some(lease) = load(path)? else {
        return Ok(());
    };
    let current = bridge.read(Some(lease.before.user))?;
    if current.user != lease.before.user {
        bail!("density recovery returned another user");
    }
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
pub fn apply(
    root: &Path,
    user: u32,
    density: u32,
    generation: &str,
    bridge: &mut impl Bridge,
) -> Result<()> {
    if user > i32::MAX as u32
        || !(crate::config::DPI_MIN..=crate::config::DPI_MAX).contains(&density)
    {
        bail!("invalid density activation values");
    }
    let (path, lock_path) = paths(root);
    let _lock = lock(&lock_path)?;
    restore_locked(&path, bridge)?;
    if crate::gate::activation_at(root).as_deref() != Some(generation) {
        bail!("density activation is disabled");
    }
    let before = bridge.read(None)?;
    if before.user != user {
        bail!("foreground user changed before density activation");
    }
    // The helper can restore only positive Java integer density overrides.
    if before.user > i32::MAX as u32
        || before
            .density
            .is_some_and(|value| value == 0 || value > i32::MAX as u32)
    {
        bail!("original density cannot be restored safely");
    }
    if crate::gate::activation_at(root).as_deref() != Some(generation) {
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
    if crate::gate::activation_at(root).as_deref() != Some(generation) {
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
        change_generation_on_read: Option<PathBuf>,
        wrong_read_user: bool,
    }
    impl Bridge for Fake {
        fn read(&mut self, user: Option<u32>) -> Result<Value> {
            if let Some(root) = self.change_generation_on_read.take() {
                let data = root.join("data/adb/duck-device-faker");
                fs::write(data.join("stop"), "next").unwrap();
                fs::write(data.join("enabled"), "next").unwrap();
            }
            let user = user.unwrap_or(self.current);
            Ok(Value {
                user: if self.wrong_read_user { user + 1 } else { user },
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
                change_generation_on_read: None,
                wrong_read_user: false,
            },
        )
    }
    #[test]
    fn recovery_uses_the_original_user_after_a_user_switch() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 0, 420, "initial", &mut bridge).unwrap();
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
        assert!(apply(root.path(), 0, 420, "initial", &mut bridge).is_err());
        assert!(paths(root.path()).0.exists());
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(280));
    }
    #[test]
    fn recovery_preserves_an_external_density_edit() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 0, 420, "initial", &mut bridge).unwrap();
        bridge.values.insert(0, Some(480));
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(480));
    }
    #[test]
    fn activation_binds_the_profile_user_before_publishing_recovery_data() {
        let (root, mut bridge) = fixture();
        bridge.current = 10;
        let before = bridge.values.clone();
        assert!(apply(root.path(), 0, 420, "initial", &mut bridge).is_err());
        assert_eq!(bridge.values, before);
        assert!(!paths(root.path()).0.exists());
        apply(root.path(), 10, 420, "initial", &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], before[&0]);
        assert_eq!(bridge.values[&10], Some(420));
        bridge.current = 0;
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values, before);
    }
    #[test]
    fn reset_and_recovery_survive_a_new_process_instance() {
        let (root, mut bridge) = fixture();
        bridge.values.insert(0, None);
        apply(root.path(), 0, 420, "initial", &mut bridge).unwrap();
        let mut next = Fake {
            current: 10,
            values: bridge.values,
            fail_after_write: false,
            change_generation_on_read: None,
            wrong_read_user: false,
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
    struct UnusedBridge;
    impl Bridge for UnusedBridge {
        fn read(&mut self, _: Option<u32>) -> Result<Value> {
            panic!("invalid values must not reach the platform")
        }
        fn set(&mut self, _: u32, _: Option<u32>) -> Result<()> {
            panic!("invalid values must not mutate the platform")
        }
    }
    #[test]
    fn invalid_journal_values_are_preserved_without_platform_access() {
        let (root, _) = fixture();
        let (path, _) = paths(root.path());
        for (user, before, after) in [
            (u32::MAX, Some(280), 420),
            (0, Some(u32::MAX), 420),
            (0, Some(280), 0),
            (0, Some(280), 641),
        ] {
            let content = serde_json::to_vec(&Lease {
                before: Value {
                    user,
                    density: before,
                },
                after,
            })
            .unwrap();
            fs::write(&path, &content).unwrap();
            assert!(recover(root.path(), &mut UnusedBridge).is_err());
            assert_eq!(fs::read(&path).unwrap(), content);
        }
    }
    #[test]
    fn invalid_activation_values_cannot_replace_a_recovery_record() {
        let (root, _) = fixture();
        let (path, _) = paths(root.path());
        let content = br#"{"before":{"user":0,"override":280},"after":420}"#;
        fs::write(&path, content).unwrap();
        for (user, density) in [(u32::MAX, 420), (0, 0), (0, 641)] {
            assert!(apply(root.path(), user, density, "initial", &mut UnusedBridge).is_err());
            assert_eq!(fs::read(&path).unwrap(), content);
        }
    }
    #[test]
    fn stop_then_reenable_cannot_resume_an_old_density_activation() {
        let (root, mut bridge) = fixture();
        bridge.change_generation_on_read = Some(root.path().into());
        assert!(apply(root.path(), 0, 420, "initial", &mut bridge).is_err());
        assert_eq!(bridge.values[&0], Some(280));
        assert!(!paths(root.path()).0.exists());
        apply(root.path(), 0, 420, "next", &mut bridge).unwrap();
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(280));
    }
    #[test]
    fn mismatched_user_readback_retains_recovery_and_never_writes_another_user() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 0, 420, "initial", &mut bridge).unwrap();
        bridge.wrong_read_user = true;
        assert!(recover(root.path(), &mut bridge).is_err());
        assert!(paths(root.path()).0.exists());
        assert_eq!(bridge.values[&0], Some(420));
        assert_eq!(bridge.values[&10], Some(360));
        bridge.wrong_read_user = false;
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], Some(280));
    }
    #[test]
    fn recovery_accepts_an_earlier_journal_with_a_zero_unset_baseline() {
        let (root, mut bridge) = fixture();
        fs::write(
            paths(root.path()).0,
            r#"{"before":{"user":0,"override":0},"after":420}"#,
        )
        .unwrap();
        bridge.values.insert(0, Some(420));
        recover(root.path(), &mut bridge).unwrap();
        assert_eq!(bridge.values[&0], None);
        assert!(!paths(root.path()).0.exists());
    }
    #[test]
    fn an_unrestorable_baseline_is_rejected_before_any_density_change() {
        let (root, mut bridge) = fixture();
        bridge.values.insert(0, Some(u32::MAX));
        assert!(apply(root.path(), 0, 420, "initial", &mut bridge).is_err());
        assert_eq!(bridge.values[&0], Some(u32::MAX));
        assert!(!paths(root.path()).0.exists());
    }
    #[cfg(unix)]
    #[test]
    fn inherited_descriptors_do_not_extend_a_completed_density_transaction() {
        const CHILD: &str = "DEVICE_FAKER_TEST_INHERITED_DENSITY_LOCK";
        if std::env::var_os(CHILD).is_none() {
            assert!(Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "density::tests::inherited_descriptors_do_not_extend_a_completed_density_transaction"])
                .env(CHILD, "1").status().unwrap().success());
            return;
        }
        let (root, _) = fixture();
        let path = paths(root.path()).1;
        let guard = lock(&path).unwrap();
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            // After a multithreaded fork, the child executes only async-signal-safe calls.
            loop {
                unsafe {
                    libc::pause();
                }
            }
        }
        struct Reap(libc::pid_t);
        impl Drop for Reap {
            fn drop(&mut self) {
                unsafe {
                    libc::kill(self.0, libc::SIGKILL);
                    while libc::waitpid(self.0, std::ptr::null_mut(), 0) < 0 {
                        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                            break;
                        }
                    }
                }
            }
        }
        let _child = Reap(pid);
        drop(guard);
        assert!(
            lock(&path).is_ok(),
            "child retains the descriptor but not transaction ownership"
        );
    }
    #[test]
    fn the_recovery_record_is_private() {
        let (root, mut bridge) = fixture();
        apply(root.path(), 0, 420, "initial", &mut bridge).unwrap();
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
