//! Root-owned activation snapshots. Zygote cannot read the private state directory.
use crate::{
    config::{Config, MergedAppConfig},
    gate,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct Activation {
    pub generation: String,
    pub profile: MergedAppConfig,
    pub debug: bool,
    pub warnings: Vec<String>,
}

pub fn resolve_at(root: &Path, package: &str) -> Result<Option<Activation>> {
    let Some(generation) = gate::activation_at(root) else {
        return Ok(None);
    };
    let path = root.join("data/adb/duck-device-faker/config/config.toml");
    let mut content = String::new();
    std::fs::File::open(path)?
        .take(48 * 1024 + 1)
        .read_to_string(&mut content)
        .context("Failed to read Device Faker configuration")?;
    ensure!(content.len() <= 48 * 1024, "Configuration exceeds 48 KiB");
    let config = Config::from_toml(&content)?;
    let base = package.split('@').next().unwrap_or(package);
    let Some(profile) = config
        .get_merged_config(package)
        .or_else(|| config.get_merged_config(base))
    else {
        return Ok(None);
    };
    let mut warnings = Vec::new();
    if config.debug {
        for key in [Some(package), (base != package).then_some(base)]
            .into_iter()
            .flatten()
        {
            let templates = config.conflicting_templates_for_package(key);
            if templates.len() > 1 {
                warnings.push(format!(
                    "Package {key} is claimed by {} templates [{}]; '{}' takes precedence",
                    templates.len(),
                    templates.join(", "),
                    templates[0]
                ));
            }
        }
    }
    if gate::activation_at(root).as_deref() != Some(generation.as_str()) {
        return Ok(None);
    }
    Ok(Some(Activation {
        generation,
        profile,
        debug: config.debug,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data/adb/duck-device-faker");
        let module = root.path().join("data/adb/modules/duck-toolbox");
        fs::create_dir_all(data.join("config")).unwrap();
        fs::create_dir_all(&module).unwrap();
        fs::create_dir_all(root.path().join("proc/sys/kernel/random")).unwrap();
        fs::write(module.join("module.prop"), "id=duck-toolbox").unwrap();
        fs::write(
            root.path().join("proc/sys/kernel/random/boot_id"),
            "boot-one",
        )
        .unwrap();
        fs::write(data.join("ready"), format!("boot-one:{}", gate::BUILD)).unwrap();
        fs::write(data.join("enabled"), "initial").unwrap();
        fs::write(
            data.join("config/config.toml"),
            "[[apps]]\npackage='com.example.app'\nmodel='V2546A'\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn root_snapshot_resolves_targets_and_user_fallback() {
        let root = fixture();
        let selected = resolve_at(root.path(), "com.example.app@999")
            .unwrap()
            .unwrap();
        assert_eq!(selected.generation, "initial");
        assert_eq!(selected.profile.model.as_deref(), Some("V2546A"));
        assert!(
            resolve_at(root.path(), "com.other.app@0")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_user_override_is_resolved_without_disclosing_other_profiles() {
        let root = fixture();
        fs::write(
            root.path()
                .join("data/adb/duck-device-faker/config/config.toml"),
            r#"
[[apps]]
package = "com.example.app"
model = "Base"

[[apps]]
package = "com.example.app@999"
model = "Clone"

[[apps]]
package = "com.other.app"
model = "Unrelated"
"#,
        )
        .unwrap();
        let snapshot = resolve_at(root.path(), "com.example.app@999")
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.profile.model.as_deref(), Some("Clone"));
        let response = serde_json::to_string(&snapshot).unwrap();
        for unrelated in ["Base", "Unrelated", "com.other.app"] {
            assert!(!response.contains(unrelated));
        }
    }

    #[test]
    fn stopped_or_unready_engine_never_returns_private_configuration() {
        let root = fixture();
        let data = root.path().join("data/adb/duck-device-faker");
        fs::write(data.join("stop"), "new-generation").unwrap();
        assert!(
            resolve_at(root.path(), "com.example.app@0")
                .unwrap()
                .is_none()
        );
        fs::write(data.join("enabled"), "new-generation").unwrap();
        assert!(
            resolve_at(root.path(), "com.example.app@0")
                .unwrap()
                .is_some()
        );
        fs::remove_file(data.join("ready")).unwrap();
        assert!(
            resolve_at(root.path(), "com.example.app@0")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_and_oversized_snapshots_fail_closed() {
        let root = fixture();
        let path = root
            .path()
            .join("data/adb/duck-device-faker/config/config.toml");
        fs::write(&path, "invalid[").unwrap();
        assert!(resolve_at(root.path(), "com.example.app@0").is_err());
        fs::write(path, " ".repeat(48 * 1024 + 1)).unwrap();
        assert!(resolve_at(root.path(), "com.example.app@0").is_err());
    }

    #[test]
    fn reenable_does_not_validate_an_older_snapshot() {
        let root = fixture();
        let data = root.path().join("data/adb/duck-device-faker");
        let initial = resolve_at(root.path(), "com.example.app@0")
            .unwrap()
            .unwrap();
        fs::write(data.join("stop"), "next-generation").unwrap();
        assert!(gate::activation_at(root.path()).is_none());
        assert!(
            resolve_at(root.path(), "com.example.app@0")
                .unwrap()
                .is_none()
        );

        fs::write(data.join("enabled"), "next-generation").unwrap();
        let next = resolve_at(root.path(), "com.example.app@0")
            .unwrap()
            .unwrap();
        assert_ne!(initial.generation, next.generation);
        assert_eq!(gate::activation_at(root.path()), Some(next.generation));
        assert_eq!(initial.profile.model, next.profile.model);
    }

    #[test]
    fn user_specific_targets_do_not_select_other_users() {
        let root = fixture();
        let path = root
            .path()
            .join("data/adb/duck-device-faker/config/config.toml");
        for content in [
            "[[apps]]\npackage='com.example.app@999'\nmodel='Phone'\n",
            "[templates.phone]\npackages=['com.example.app@999']\nmodel='Phone'\n",
        ] {
            fs::write(&path, content).unwrap();
            let selected = resolve_at(root.path(), "com.example.app@999")
                .unwrap()
                .unwrap();
            assert_eq!(selected.profile.model.as_deref(), Some("Phone"));
            for package in ["com.example.app@0", "com.example.app", "com.other.app@999"] {
                assert!(
                    resolve_at(root.path(), package).unwrap().is_none(),
                    "{package}"
                );
            }
        }
        fs::write(
            &path,
            "[templates.phone]\npackages=['com.example.app']\nmodel='Phone'\n",
        )
        .unwrap();
        assert!(
            resolve_at(root.path(), "com.example.app@999")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn maximum_size_configuration_survives_resolution_without_truncation() {
        let root = fixture();
        let path = root
            .path()
            .join("data/adb/duck-device-faker/config/config.toml");
        let mut content = fs::read_to_string(&path).unwrap();
        content.push('#');
        content.push_str(&"x".repeat(48 * 1024 - content.len()));
        fs::write(&path, &content).unwrap();
        assert_eq!(
            resolve_at(root.path(), "com.example.app@0")
                .unwrap()
                .unwrap()
                .profile
                .model
                .as_deref(),
            Some("V2546A")
        );
        content.push('x');
        fs::write(&path, content).unwrap();
        assert!(resolve_at(root.path(), "com.example.app@0").is_err());
    }

    #[test]
    fn invalid_unused_profiles_prevent_publishing_a_snapshot() {
        let root = fixture();
        let path = root
            .path()
            .join("data/adb/duck-device-faker/config/config.toml");
        let valid = fs::read_to_string(&path).unwrap();
        for invalid in ["sdk_int=-1", "dpi=0", r#"model="\u0000""#] {
            fs::write(&path, format!("{valid}\n[templates.unused]\n{invalid}\n")).unwrap();
            assert!(
                resolve_at(root.path(), "com.example.app@0").is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn lifecycle_blocks_prevent_publishing_private_configuration() {
        for (path, content) in [
            ("data/adb/modules/duck-toolbox/disable", ""),
            ("data/adb/modules/duck-toolbox/remove", ""),
            (
                "data/adb/modules_update/duck-toolbox/module.prop",
                "id=duck-toolbox",
            ),
            (
                "data/adb/modules/device_faker/module.prop",
                "id=device_faker",
            ),
            (
                "data/adb/duck-device-faker/cleanup-failed",
                "boot-one:initial",
            ),
            ("data/adb/duck-device-faker/global-active", "invalid"),
            ("data/adb/duck-device-faker/ready", "another-build"),
            ("proc/sys/kernel/random/boot_id", "another-boot"),
        ] {
            let root = fixture();
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            assert!(
                resolve_at(root.path(), "com.example.app@0")
                    .unwrap()
                    .is_none(),
                "{}",
                path.display()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn stopping_during_configuration_read_invalidates_the_snapshot() {
        use std::{
            io::Write,
            os::unix::fs::OpenOptionsExt,
            thread,
            time::{Duration, Instant},
        };
        for reenable in [false, true] {
            let root = fixture();
            let data = root.path().join("data/adb/duck-device-faker");
            let path = data.join("config/config.toml");
            let content = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            let fifo = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            let root_path = root.path().to_owned();
            let worker = thread::spawn(move || resolve_at(&root_path, "com.example.app@0"));
            let deadline = Instant::now() + Duration::from_secs(2);
            // Opening the writer pairs with the reader after its initial generation check.
            let mut writer = loop {
                match fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                {
                    Ok(writer) => break writer,
                    Err(error)
                        if error.raw_os_error() == Some(libc::ENXIO)
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("configuration reader did not open: {error}"),
                }
            };
            fs::write(data.join("stop"), "next-generation").unwrap();
            if reenable {
                fs::write(data.join("enabled"), "next-generation").unwrap();
            }
            writer.write_all(&content).unwrap();
            drop(writer);
            assert!(worker.join().unwrap().unwrap().is_none());
        }
    }
}
