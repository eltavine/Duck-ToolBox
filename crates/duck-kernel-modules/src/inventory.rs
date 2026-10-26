use std::{collections::BTreeMap, fs, io::Read, path::Path};

use anyhow::{Context as _, Result, ensure};
use duck_core::Sysroot;
use serde::Serialize;

use crate::ModuleError;

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub name: String,
    pub source: &'static str,
    pub size: Option<u64>,
    pub references: Option<i64>,
    pub used_by: Vec<String>,
    pub state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub kernel_release: Option<String>,
    pub modules_disabled: Option<bool>,
    pub operations_supported: bool,
    pub proc_available: bool,
    pub sysfs_available: bool,
    pub warnings: Vec<String>,
    pub modules: Vec<Module>,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
}

fn read_raw_text(path: &Path, limit: usize) -> Result<String> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= limit, "{} exceeds read limit", path.display());
    Ok(String::from_utf8(data)?)
}

pub fn read_text(path: &Path, limit: usize) -> Result<String> {
    Ok(read_raw_text(path, limit)?.trim().to_owned())
}

pub fn optional_text(sysroot: &Sysroot, path: &str) -> Option<String> {
    read_text(&sysroot.path(path), 4096).ok()
}

pub fn parse_proc(raw: &str) -> Result<Vec<Module>> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            ensure!(
                fields.len() >= 6 && valid_name(fields[0]),
                "malformed /proc/modules row"
            );
            Ok(Module {
                name: fields[0].into(),
                source: "loaded",
                size: Some(fields[1].parse().context("module size")?),
                // CONFIG_MODULE_UNLOAD=n emits '-' for both reference count and users.
                references: if fields[2] == "-" {
                    None
                } else {
                    Some(fields[2].parse().context("module reference count")?)
                },
                used_by: fields[3]
                    .split(',')
                    .filter(|s| !s.is_empty() && *s != "-")
                    .map(str::to_owned)
                    .collect(),
                state: Some(fields[4].into()),
            })
        })
        .collect()
}

pub fn status(sysroot: &Sysroot) -> Result<Status> {
    let mut warnings = Vec::new();
    let proc =
        read_text(&sysroot.path("/proc/modules"), 2 * 1024 * 1024).and_then(|raw| parse_proc(&raw));
    let proc_available = proc.is_ok();
    let mut modules: BTreeMap<_, _> = match proc {
        Ok(items) => items
            .into_iter()
            .map(|item| (item.name.clone(), item))
            .collect(),
        Err(error) => {
            warnings.push(format!("/proc/modules: {error:#}"));
            BTreeMap::new()
        }
    };
    let sysfs_available = match fs::read_dir(sysroot.path("/sys/module")) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        warnings.push(format!("/sys/module: {error}"));
                        continue;
                    }
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                if valid_name(&name) && entry.path().is_dir() {
                    modules.entry(name.clone()).or_insert_with(|| Module {
                        state: optional_text(sysroot, &format!("/sys/module/{name}/initstate")),
                        name,
                        source: "sysfs_only",
                        size: None,
                        references: None,
                        used_by: vec![],
                    });
                }
            }
            true
        }
        Err(error) => {
            warnings.push(format!("/sys/module: {error}"));
            false
        }
    };
    let modules_disabled =
        match optional_text(sysroot, "/proc/sys/kernel/modules_disabled").as_deref() {
            Some("0") => Some(false),
            Some("1") => Some(true),
            _ => None,
        };
    Ok(Status {
        kernel_release: optional_text(sysroot, "/proc/sys/kernel/osrelease"),
        modules_disabled,
        operations_supported: crate::kernel::supported(sysroot),
        proc_available,
        sysfs_available,
        warnings,
        modules: modules.into_values().collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct Parameter {
    pub name: String,
    pub value: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Details {
    pub module: Module,
    pub version: Option<String>,
    pub srcversion: Option<String>,
    pub taint: Option<String>,
    pub holders: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub warnings: Vec<String>,
}

pub fn details(sysroot: &Sysroot, name: &str) -> Result<Details> {
    ensure!(
        valid_name(name),
        ModuleError::InvalidInput("invalid module name".into())
    );
    let inventory = status(sysroot)?;
    let module = inventory
        .modules
        .into_iter()
        .find(|item| item.name == name)
        .ok_or_else(|| ModuleError::Unavailable(format!("module {name} is no longer visible")))?;
    let base = format!("/sys/module/{name}");
    let mut warnings = inventory.warnings;
    let mut holders = vec![];
    match fs::read_dir(sysroot.path(format!("{base}/holders"))) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => holders.push(entry.file_name().to_string_lossy().into_owned()),
                    Err(error) => warnings.push(format!("holders: {error}")),
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warnings.push(format!("holders: {error}")),
    }
    holders.sort();
    let mut parameters = vec![];
    match fs::read_dir(sysroot.path(format!("{base}/parameters"))) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index >= 1024 {
                    warnings.push("parameter listing limited to 1024 entries".into());
                    break;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        warnings.push(format!("parameters: {error}"));
                        continue;
                    }
                };
                let (value, error) = match read_raw_text(&entry.path(), 16_384) {
                    // Kernel parameter getters append one newline; other whitespace is data.
                    Ok(value) => (
                        Some(value.strip_suffix('\n').unwrap_or(&value).to_owned()),
                        None,
                    ),
                    Err(error) => (None, Some(format!("{error:#}"))),
                };
                parameters.push(Parameter {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    value,
                    error,
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warnings.push(format!("parameters: {error}")),
    }
    parameters.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Details {
        version: optional_text(sysroot, &format!("{base}/version")),
        srcversion: optional_text(sysroot, &format!("{base}/srcversion")),
        taint: optional_text(sysroot, &format!("{base}/taint")),
        module,
        holders,
        parameters,
        warnings,
    })
}
