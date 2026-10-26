use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path},
};

use anyhow::{Context as _, Result, ensure};
use duck_core::Sysroot;
use object::{Object, ObjectSection};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{ModuleError, inventory};

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const ROOTS: &[&str] = &[
    "/vendor/lib/modules",
    "/vendor_dlkm/lib/modules",
    "/system/lib/modules",
    "/system_dlkm/lib/modules",
    "/odm/lib/modules",
    "/odm_dlkm/lib/modules",
];

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    Aarch64,
    Arm,
    X86_64,
    I386,
    Riscv64,
    Riscv32,
    Other,
}

impl From<object::Architecture> for Architecture {
    fn from(value: object::Architecture) -> Self {
        match value {
            object::Architecture::Aarch64 => Self::Aarch64,
            object::Architecture::Arm => Self::Arm,
            object::Architecture::X86_64 => Self::X86_64,
            object::Architecture::I386 => Self::I386,
            object::Architecture::Riscv64 => Self::Riscv64,
            object::Architecture::Riscv32 => Self::Riscv32,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub architecture: Architecture,
    pub elf_class: u8,
    pub little_endian: bool,
    pub versioned_symbols: bool,
    pub warnings: Vec<&'static str>,
    pub metadata: BTreeMap<String, Vec<String>>,
    pub kernel_release_match: Option<bool>,
}

pub fn inspect(sysroot: &Sysroot, path: &str) -> Result<(FileInfo, Vec<u8>)> {
    let path_obj = Path::new(path);
    ensure!(
        path_obj.is_absolute()
            && !path_obj
                .components()
                .any(|p| matches!(p, Component::ParentDir)),
        ModuleError::InvalidInput("use an absolute device path without '..'".into())
    );
    ensure!(
        path_obj.extension().is_some_and(|ext| ext == "ko"),
        ModuleError::InvalidInput("only uncompressed .ko files are supported".into())
    );
    let host = sysroot
        .path(path)
        .canonicalize()
        .context("resolve module file")?;
    ensure!(
        host.starts_with(sysroot.root().canonicalize()?),
        ModuleError::InvalidInput("module path leaves the device root".into())
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(host).context("open module file")?;
    ensure!(
        file.metadata()?.is_file(),
        ModuleError::InvalidInput("not a regular file".into())
    );
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        ModuleError::InvalidInput("module exceeds 64 MiB".into())
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        ModuleError::InvalidInput("module exceeds 64 MiB".into())
    );
    let info = parse(sysroot, path, &bytes)?;
    Ok((info, bytes))
}

fn parse(sysroot: &Sysroot, path: &str, bytes: &[u8]) -> Result<FileInfo> {
    let object = object::File::parse(bytes)
        .map_err(|e| ModuleError::InvalidInput(format!("invalid ELF module: {e}")))?;
    ensure!(
        object.format() == object::BinaryFormat::Elf
            && object.kind() == object::ObjectKind::Relocatable,
        ModuleError::InvalidInput("expected a relocatable ELF kernel module".into())
    );
    let section = object
        .section_by_name(".modinfo")
        .ok_or_else(|| ModuleError::InvalidInput("ELF has no .modinfo section".into()))?;
    ensure!(
        section.size() <= 1024 * 1024,
        ModuleError::InvalidInput("module metadata exceeds 1 MiB".into())
    );
    let mut metadata: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut warnings = Vec::new();
    for (index, entry) in section
        .data()?
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
        .enumerate()
    {
        // Application budgets complement the byte limit and bound map/DOM amplification.
        ensure!(
            index < 8192,
            ModuleError::InvalidInput("module metadata exceeds 8192 entries".into())
        );
        let text = String::from_utf8_lossy(entry);
        if matches!(text, std::borrow::Cow::Owned(_)) && warnings.is_empty() {
            warnings.push("metadata_text_replaced");
        }
        if let Some((key, value)) = text.split_once('=') {
            ensure!(
                !key.is_empty() && key.len() <= 128,
                ModuleError::InvalidInput("module metadata key must contain 1–128 bytes".into())
            );
            ensure!(
                metadata.contains_key(key) || metadata.len() < 128,
                ModuleError::InvalidInput("module metadata exceeds 128 distinct keys".into())
            );
            metadata
                .entry(key.to_owned())
                .or_default()
                .push(value.to_owned());
        }
    }
    let first = |key: &str| metadata.get(key).and_then(|items| items.first());
    let name = first("name").cloned().unwrap_or_else(|| {
        Path::new(path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .replace('-', "_")
    });
    ensure!(
        inventory::valid_name(&name),
        ModuleError::InvalidInput("invalid module name in ELF".into())
    );
    let release = inventory::optional_text(sysroot, "/proc/sys/kernel/osrelease");
    let kernel_release_match = first("vermagic")
        .and_then(|v| v.split_whitespace().next())
        .zip(release.as_deref())
        .map(|(module, kernel)| module == kernel);
    Ok(FileInfo {
        path: path.into(),
        name,
        size: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(bytes)),
        architecture: object.architecture().into(),
        elf_class: if object.is_64() { 64 } else { 32 },
        little_endian: object.is_little_endian(),
        versioned_symbols: object
            .section_by_name("__versions")
            .is_some_and(|s| s.size() > 0)
            || (object
                .section_by_name("__version_ext_names")
                .is_some_and(|s| s.size() > 0)
                && object
                    .section_by_name("__version_ext_crcs")
                    .is_some_and(|s| s.size() > 0)),
        warnings,
        metadata,
        kernel_release_match,
    })
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub roots: &'static [&'static str],
    pub files: Vec<Candidate>,
    pub warnings: Vec<String>,
    pub truncated: bool,
}

pub fn catalog(sysroot: &Sysroot) -> Result<Catalog> {
    let mut result = Catalog {
        roots: ROOTS,
        files: vec![],
        warnings: vec![],
        truncated: false,
    };
    let mut pending: Vec<_> = ROOTS.iter().map(|root| (sysroot.path(root), 0)).collect();
    let mut seen = BTreeSet::new();
    let mut visited = 0;
    while let Some((dir, depth)) = pending.pop() {
        let canonical = match dir.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                result
                    .warnings
                    .push(format!("{}: {error}", sysroot.display_path(&dir)));
                continue;
            }
        };
        if !canonical.starts_with(sysroot.root().canonicalize()?) || !seen.insert(canonical) {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                result
                    .warnings
                    .push(format!("{}: {error}", sysroot.display_path(&dir)));
                continue;
            }
        };
        for entry in entries {
            visited += 1;
            if visited > 8192 || result.files.len() >= 1024 {
                result.truncated = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    result.warnings.push(error.to_string());
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    result
                        .warnings
                        .push(format!("{}: {error}", sysroot.display_path(&entry.path())));
                    continue;
                }
            };
            if kind.is_dir() {
                if depth < 8 {
                    pending.push((entry.path(), depth + 1));
                } else {
                    result.truncated = true;
                }
            } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "ko") {
                result.files.push(Candidate {
                    path: sysroot.display_path(&entry.path()),
                    size: match entry.metadata() {
                        Ok(metadata) => metadata.len(),
                        Err(error) => {
                            result
                                .warnings
                                .push(format!("{}: {error}", sysroot.display_path(&entry.path())));
                            continue;
                        }
                    },
                });
            }
        }
        if visited > 8192 || result.files.len() >= 1024 {
            result.truncated = true;
            break;
        }
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result.files.dedup_by(|a, b| a.path == b.path);
    Ok(result)
}

#[cfg(test)]
pub(crate) fn fixture(vermagic: &str) -> Vec<u8> {
    fixture_elf(object::Architecture::Aarch64, object::Endianness::Little,
        format!("name=sample\0vermagic={vermagic}\0license=GPL\0parm=debug:Enable logging\0alias=test:first\0alias=test:second\0").as_bytes())
}

#[cfg(test)]
pub(crate) fn fixture_elf(
    architecture: object::Architecture,
    endian: object::Endianness,
    metadata: &[u8],
) -> Vec<u8> {
    use object::write::Object as Writer;
    let mut object = Writer::new(object::BinaryFormat::Elf, architecture, endian);
    let section = object.add_section(
        vec![],
        b".modinfo".to_vec(),
        object::SectionKind::ReadOnlyData,
    );
    object.append_section_data(section, metadata, 1);
    object.write().unwrap()
}
