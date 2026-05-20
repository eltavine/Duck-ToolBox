use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::runtime::{files::write_string_atomic, paths::AppPaths};

const TRICKY_STORE_MODULE_DIR: &str = "/data/adb/modules/tricky_store";
const TRICKY_STORE_DATA_DIR: &str = "/data/adb/tricky_store";
const TEESIM_MODULE_DIRS: [&str; 3] = [
    "/data/adb/modules/tee_simulator",
    "/data/adb/modules/teesimulator",
    "/data/adb/modules/TEESimulator",
];
const TARGET_FILE: &str = "/data/adb/tricky_store/target.txt";
const SYSTEM_APP_FILE: &str = "/data/adb/tricky_store/system_app";
const KEYBOX_FILE: &str = "/data/adb/tricky_store/keybox.xml";
const AUTO_CONFIG_FILE_NAME: &str = "tricky-store-auto-target.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetMode {
    Auto,
    Generate,
    Hack,
}

impl Default for TargetMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl TargetMode {
    fn marker(&self) -> &'static str {
        match self {
            Self::Auto => "",
            Self::Generate => "!",
            Self::Hack => "?",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetEntry {
    pub package_name: String,
    #[serde(default)]
    pub mode: TargetMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSaveRequest {
    #[serde(default)]
    pub targets: Vec<TargetEntry>,
    #[serde(default)]
    pub system_apps: Vec<String>,
    #[serde(default)]
    pub auto_add_new_apps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTargetConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDetection {
    pub installed: bool,
    pub module_dir: String,
    pub version: Option<String>,
    pub version_code: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageEntry {
    pub package_name: String,
    pub app_label: String,
    pub system: bool,
    pub selected: bool,
    pub mode: TargetMode,
    pub tracked_system: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboxStatus {
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub modified_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusData {
    pub tricky_store: ModuleDetection,
    pub tee_simulator: ModuleDetection,
    pub target_path: String,
    pub system_app_path: String,
    pub keybox: KeyboxStatus,
    pub auto_config: AutoTargetConfig,
    pub targets: Vec<TargetEntry>,
    pub system_apps: Vec<String>,
    pub packages: Vec<PackageEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetSaveData {
    pub target_path: String,
    pub system_app_path: String,
    pub auto_config: AutoTargetConfig,
    pub target_count: usize,
    pub system_app_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoApplyData {
    pub enabled: bool,
    pub added_count: usize,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboxInstallData {
    pub source_path: String,
    pub target_path: String,
    pub backup_path: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileListData {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
}

pub fn status(paths: &AppPaths) -> Result<StatusData> {
    let targets = read_targets(Path::new(TARGET_FILE))?;
    let system_apps = read_plain_list(Path::new(SYSTEM_APP_FILE))?;
    let auto_config = read_auto_config(paths)?;
    let target_map = target_map(&targets);
    let system_app_set = system_apps.iter().cloned().collect::<BTreeSet<_>>();
    let packages = installed_packages(&target_map, &system_app_set);

    Ok(StatusData {
        tricky_store: detect_module(Path::new(TRICKY_STORE_MODULE_DIR)),
        tee_simulator: detect_tee_simulator(),
        target_path: TARGET_FILE.into(),
        system_app_path: SYSTEM_APP_FILE.into(),
        keybox: keybox_status(Path::new(KEYBOX_FILE)),
        auto_config,
        targets,
        system_apps,
        packages,
    })
}

pub fn save_targets(paths: &AppPaths, request: TargetSaveRequest) -> Result<TargetSaveData> {
    ensure_tricky_store_dir()?;

    let targets = normalize_targets(request.targets);
    let system_apps = normalize_package_list(request.system_apps);
    write_targets(Path::new(TARGET_FILE), &targets)?;
    write_plain_list(Path::new(SYSTEM_APP_FILE), &system_apps)?;

    let auto_config = AutoTargetConfig {
        enabled: request.auto_add_new_apps,
    };
    write_auto_config(paths, &auto_config)?;

    if auto_config.enabled {
        apply_auto_targets_with_config(&auto_config)?;
    }

    Ok(TargetSaveData {
        target_path: TARGET_FILE.into(),
        system_app_path: SYSTEM_APP_FILE.into(),
        auto_config,
        target_count: targets.len(),
        system_app_count: system_apps.len(),
    })
}

pub fn apply_auto_targets(paths: &AppPaths) -> Result<AutoApplyData> {
    let auto_config = read_auto_config(paths)?;
    apply_auto_targets_with_config(&auto_config)
}

pub fn install_keybox(paths: &AppPaths, source: &str) -> Result<KeyboxInstallData> {
    let source_path = paths.resolve_in_root(source.trim());
    if !source_path.is_file() {
        bail!("keybox source file not found: {}", source_path.display());
    }

    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("read {}", source_path.display()))?;
    validate_keybox_xml(&source_text)?;
    ensure_tricky_store_dir()?;

    let target_path = PathBuf::from(KEYBOX_FILE);
    let backup_path = backup_existing_file(&target_path)?;
    write_string_atomic(&target_path, &source_text)
        .with_context(|| format!("write {}", target_path.display()))?;
    set_keybox_permissions(&target_path);
    let size = fs::metadata(&target_path)
        .map(|metadata| metadata.len())
        .unwrap_or(source_text.len() as u64);

    Ok(KeyboxInstallData {
        source_path: source_path.display().to_string(),
        target_path: target_path.display().to_string(),
        backup_path: backup_path.map(|path| path.display().to_string()),
        size,
    })
}

pub fn import_keybox(paths: &AppPaths, source: &str) -> Result<KeyboxInstallData> {
    install_keybox(paths, source)
}

pub fn list_files(paths: &AppPaths, directory: &str, extension: &str) -> Result<FileListData> {
    let path = paths.resolve_in_root(directory.trim());
    if !path.is_dir() {
        bail!("directory not found: {}", path.display());
    }

    let wanted_extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let mut entries = Vec::new();

    for entry in fs::read_dir(&path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("read metadata for {}", entry_path.display()))?;
        let is_directory = metadata.is_dir();
        let matches_extension = wanted_extension.is_empty()
            || entry_path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(&wanted_extension));

        if !is_directory && !matches_extension {
            continue;
        }

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry_path.display().to_string(),
            directory: is_directory,
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }

    entries.sort_by(|left, right| {
        right
            .directory
            .cmp(&left.directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let parent = path.parent().map(|value| value.display().to_string());

    Ok(FileListData {
        path: path.display().to_string(),
        parent,
        entries,
    })
}

pub fn config_path(paths: &AppPaths) -> PathBuf {
    paths.var_dir.join(AUTO_CONFIG_FILE_NAME)
}

fn ensure_tricky_store_dir() -> Result<()> {
    fs::create_dir_all(TRICKY_STORE_DATA_DIR).context("create Tricky Store data directory")
}

fn read_targets(path: &Path) -> Result<Vec<TargetEntry>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };

    let mut targets = Vec::new();
    let mut seen = BTreeSet::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (package_name, mode) = parse_target_line(trimmed);
        if package_name.is_empty() || !seen.insert(package_name.clone()) {
            continue;
        }

        targets.push(TargetEntry { package_name, mode });
    }

    Ok(targets)
}

fn parse_target_line(line: &str) -> (String, TargetMode) {
    if let Some(value) = line.strip_suffix('!') {
        return (value.trim().into(), TargetMode::Generate);
    }

    if let Some(value) = line.strip_suffix('?') {
        return (value.trim().into(), TargetMode::Hack);
    }

    (
        line.trim_end_matches(['!', '?']).trim().into(),
        TargetMode::Auto,
    )
}

fn normalize_targets(targets: Vec<TargetEntry>) -> Vec<TargetEntry> {
    let mut by_package = BTreeMap::<String, TargetMode>::new();
    for entry in targets {
        let package_name = normalize_package_name(&entry.package_name);
        if !package_name.is_empty() {
            by_package.insert(package_name, entry.mode);
        }
    }

    by_package
        .into_iter()
        .map(|(package_name, mode)| TargetEntry { package_name, mode })
        .collect()
}

fn normalize_package_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| normalize_package_name(&value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_package_name(value: &str) -> String {
    value.trim().trim_end_matches(['!', '?']).trim().to_owned()
}

fn write_targets(path: &Path, targets: &[TargetEntry]) -> Result<()> {
    let content = targets
        .iter()
        .map(|entry| format!("{}{}", entry.package_name, entry.mode.marker()))
        .collect::<Vec<_>>()
        .join("\n");
    let content = if content.is_empty() {
        String::new()
    } else {
        format!("{content}\n")
    };

    write_string_atomic(path, &content).with_context(|| format!("write {}", path.display()))
}

fn read_plain_list(path: &Path) -> Result<Vec<String>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };

    Ok(content
        .lines()
        .map(normalize_package_name)
        .filter(|value| !value.is_empty() && !value.starts_with('#'))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn write_plain_list(path: &Path, entries: &[String]) -> Result<()> {
    let content = if entries.is_empty() {
        String::new()
    } else {
        format!("{}\n", entries.join("\n"))
    };

    write_string_atomic(path, &content).with_context(|| format!("write {}", path.display()))
}

fn read_auto_config(paths: &AppPaths) -> Result<AutoTargetConfig> {
    let path = config_path(paths);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AutoTargetConfig { enabled: false });
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };

    toml::from_str(&content).with_context(|| format!("parse {}", path.display()))
}

fn write_auto_config(paths: &AppPaths, config: &AutoTargetConfig) -> Result<()> {
    paths.ensure_runtime_dirs()?;
    let content = toml::to_string_pretty(config).context("serialize auto target config")?;
    let path = config_path(paths);
    write_string_atomic(&path, &content).with_context(|| format!("write {}", path.display()))
}

fn apply_auto_targets_with_config(config: &AutoTargetConfig) -> Result<AutoApplyData> {
    if !config.enabled {
        return Ok(AutoApplyData {
            enabled: false,
            added_count: 0,
            target_path: TARGET_FILE.into(),
        });
    }

    ensure_tricky_store_dir()?;
    let mut targets = read_targets(Path::new(TARGET_FILE))?;
    let mut existing = targets
        .iter()
        .map(|entry| entry.package_name.clone())
        .collect::<BTreeSet<_>>();
    let user_packages = list_pm_packages("-3");
    let mut added_count = 0;

    for package_name in user_packages {
        if existing.insert(package_name.clone()) {
            targets.push(TargetEntry {
                package_name,
                mode: TargetMode::Auto,
            });
            added_count += 1;
        }
    }

    targets = normalize_targets(targets);
    write_targets(Path::new(TARGET_FILE), &targets)?;

    Ok(AutoApplyData {
        enabled: true,
        added_count,
        target_path: TARGET_FILE.into(),
    })
}

fn detect_module(path: &Path) -> ModuleDetection {
    let installed = path.is_dir();
    let module_prop = parse_module_prop(&path.join("module.prop"));

    ModuleDetection {
        installed,
        module_dir: path.display().to_string(),
        version: module_prop.get("version").cloned(),
        version_code: module_prop
            .get("versionCode")
            .and_then(|value| value.trim().parse::<u64>().ok()),
    }
}

fn detect_tee_simulator() -> ModuleDetection {
    for candidate in TEESIM_MODULE_DIRS {
        let path = Path::new(candidate);
        if path.is_dir() {
            return detect_module(path);
        }
    }

    detect_module(Path::new(TEESIM_MODULE_DIRS[0]))
}

fn parse_module_prop(path: &Path) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return props;
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            props.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }

    props
}

fn target_map(targets: &[TargetEntry]) -> BTreeMap<String, TargetMode> {
    targets
        .iter()
        .map(|entry| (entry.package_name.clone(), entry.mode.clone()))
        .collect()
}

fn installed_packages(
    target_map: &BTreeMap<String, TargetMode>,
    system_app_set: &BTreeSet<String>,
) -> Vec<PackageEntry> {
    let mut packages = Vec::new();
    let user_packages = list_pm_packages("-3");
    let system_packages = list_pm_packages("-s");
    let all_system = system_packages.iter().cloned().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();

    for package_name in user_packages {
        if seen.insert(package_name.clone()) {
            packages.push(package_entry(
                package_name,
                false,
                target_map,
                system_app_set,
            ));
        }
    }

    for package_name in system_packages {
        if (system_app_set.contains(&package_name) || target_map.contains_key(&package_name))
            && seen.insert(package_name.clone())
        {
            packages.push(package_entry(
                package_name,
                true,
                target_map,
                system_app_set,
            ));
        }
    }

    for package_name in target_map.keys() {
        if seen.insert(package_name.clone()) {
            packages.push(package_entry(
                package_name.clone(),
                all_system.contains(package_name),
                target_map,
                system_app_set,
            ));
        }
    }

    packages.sort_by(|left, right| {
        right
            .selected
            .cmp(&left.selected)
            .then_with(|| left.system.cmp(&right.system))
            .then_with(|| left.package_name.cmp(&right.package_name))
    });
    packages
}

fn package_entry(
    package_name: String,
    system: bool,
    target_map: &BTreeMap<String, TargetMode>,
    system_app_set: &BTreeSet<String>,
) -> PackageEntry {
    PackageEntry {
        app_label: package_name.clone(),
        selected: target_map.contains_key(&package_name),
        mode: target_map
            .get(&package_name)
            .cloned()
            .unwrap_or(TargetMode::Auto),
        tracked_system: system_app_set.contains(&package_name),
        package_name,
        system,
    }
}

fn list_pm_packages(flag: &str) -> Vec<String> {
    let output = Command::new("pm")
        .args(["list", "packages", flag])
        .output()
        .ok();
    let Some(output) = output.filter(|output| output.status.success()) else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:").map(str::to_owned))
        .filter(|value| !value.is_empty())
        .collect()
}

fn keybox_status(path: &Path) -> KeyboxStatus {
    let metadata = fs::metadata(path).ok();
    KeyboxStatus {
        path: path.display().to_string(),
        exists: metadata.as_ref().is_some_and(|value| value.is_file()),
        size: metadata.as_ref().map(|value| value.len()).unwrap_or(0),
        modified_unix: metadata
            .and_then(|value| value.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
    }
}

fn backup_existing_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let backup = path.with_file_name(format!(
        "{}.{timestamp}.bak",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("keybox.xml")
    ));
    fs::rename(path, &backup).with_context(|| {
        format!(
            "backup existing keybox {} -> {}",
            path.display(),
            backup.display()
        )
    })?;
    Ok(Some(backup))
}

fn validate_keybox_xml(content: &str) -> Result<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        bail!("keybox XML is empty");
    }

    if !trimmed.contains("<AndroidAttestation") || !trimmed.contains("</AndroidAttestation>") {
        return Err(anyhow!(
            "keybox XML must contain an AndroidAttestation document"
        ));
    }

    if !trimmed.contains("<Keybox") {
        return Err(anyhow!("keybox XML must contain at least one Keybox entry"));
    }

    Ok(())
}

fn set_keybox_permissions(_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(_path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o644);
            let _ = fs::set_permissions(_path, permissions);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TargetMode, parse_target_line, validate_keybox_xml};

    #[test]
    fn parses_target_suffix_modes() {
        assert_eq!(
            parse_target_line("com.example.app"),
            ("com.example.app".into(), TargetMode::Auto)
        );
        assert_eq!(
            parse_target_line("com.example.app!"),
            ("com.example.app".into(), TargetMode::Generate)
        );
        assert_eq!(
            parse_target_line("com.example.app?"),
            ("com.example.app".into(), TargetMode::Hack)
        );
    }

    #[test]
    fn validates_android_attestation_keybox() {
        validate_keybox_xml(
            r#"<AndroidAttestation><NumberOfKeyboxes>1</NumberOfKeyboxes><Keybox DeviceID="x"></Keybox></AndroidAttestation>"#,
        )
        .unwrap();
    }
}
