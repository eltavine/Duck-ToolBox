//! Xposed module detection, used by "Deselect unnecessary".
//!
//! Xposed modules never check the bootloader, so targeting them is pointless. A module is
//! recognised by the entry points LSPosed loads: legacy `assets/xposed_init` or the modern
//! libxposed `META-INF/xposed/*` files. Results are cached per APK path, size and mtime.

use std::{collections::BTreeMap, fs::File, path::Path};

use anyhow::{Context as _, Result};
use duck_core::{Context, fs::modified_unix, fs::write_string_atomic};
use duck_platform::packages::{self, PackageFilter};
use serde::{Deserialize, Serialize};

const CACHE_FILE: &str = "tricky-store-xposed.json";
const XPOSED_MARKERS: &[&str] = &[
    "assets/xposed_init",
    "META-INF/xposed/java_init.list",
    "META-INF/xposed/module.prop",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheEntry {
    apk: String,
    size: u64,
    modified: u64,
    xposed: bool,
}

pub fn xposed_modules(ctx: &Context) -> Result<Vec<String>> {
    let cache_path = ctx.paths.var_dir.join(CACHE_FILE);
    let cache: BTreeMap<String, CacheEntry> = duck_core::fs::read_optional(&cache_path)?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    let mut fresh = BTreeMap::new();
    let mut modules = Vec::new();
    for (package, apk) in packages::list_with_paths(PackageFilter::User)? {
        let host = ctx.sysroot.path(&apk);
        let Ok(metadata) = std::fs::metadata(&host) else {
            continue;
        };
        let (size, modified) = (metadata.len(), modified_unix(&metadata));

        let xposed = match cache.get(&package) {
            Some(entry) if entry.apk == apk && entry.size == size && entry.modified == modified => {
                entry.xposed
            }
            _ => is_xposed_apk(&host).unwrap_or(false),
        };
        if xposed {
            modules.push(package.clone());
        }
        fresh.insert(
            package,
            CacheEntry {
                apk,
                size,
                modified,
                xposed,
            },
        );
    }

    if fresh != cache {
        ctx.paths.ensure_runtime_dirs()?;
        let body = serde_json::to_string(&fresh).context("serialize Xposed cache")?;
        write_string_atomic(&cache_path, &body)?;
    }
    Ok(modules)
}

/// Reads only the ZIP central directory, so even large APKs are checked quickly.
fn is_xposed_apk(path: &Path) -> Result<bool> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let archive = zip::ZipArchive::new(file).with_context(|| format!("read {}", path.display()))?;
    Ok(XPOSED_MARKERS
        .iter()
        .any(|marker| archive.index_for_name(marker).is_some()))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::is_xposed_apk;

    fn apk_with(entries: &[&str]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for entry in entries {
            zip.start_file(*entry, options).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        file
    }

    #[test]
    fn detects_legacy_and_modern_modules() {
        assert!(is_xposed_apk(apk_with(&["classes.dex", "assets/xposed_init"]).path()).unwrap());
        assert!(is_xposed_apk(apk_with(&["META-INF/xposed/java_init.list"]).path()).unwrap());
        assert!(is_xposed_apk(apk_with(&["META-INF/xposed/module.prop"]).path()).unwrap());
        assert!(!is_xposed_apk(apk_with(&["classes.dex", "assets/other"]).path()).unwrap());
    }

    #[test]
    fn ignores_unrelated_non_utf8_names() {
        for (entry, expected) in [("assets/xposed_init", true), ("classes.dex", false)] {
            let apk = apk_with(&["assets/other", entry]);
            let mut bytes = std::fs::read(apk.path()).unwrap();
            let name = b"assets/other";
            for offset in 0..=bytes.len() - name.len() {
                if bytes[offset..].starts_with(name) {
                    bytes[offset + name.len() - 1] = 0xff;
                }
            }
            std::fs::write(apk.path(), bytes).unwrap();

            assert_eq!(is_xposed_apk(apk.path()).unwrap(), expected);
        }
    }

    #[test]
    fn requires_an_exact_marker_path() {
        assert!(
            !is_xposed_apk(
                apk_with(&[
                    "assets/xposed_init.bak",
                    "META-INF/xposed/java_init.list.bak",
                    "META-INF/xposed/module.properties",
                    "nested/assets/xposed_init",
                ])
                .path()
            )
            .unwrap()
        );
        assert!(!is_xposed_apk(apk_with(&[]).path()).unwrap());
    }
}
