// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: journal Magisk list changes and preserve external edits on restoration.
use crate::{Engine, command, files, power};
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
    time::Duration,
};
const PACKAGES: [&str; 2] = ["com.google.android.gms", "com.android.vending"];
type Processes = BTreeSet<String>;
#[derive(Default, Serialize, Deserialize)]
struct Record {
    before: Processes,
    expected: Processes,
    pending: Option<Processes>,
}
type Journal = BTreeMap<String, Record>;
trait List {
    fn read(&mut self) -> Result<BTreeMap<String, Processes>>;
    fn set(&mut self, pkg: &str, proc: &str, present: bool) -> Result<()>;
}
struct Magisk(std::path::PathBuf);
impl Magisk {
    fn discover(engine: &Engine) -> Option<Self> {
        let path = engine.device("/data/adb/magisk/magisk");
        if path.is_file() {
            Some(Self(path))
        } else if engine.sysroot == std::path::Path::new("/") {
            Some(Self("magisk".into()))
        } else {
            None
        }
    }
    fn call(&self, args: &[&str]) -> Result<std::process::Output> {
        command::run(
            Command::new(&self.0).arg("--denylist").args(args),
            Duration::from_secs(2),
        )
    }
}
impl List for Magisk {
    fn read(&mut self) -> Result<BTreeMap<String, Processes>> {
        let output = self.call(&["ls"])?;
        if !output.status.success() {
            bail!(
                "cannot read Magisk DenyList: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let mut result = BTreeMap::<String, Processes>::new();
        for line in String::from_utf8(output.stdout)?.lines() {
            let Some((pkg, proc)) = line.split_once('|') else {
                bail!("unsupported Magisk DenyList output");
            };
            if !PACKAGES.contains(&pkg) {
                continue;
            }
            if proc.is_empty()
                || proc.len() > 256
                || !proc
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._:".contains(&c))
            {
                bail!("invalid process in Magisk list");
            }
            result.entry(pkg.into()).or_default().insert(proc.into());
        }
        Ok(result)
    }
    fn set(&mut self, pkg: &str, proc: &str, present: bool) -> Result<()> {
        let output = self.call(&[if present { "add" } else { "rm" }, pkg, proc])?;
        if !output.status.success() {
            bail!(
                "Magisk list update failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
}
fn load(engine: &Engine) -> Result<Journal> {
    let raw = files::read(&engine.data.join("denylist.json"))?;
    let result: Journal = if raw.is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(&raw)?
    };
    if result.keys().any(|p| !PACKAGES.contains(&p.as_str())) {
        bail!("invalid DenyList journal package");
    }
    Ok(result)
}
fn save(engine: &Engine, journal: &Journal) -> Result<()> {
    files::atomic(
        &engine.data.join("denylist.json"),
        &serde_json::to_vec(journal)?,
    )
}
fn reconcile(record: &mut Record, current: &Processes) -> Result<()> {
    if let Some(pending) = &record.pending {
        if current == pending {
            record.expected = pending.clone();
        } else if current != &record.expected {
            bail!("external DenyList edit; preserved without restoration");
        }
        record.pending = None;
    }
    if current != &record.expected {
        bail!("external DenyList edit; preserved without restoration");
    }
    Ok(())
}
fn update(
    engine: &Engine,
    api: &mut impl List,
    journal: &mut Journal,
    pkg: &str,
    desired: &Processes,
    restoring: bool,
) -> Result<()> {
    let current = api.read()?.remove(pkg).unwrap_or_default();
    let record = journal.get_mut(pkg).context("missing package journal")?;
    reconcile(record, &current)?;
    let changes: Vec<_> = current.symmetric_difference(desired).cloned().collect();
    for proc in changes {
        if !restoring && power::disabled(engine) {
            bail!("engine disabled during DenyList update");
        }
        let current = api.read()?.remove(pkg).unwrap_or_default();
        let record = journal.get_mut(pkg).context("missing package journal")?;
        reconcile(record, &current)?;
        let present = desired.contains(&proc);
        let mut next = current;
        if present {
            next.insert(proc.clone());
        } else {
            next.remove(&proc);
        }
        record.pending = Some(next.clone());
        save(engine, journal)?;
        api.set(pkg, &proc, present)?;
        if api.read()?.remove(pkg).unwrap_or_default() != next {
            bail!("DenyList readback mismatch; journal retained");
        }
        let record = journal.get_mut(pkg).unwrap();
        record.expected = next;
        record.pending = None;
        save(engine, journal)?;
    }
    save(engine, journal)
}
fn manage_with(engine: &Engine, api: &mut impl List, scripts_only: bool) -> Result<()> {
    let current = api.read()?;
    let mut journal = load(engine)?;
    for pkg in PACKAGES {
        let initial = current.get(pkg).cloned().unwrap_or_default();
        journal.entry(pkg.into()).or_insert_with(|| Record {
            before: initial.clone(),
            expected: initial.clone(),
            pending: None,
        });
        let mut desired = initial;
        if scripts_only {
            desired.insert(if pkg == PACKAGES[0] {
                "com.google.android.gms.unstable".into()
            } else {
                pkg.into()
            });
        } else {
            desired.clear();
        }
        update(engine, api, &mut journal, pkg, &desired, false)?;
    }
    Ok(())
}
fn restore_with(engine: &Engine, api: &mut impl List) -> Result<()> {
    let mut journal = load(engine)?;
    let mut errors = Vec::new();
    for pkg in PACKAGES {
        let Some(record) = journal.get(pkg) else {
            continue;
        };
        let desired = record.before.clone();
        match update(engine, api, &mut journal, pkg, &desired, true) {
            Ok(()) => {
                journal.remove(pkg);
                save(engine, &journal)?;
            }
            Err(error) => errors.push(format!("{pkg}: {error:#}")),
        }
    }
    if !errors.is_empty() {
        bail!(
            "{}; journal: {}",
            errors.join("; "),
            engine.data.join("denylist.json").display()
        );
    }
    Ok(())
}
pub fn manage(engine: &Engine, mode: &str) -> Result<()> {
    let Some(mut api) = Magisk::discover(engine) else {
        return Ok(());
    };
    let status = match api.call(&["status"]) {
        Ok(v) => v,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    if mode != "scripts-only" && !status.status.success() {
        return Ok(());
    }
    manage_with(engine, &mut api, mode == "scripts-only")
}
pub fn restore(engine: &Engine) -> Result<()> {
    if load(engine)?.is_empty() {
        return Ok(());
    }
    let mut api =
        Magisk::discover(engine).context("Magisk unavailable; DenyList journal retained")?;
    restore_with(engine, &mut api)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake {
        entries: BTreeMap<String, Processes>,
    }
    impl List for Fake {
        fn read(&mut self) -> Result<BTreeMap<String, Processes>> {
            Ok(self.entries.clone())
        }
        fn set(&mut self, pkg: &str, proc: &str, present: bool) -> Result<()> {
            let entries = self.entries.entry(pkg.into()).or_default();
            if present {
                entries.insert(proc.into());
            } else {
                entries.remove(proc);
            }
            Ok(())
        }
    }
    #[test]
    fn restore_modes_and_preserve_external_edits() {
        for scripts in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let e = Engine {
                data: root.path().into(),
                module: root.path().into(),
                sysroot: root.path().into(),
            };
            files::atomic(&e.data.join("options.json"), br#"{"enabled":true}"#).unwrap();
            let mut api = Fake::default();
            api.set(PACKAGES[0], "com.google.android.gms", true)
                .unwrap();
            let before = api.entries.clone();
            manage_with(&e, &mut api, scripts).unwrap();
            manage_with(&e, &mut api, scripts).unwrap();
            restore_with(&e, &mut api).unwrap();
            assert_eq!(api.entries.get(PACKAGES[0]), before.get(PACKAGES[0]));
            assert!(api.entries.get(PACKAGES[1]).is_none_or(BTreeSet::is_empty));
            manage_with(&e, &mut api, scripts).unwrap();
            api.set(PACKAGES[0], "com.google.android.gms.external", true)
                .unwrap();
            let edited = api.entries[PACKAGES[0]].clone();
            assert!(restore_with(&e, &mut api).is_err());
            assert_eq!(api.entries[PACKAGES[0]], edited);
        }
    }
    #[test]
    fn pending_change_and_mode_switch_keep_original_baseline() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().into(),
            module: root.path().into(),
            sysroot: root.path().into(),
        };
        files::atomic(&e.data.join("options.json"), br#"{"enabled":true}"#).unwrap();
        let mut api = Fake::default();
        api.set(PACKAGES[0], "com.google.android.gms.original", true)
            .unwrap();
        manage_with(&e, &mut api, false).unwrap();
        manage_with(&e, &mut api, true).unwrap();
        let mut journal = load(&e).unwrap();
        let record = journal.get_mut(PACKAGES[0]).unwrap();
        record.pending = Some(record.before.clone());
        save(&e, &journal).unwrap();
        api.entries
            .insert(PACKAGES[0].into(), journal[PACKAGES[0]].before.clone());
        restore_with(&e, &mut api).unwrap();
        assert!(api.entries[PACKAGES[0]].contains("com.google.android.gms.original"));
    }
}
