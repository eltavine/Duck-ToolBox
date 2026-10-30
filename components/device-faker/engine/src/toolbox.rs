//! Runtime ownership and opt-in gate for the toolbox-managed Zygisk payload.
use std::path::Path;

pub fn enabled() -> bool {
    enabled_at(Path::new("/"))
}

fn active(root: &Path, id: &str) -> bool {
    let module = root.join("data/adb/modules").join(id);
    module.join("module.prop").is_file()
        && !module.join("disable").exists()
        && !module.join("remove").exists()
}

/// A flag changed during this boot cannot unload an already registered Zygisk library.
/// Accept only a disable flag known to predate the current boot; absent evidence blocks.
pub fn standalone_conflict_at(root: &Path) -> bool {
    let module = root.join("data/adb/modules/device_faker");
    if !module.join("module.prop").exists() {
        return false;
    }
    let evidence = (|| -> Option<bool> {
        let stat = std::fs::read_to_string(root.join("proc/stat")).ok()?;
        let boot = stat
            .lines()
            .find_map(|line| line.strip_prefix("btime "))?
            .trim()
            .parse::<u64>()
            .ok()?;
        let changed = module
            .join("disable")
            .metadata()
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Some(changed < boot)
    })();
    evidence != Some(true)
}

pub const BUILD: &str = match option_env!("DEVICE_FAKER_BUILD") {
    Some(value) => value,
    None => "development",
};
pub fn startup_ready_at(root: &Path) -> bool {
    let Ok(boot) = std::fs::read_to_string(root.join("proc/sys/kernel/random/boot_id")) else {
        return false;
    };
    if boot.trim().is_empty() || boot.len() > 64 {
        return false;
    }
    std::fs::read_to_string(root.join("data/adb/duck-device-faker/ready"))
        .is_ok_and(|ready| ready == format!("{}:{BUILD}", boot.trim()))
}

pub(crate) fn enabled_at(root: &Path) -> bool {
    std::fs::read_to_string(root.join("data/adb/duck-device-faker/enabled")).is_ok_and(|token| {
        let stop = std::fs::read_to_string(root.join("data/adb/duck-device-faker/stop"));
        match stop {
            Ok(stop) => token == stop,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => token == "initial",
            Err(_) => false,
        }
    }) && !root
        .join("data/adb/duck-device-faker/cleanup-failed")
        .exists()
        && active(root, "duck-toolbox")
        && startup_ready_at(root)
        && !standalone_conflict_at(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_requires_opt_in_both_owners_and_no_upstream_conflict() {
        let root = std::env::temp_dir().join(format!("faker-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data/adb/duck-device-faker");
        std::fs::create_dir_all(&data).unwrap();
        assert!(!enabled_at(&root));
        std::fs::write(data.join("enabled"), "initial").unwrap();
        std::fs::create_dir_all(root.join("proc/sys/kernel/random")).unwrap();
        std::fs::write(root.join("proc/sys/kernel/random/boot_id"), "boot-one").unwrap();
        std::fs::write(data.join("ready"), format!("boot-one:{BUILD}")).unwrap();
        let future_boot = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 60;
        std::fs::write(root.join("proc/stat"), format!("btime {future_boot}\n")).unwrap();
        for id in ["duck-toolbox", "device_faker"] {
            let module = root.join("data/adb/modules").join(id);
            std::fs::create_dir_all(&module).unwrap();
            std::fs::write(module.join("module.prop"), "").unwrap();
            assert_eq!(enabled_at(&root), id == "duck-toolbox");
            for flag in ["disable", "remove"] {
                std::fs::write(module.join(flag), "").unwrap();
                assert_eq!(enabled_at(&root), id == "device_faker" && flag == "disable");
                std::fs::remove_file(module.join(flag)).unwrap();
            }
        }
        let upstream = root.join("data/adb/modules/device_faker");
        std::fs::write(upstream.join("disable"), "").unwrap();
        std::fs::write(root.join("proc/stat"), "btime 1\n").unwrap();
        assert!(
            !enabled_at(&root),
            "a disable from the running boot still conflicts"
        );
        std::fs::remove_file(root.join("proc/stat")).unwrap();
        assert!(!enabled_at(&root), "missing boot evidence must block");
        std::fs::remove_dir_all(root).unwrap();
    }
}
