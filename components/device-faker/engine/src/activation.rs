//! Root-owned activation snapshots. Zygote cannot read the private state directory.
use crate::{config::Config, gate};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct Activation {
    pub generation: String,
    pub content: String,
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
    if config
        .get_merged_config(package)
        .or_else(|| config.get_merged_config(base))
        .is_none()
        || gate::activation_at(root).as_deref() != Some(generation.as_str())
    {
        return Ok(None);
    }
    Ok(Some(Activation {
        generation,
        content,
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
        let config = Config::from_toml(&selected.content).unwrap();
        assert_eq!(
            config
                .get_merged_config("com.example.app")
                .unwrap()
                .model
                .as_deref(),
            Some("V2546A")
        );
        assert!(
            resolve_at(root.path(), "com.other.app@0")
                .unwrap()
                .is_none()
        );
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
}
