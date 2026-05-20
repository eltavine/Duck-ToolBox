use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use quick_xml::{Reader, events::Event, name::QName};
use serde::{Deserialize, Serialize};

use crate::runtime::{files::write_string_atomic, paths::AppPaths};

const TRICKY_STORE_MODULE_DIR: &str = "/data/adb/modules/tricky_store";
const TRICKY_STORE_DATA_DIR: &str = "/data/adb/tricky_store";
const TARGET_FILE: &str = "/data/adb/tricky_store/target.txt";
const SYSTEM_APP_FILE: &str = "/data/adb/tricky_store/system_app";
const KEYBOX_FILE: &str = "/data/adb/tricky_store/keybox.xml";
const AUTO_CONFIG_FILE_NAME: &str = "tricky-store-auto-target.toml";
const AUTO_CONFIG_VERSION: u8 = 1;

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
    #[serde(default = "default_auto_config_version")]
    pub version: u8,
    pub enabled: bool,
    #[serde(default)]
    pub baseline_initialized: bool,
    #[serde(default)]
    pub known_user_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDetection {
    pub installed: bool,
    pub module_dir: String,
    pub name: Option<String>,
    pub variant: Option<String>,
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

struct AutoTargetPlan {
    config: AutoTargetConfig,
    targets: Vec<TargetEntry>,
    added_count: usize,
    initialized_baseline: bool,
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
    let tricky_store = detect_tricky_store();

    Ok(StatusData {
        tricky_store,
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
        version: AUTO_CONFIG_VERSION,
        enabled: request.auto_add_new_apps,
        baseline_initialized: request.auto_add_new_apps,
        known_user_apps: if request.auto_add_new_apps {
            current_user_packages()
        } else {
            Vec::new()
        },
    };
    write_auto_config(paths, &auto_config)?;

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
    apply_auto_targets_with_config(paths, &auto_config)
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

fn default_auto_config_version() -> u8 {
    AUTO_CONFIG_VERSION
}

fn read_auto_config(paths: &AppPaths) -> Result<AutoTargetConfig> {
    let path = config_path(paths);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AutoTargetConfig {
                version: AUTO_CONFIG_VERSION,
                enabled: false,
                baseline_initialized: false,
                known_user_apps: Vec::new(),
            });
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };

    let config: AutoTargetConfig =
        toml::from_str(&content).with_context(|| format!("parse {}", path.display()))?;
    let original_version = config.version;
    let original_baseline_initialized = config.baseline_initialized;
    let original_known_user_apps = config.known_user_apps.clone();
    let mut config = normalize_auto_config(config);
    let mut needs_write = original_version != config.version
        || original_baseline_initialized != config.baseline_initialized
        || original_known_user_apps != config.known_user_apps;

    if config.enabled && !config.baseline_initialized {
        config.known_user_apps = current_user_packages();
        config.baseline_initialized = true;
        needs_write = true;
    }

    if needs_write {
        write_auto_config(paths, &config)?;
    }

    Ok(config)
}

fn write_auto_config(paths: &AppPaths, config: &AutoTargetConfig) -> Result<()> {
    paths.ensure_runtime_dirs()?;
    let content = toml::to_string_pretty(config).context("serialize auto target config")?;
    let path = config_path(paths);
    write_string_atomic(&path, &content).with_context(|| format!("write {}", path.display()))
}

fn normalize_auto_config(mut config: AutoTargetConfig) -> AutoTargetConfig {
    config.version = AUTO_CONFIG_VERSION;
    config.known_user_apps = normalize_package_list(config.known_user_apps);
    if !config.enabled {
        config.baseline_initialized = false;
        config.known_user_apps.clear();
    }
    config
}

fn current_user_packages() -> Vec<String> {
    normalize_package_list(list_pm_packages("-3"))
}

fn apply_auto_targets_with_config(
    paths: &AppPaths,
    config: &AutoTargetConfig,
) -> Result<AutoApplyData> {
    if !config.enabled {
        return Ok(AutoApplyData {
            enabled: false,
            added_count: 0,
            target_path: TARGET_FILE.into(),
        });
    }

    ensure_tricky_store_dir()?;
    let config = normalize_auto_config(config.clone());
    let user_packages = current_user_packages();

    let targets = if config.baseline_initialized {
        read_targets(Path::new(TARGET_FILE))?
    } else {
        Vec::new()
    };
    let plan = plan_auto_targets(config, targets, user_packages);

    if !plan.initialized_baseline {
        write_targets(Path::new(TARGET_FILE), &plan.targets)?;
    }
    write_auto_config(paths, &plan.config)?;

    if plan.initialized_baseline {
        return Ok(AutoApplyData {
            enabled: true,
            added_count: 0,
            target_path: TARGET_FILE.into(),
        });
    }

    Ok(AutoApplyData {
        enabled: true,
        added_count: plan.added_count,
        target_path: TARGET_FILE.into(),
    })
}

fn plan_auto_targets(
    config: AutoTargetConfig,
    targets: Vec<TargetEntry>,
    user_packages: Vec<String>,
) -> AutoTargetPlan {
    let mut config = normalize_auto_config(config);
    let user_packages = normalize_package_list(user_packages);
    let targets = normalize_targets(targets);

    if !config.baseline_initialized {
        config.known_user_apps = user_packages;
        config.baseline_initialized = true;
        return AutoTargetPlan {
            config,
            targets,
            added_count: 0,
            initialized_baseline: true,
        };
    }

    let known_user_apps = config
        .known_user_apps
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut targets = targets;
    let mut existing = targets
        .iter()
        .map(|entry| entry.package_name.clone())
        .collect::<BTreeSet<_>>();
    let mut added_count = 0;

    for package_name in &user_packages {
        if !known_user_apps.contains(package_name) && existing.insert(package_name.clone()) {
            targets.push(TargetEntry {
                package_name: package_name.clone(),
                mode: TargetMode::Auto,
            });
            added_count += 1;
        }
    }

    targets = normalize_targets(targets);
    config.known_user_apps = user_packages;

    AutoTargetPlan {
        config,
        targets,
        added_count,
        initialized_baseline: false,
    }
}

fn detect_tricky_store() -> ModuleDetection {
    detect_tricky_store_from_path(Path::new(TRICKY_STORE_MODULE_DIR))
}

fn detect_tricky_store_from_path(path: &Path) -> ModuleDetection {
    let module_dir_exists = path.is_dir();
    let props = if module_dir_exists {
        parse_module_prop(&path.join("module.prop"))
    } else {
        BTreeMap::new()
    };
    let variant = module_dir_exists.then(|| classify_tricky_store_variant(&props));

    ModuleDetection {
        installed: module_dir_exists,
        module_dir: path.display().to_string(),
        name: props.get("name").cloned(),
        variant,
        version: props.get("version").cloned(),
        version_code: props
            .get("versionCode")
            .and_then(|value| value.trim().parse::<u64>().ok()),
    }
}

fn classify_tricky_store_variant(props: &BTreeMap<String, String>) -> String {
    let haystack = props
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();

    if haystack.contains("jingmatrix")
        || haystack.contains("teesimulator")
        || haystack.contains("tee simulator")
    {
        return "tee-simulator".into();
    }

    if haystack.contains("tricky store") || haystack.contains("trickystore") {
        return "tricky-store".into();
    }

    "unknown".into()
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
        if seen.insert(package_name.clone()) {
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

    let mut reader = Reader::from_reader(Cursor::new(trimmed.as_bytes()));
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut keybox_seen = false;

    loop {
        match reader.read_event_into(&mut buffer).with_context(|| {
            format!(
                "parse keybox XML near byte offset {}",
                reader.buffer_position()
            )
        })? {
            Event::Start(event) => {
                if depth == 0 {
                    if root_seen {
                        bail!("keybox XML must contain a single AndroidAttestation document");
                    }
                    if event.name() != QName(b"AndroidAttestation") {
                        bail!("keybox XML root must be AndroidAttestation");
                    }
                    root_seen = true;
                } else if root_closed {
                    bail!("keybox XML must contain a single AndroidAttestation document");
                }

                if root_seen && event.name() == QName(b"Keybox") {
                    keybox_seen = true;
                }
                depth += 1;
            }
            Event::Empty(event) => {
                if depth == 0 {
                    if root_seen {
                        bail!("keybox XML must contain a single AndroidAttestation document");
                    }
                    if event.name() != QName(b"AndroidAttestation") {
                        bail!("keybox XML root must be AndroidAttestation");
                    }
                    root_seen = true;
                    root_closed = true;
                } else if !root_closed && event.name() == QName(b"Keybox") {
                    keybox_seen = true;
                }
            }
            Event::End(event) => {
                if depth == 0 {
                    bail!("keybox XML has an unexpected closing tag");
                }
                depth -= 1;
                if depth == 0 {
                    if event.name() != QName(b"AndroidAttestation") {
                        bail!("keybox XML root must be AndroidAttestation");
                    }
                    root_closed = true;
                }
            }
            Event::Eof => break,
            Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::Comment(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::GeneralRef(_) => {}
        }
        buffer.clear();
    }

    if !root_seen {
        bail!("keybox XML must contain an AndroidAttestation document");
    }

    if !root_closed || depth != 0 {
        bail!("keybox XML has an unclosed AndroidAttestation document");
    }

    if !keybox_seen {
        bail!("keybox XML must contain at least one Keybox entry");
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
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        AUTO_CONFIG_VERSION, AutoTargetConfig, TargetEntry, TargetMode,
        detect_tricky_store_from_path, parse_target_line, plan_auto_targets, validate_keybox_xml,
    };

    fn temp_module_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "duck-tricky-store-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

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

    #[test]
    fn validates_self_closing_keybox_entry() {
        validate_keybox_xml(
            r#"<?xml version="1.0"?><AndroidAttestation><Keybox DeviceID="x"/></AndroidAttestation>"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_non_attestation_keybox_xml() {
        assert!(validate_keybox_xml(r#"<Keybox DeviceID="x"></Keybox>"#).is_err());
    }

    #[test]
    fn rejects_plain_text_keybox_markers() {
        assert!(
            validate_keybox_xml(
                r#"<AndroidAttestation><Note>&lt;Keybox DeviceID="x"&gt;</Note></AndroidAttestation>"#,
            )
            .is_err()
        );
    }

    #[test]
    fn classifies_jingmatrix_as_tee_simulator() {
        let props = [
            ("name".into(), "TEE Simulator".into()),
            ("author".into(), "JingMatrix".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            super::classify_tricky_store_variant(&props),
            "tee-simulator"
        );
    }

    #[test]
    fn detects_tee_simulator_name_under_tricky_store_dir_as_same_module() {
        let module_dir = temp_module_dir("tees");
        fs::write(
            module_dir.join("module.prop"),
            "name=TEE Simulator\nauthor=JingMatrix\nversion=1\nversionCode=2\n",
        )
        .unwrap();

        let tricky_store = detect_tricky_store_from_path(&module_dir);

        assert!(tricky_store.installed);
        assert_eq!(tricky_store.name.as_deref(), Some("TEE Simulator"));
        assert_eq!(tricky_store.variant.as_deref(), Some("tee-simulator"));
        assert_eq!(tricky_store.module_dir, module_dir.display().to_string());
    }

    #[test]
    fn missing_module_dir_ignores_neighbor_module_prop_file() {
        let module_dir = std::env::temp_dir().join(format!(
            "duck-tricky-store-missing-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&module_dir, "name=TEE Simulator\n").unwrap();

        let tricky_store = detect_tricky_store_from_path(&module_dir);

        assert!(!tricky_store.installed);
        assert!(tricky_store.name.is_none());
        assert!(tricky_store.variant.is_none());
    }

    #[test]
    fn detects_regular_tricky_store_variant_under_tricky_store_dir() {
        let module_dir = temp_module_dir("ts");
        fs::write(
            module_dir.join("module.prop"),
            "name=Tricky Store\nversion=1\nversionCode=2\n",
        )
        .unwrap();

        let tricky_store = detect_tricky_store_from_path(&module_dir);

        assert!(tricky_store.installed);
        assert_eq!(tricky_store.name.as_deref(), Some("Tricky Store"));
        assert_eq!(tricky_store.variant.as_deref(), Some("tricky-store"));
    }

    #[test]
    fn auto_target_plan_initializes_legacy_enabled_config_without_adding_targets() {
        let plan = plan_auto_targets(
            AutoTargetConfig {
                version: AUTO_CONFIG_VERSION,
                enabled: true,
                baseline_initialized: false,
                known_user_apps: Vec::new(),
            },
            Vec::new(),
            vec!["com.example.existing".into()],
        );

        assert!(plan.initialized_baseline);
        assert_eq!(plan.added_count, 0);
        assert!(plan.targets.is_empty());
        assert_eq!(plan.config.known_user_apps, vec!["com.example.existing"]);
        assert!(plan.config.baseline_initialized);
    }

    #[test]
    fn auto_target_plan_adds_only_new_user_apps_after_baseline() {
        let plan = plan_auto_targets(
            AutoTargetConfig {
                version: AUTO_CONFIG_VERSION,
                enabled: true,
                baseline_initialized: true,
                known_user_apps: vec!["com.example.old".into()],
            },
            vec![TargetEntry {
                package_name: "com.example.selected".into(),
                mode: TargetMode::Hack,
            }],
            vec![
                "com.example.old".into(),
                "com.example.new".into(),
                "com.example.selected".into(),
            ],
        );

        assert!(!plan.initialized_baseline);
        assert_eq!(plan.added_count, 1);
        assert_eq!(
            plan.targets
                .iter()
                .map(|entry| (entry.package_name.as_str(), &entry.mode))
                .collect::<Vec<_>>(),
            vec![
                ("com.example.new", &TargetMode::Auto),
                ("com.example.selected", &TargetMode::Hack),
            ]
        );
        assert_eq!(
            plan.config.known_user_apps,
            vec!["com.example.new", "com.example.old", "com.example.selected"]
        );
    }
}
