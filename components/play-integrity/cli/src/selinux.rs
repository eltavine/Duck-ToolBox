// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: record permissive visibility permissions and restore only owned changes.
use crate::{Engine, files, power};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const FILES: [(&str, u32); 2] = [
    ("/sys/fs/selinux/enforce", 0o640),
    ("/sys/fs/selinux/policy", 0o440),
];
#[derive(Serialize, Deserialize)]
struct Permission {
    before: u32,
    applied: u32,
}
fn path(engine: &Engine) -> std::path::PathBuf {
    engine.data.join("selinux-permissions.json")
}
fn load(engine: &Engine) -> Result<BTreeMap<String, Permission>> {
    let raw = files::read(&path(engine))?;
    Ok(if raw.is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(&raw)?
    })
}
#[cfg(unix)]
fn mode(path: &std::path::Path) -> Result<u32> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o7777)
}
#[cfg(unix)]
fn chmod(path: &std::path::Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(mode),
    )?)
}
#[cfg(not(unix))]
fn mode(_: &std::path::Path) -> Result<u32> {
    bail!("SELinux permissions require Unix")
}
#[cfg(not(unix))]
fn chmod(_: &std::path::Path, _: u32) -> Result<()> {
    bail!("SELinux permissions require Unix")
}
pub fn apply(engine: &Engine) -> Result<()> {
    if power::disabled(engine) {
        return Ok(());
    }
    let enforce = engine.device(FILES[0].0);
    if !enforce.exists() {
        return Ok(());
    }
    let state = if engine.sysroot == std::path::Path::new("/") {
        let output = crate::command::run(
            std::process::Command::new("toybox").args(["cat", "/sys/fs/selinux/enforce"]),
            std::time::Duration::from_secs(2),
        )?;
        if !output.status.success() {
            bail!("cannot read SELinux enforce state");
        }
        String::from_utf8(output.stdout)?
    } else {
        files::read(&enforce)?
    };
    if state.trim() != "0" {
        return Ok(());
    }
    let mut saved = load(engine)?;
    for (name, applied) in FILES {
        if power::disabled(engine) {
            break;
        }
        let current = mode(&engine.device(name))?;
        if let Some(record) = saved.get(name) {
            if current != record.before && current != record.applied {
                bail!("external SELinux permission edit preserved: {name}");
            }
        } else {
            saved.insert(
                name.into(),
                Permission {
                    before: current,
                    applied,
                },
            );
            files::atomic(&path(engine), &serde_json::to_vec(&saved)?)?;
        }
        chmod(&engine.device(name), applied)?;
    }
    Ok(())
}
pub fn restore(engine: &Engine) -> Result<()> {
    let mut saved = load(engine)?;
    let mut errors = Vec::new();
    for (name, _) in FILES {
        let Some(record) = saved.get(name) else {
            continue;
        };
        let result = (|| -> Result<()> {
            let path = engine.device(name);
            let current = mode(&path)?;
            if current == record.before {
                return Ok(());
            }
            if current != record.applied {
                bail!("external SELinux permission edit preserved: {name}");
            }
            chmod(&path, record.before)
        })();
        match result {
            Ok(()) => {
                saved.remove(name);
                files::atomic(&path(engine), &serde_json::to_vec(&saved)?)?;
            }
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if !errors.is_empty() {
        bail!("{}; backup: {}", errors.join("; "), path(engine).display());
    }
    Ok(())
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn visibility_restore_keeps_kernel_state_and_external_permissions() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().join("data"),
            module: root.path().join("module"),
            sysroot: root.path().join("sys"),
        };
        let enforce = e.device(FILES[0].0);
        std::fs::create_dir_all(enforce.parent().unwrap()).unwrap();
        for (name, _) in FILES {
            std::fs::write(e.device(name), "0").unwrap();
            chmod(&e.device(name), 0o644).unwrap();
        }
        files::atomic(&e.data.join("options.json"), br#"{"enabled":true}"#).unwrap();
        apply(&e).unwrap();
        assert_eq!(mode(&enforce).unwrap(), 0o640);
        assert_eq!(files::read(&enforce).unwrap(), "0");
        restore(&e).unwrap();
        assert_eq!(mode(&enforce).unwrap(), 0o644);
        apply(&e).unwrap();
        chmod(&enforce, 0o600).unwrap();
        assert!(restore(&e).is_err());
        assert_eq!(mode(&enforce).unwrap(), 0o600);
    }
}
