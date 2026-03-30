use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{errors::AppError, files::write_string_atomic, paths::AppPaths};

pub const DEFAULT_SERVER_URL: &str = "https://remoteprovisioning.googleapis.com/v1";
pub const DEFAULT_OUTPUT_PATH: &str = "var/outputs/keybox.xml";

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum KeySource {
    Unset,
    Seed {
        seed_hex: String,
    },
    HwKey {
        hw_key_hex: String,
        kdf_label: String,
    },
}

impl Default for KeySource {
    fn default() -> Self {
        Self::Unset
    }
}

impl KeySource {
    pub fn mode_label(&self) -> &'static str {
        match self {
            KeySource::Unset => "unset",
            KeySource::Seed { .. } => "direct-seed",
            KeySource::HwKey { .. } => "hw-kdf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub brand: String,
    pub model: String,
    pub device: String,
    pub product: String,
    pub manufacturer: String,
    pub fused: u64,
    pub vb_state: String,
    pub os_version: String,
    pub security_level: String,
    pub bootloader_state: String,
    pub boot_patch_level: u32,
    pub system_patch_level: u32,
    pub vendor_patch_level: u32,
    #[serde(default)]
    pub vbmeta_digest: Option<String>,
    pub dice_issuer: String,
    pub dice_subject: String,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            brand: String::new(),
            model: String::new(),
            device: String::new(),
            product: String::new(),
            manufacturer: String::new(),
            fused: 0,
            vb_state: String::new(),
            os_version: String::new(),
            security_level: String::new(),
            bootloader_state: String::new(),
            boot_patch_level: 0,
            system_patch_level: 0,
            vendor_patch_level: 0,
            vbmeta_digest: None,
            dice_issuer: String::new(),
            dice_subject: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintConfig {
    pub value: String,
}

impl Default for FingerprintConfig {
    fn default() -> Self {
        Self {
            value: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    #[serde(default)]
    pub key_source: KeySource,
    #[serde(default)]
    pub device: DeviceInfo,
    #[serde(default)]
    pub fingerprint: FingerprintConfig,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default = "default_num_keys")]
    pub num_keys: u32,
    #[serde(default = "default_output_path")]
    pub output_path: String,
}

impl Default for ProfileData {
    fn default() -> Self {
        Self {
            key_source: KeySource::Unset,
            device: DeviceInfo::default(),
            fingerprint: FingerprintConfig::default(),
            server_url: default_server_url(),
            num_keys: default_num_keys(),
            output_path: default_output_path(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunOverrides {
    pub profile_name: Option<String>,
    pub seed_hex: Option<String>,
    pub hw_key_hex: Option<String>,
    pub kdf_label: Option<String>,
    pub server_url: Option<String>,
    pub num_keys: Option<u32>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedProfile {
    pub profile: ProfileData,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicProfileDisk {
    #[serde(default)]
    key_source: PublicKeySourceDisk,
    #[serde(default)]
    device: DeviceInfo,
    #[serde(default)]
    fingerprint: FingerprintConfig,
    #[serde(default = "default_server_url")]
    server_url: String,
    #[serde(default = "default_num_keys")]
    num_keys: u32,
    #[serde(default = "default_output_path")]
    output_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum KeySourceKindDisk {
    #[default]
    Unset,
    Seed,
    HwKey,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PublicKeySourceDisk {
    #[serde(default)]
    kind: KeySourceKindDisk,
    #[serde(default)]
    kdf_label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct SecretProfileDisk {
    #[serde(default)]
    seed_hex: Option<String>,
    #[serde(default)]
    hw_key_hex: Option<String>,
}

pub fn default_server_url() -> String {
    DEFAULT_SERVER_URL.into()
}

pub fn default_num_keys() -> u32 {
    1
}

pub fn default_output_path() -> String {
    DEFAULT_OUTPUT_PATH.into()
}

pub fn validate_profile_name(profile_name: Option<&str>) -> Result<()> {
    if let Some(name) = profile_name
        && name != "default"
    {
        return Err(AppError::UnsupportedProfile(name.to_owned()).into());
    }

    Ok(())
}

pub fn show_profile(paths: &AppPaths, profile_name: Option<&str>) -> Result<ProfileData> {
    validate_profile_name(profile_name)?;
    load_profile(paths)
}

pub fn save_profile(
    paths: &AppPaths,
    profile_name: Option<&str>,
    profile: &ProfileData,
) -> Result<ProfileData> {
    validate_profile_name(profile_name)?;
    paths.ensure_runtime_dirs()?;

    let normalized = normalize_profile(profile.clone())?;

    let public = PublicProfileDisk::from(&normalized);
    let secret = SecretProfileDisk::from(&normalized);

    let public_toml = toml::to_string_pretty(&public).context("serialize public profile")?;
    let secret_toml = toml::to_string_pretty(&secret).context("serialize secret profile")?;

    write_string_atomic(&paths.profile_path, &public_toml)?;
    write_string_atomic(&paths.profile_secrets_path, &secret_toml)?;

    load_profile(paths)
}

pub fn clear_profile(paths: &AppPaths, profile_name: Option<&str>) -> Result<()> {
    validate_profile_name(profile_name)?;

    for file in [&paths.profile_path, &paths.profile_secrets_path] {
        if file.exists() {
            fs::remove_file(file).with_context(|| format!("remove {}", file.display()))?;
        }
    }

    Ok(())
}

pub fn resolve_profile(paths: &AppPaths, overrides: &RunOverrides) -> Result<ResolvedProfile> {
    validate_profile_name(overrides.profile_name.as_deref())?;

    let mut profile = normalize_profile(load_profile(paths)?)?;

    if let Some(seed_hex) = overrides.seed_hex.clone() {
        profile.key_source = KeySource::Seed { seed_hex };
    }

    if let Some(hw_key_hex) = overrides.hw_key_hex.clone() {
        let kdf_label = overrides
            .kdf_label
            .clone()
            .ok_or(AppError::MissingKdfLabel)?;
        profile.key_source = KeySource::HwKey {
            hw_key_hex,
            kdf_label,
        };
    }

    if let Some(server_url) = overrides.server_url.clone() {
        profile.server_url = server_url;
    }

    if let Some(num_keys) = overrides.num_keys {
        profile.num_keys = num_keys.max(1);
    }

    if let Some(output_path) = overrides.output_path.clone() {
        profile.output_path = output_path;
    }

    profile = normalize_profile(profile)?;

    let output_path = paths.resolve_in_root(&profile.output_path);
    Ok(ResolvedProfile {
        profile,
        output_path,
    })
}

fn load_profile(paths: &AppPaths) -> Result<ProfileData> {
    let public = if paths.profile_path.exists() {
        let content = fs::read_to_string(&paths.profile_path)
            .with_context(|| format!("read {}", paths.profile_path.display()))?;
        toml::from_str::<PublicProfileDisk>(&content).context("parse profile.toml")?
    } else {
        PublicProfileDisk::from(&ProfileData::default())
    };

    let secrets = if paths.profile_secrets_path.exists() {
        let content = fs::read_to_string(&paths.profile_secrets_path)
            .with_context(|| format!("read {}", paths.profile_secrets_path.display()))?;
        toml::from_str::<SecretProfileDisk>(&content).context("parse profile.secrets.toml")?
    } else {
        SecretProfileDisk::default()
    };

    Ok(ProfileData::from_disk(public, secrets))
}

impl ProfileData {
    fn from_disk(public: PublicProfileDisk, secrets: SecretProfileDisk) -> Self {
        let key_source = match public.key_source.kind {
            KeySourceKindDisk::Unset => KeySource::Unset,
            KeySourceKindDisk::Seed => KeySource::Seed {
                seed_hex: secrets.seed_hex.clone().unwrap_or_default(),
            },
            KeySourceKindDisk::HwKey => KeySource::HwKey {
                hw_key_hex: secrets.hw_key_hex.clone().unwrap_or_default(),
                kdf_label: public.key_source.kdf_label.unwrap_or_default(),
            },
        };

        Self {
            key_source,
            device: public.device,
            fingerprint: public.fingerprint,
            server_url: public.server_url,
            num_keys: public.num_keys.max(1),
            output_path: if public.output_path.trim().is_empty() {
                default_output_path()
            } else {
                public.output_path
            },
        }
    }
}

impl From<&ProfileData> for PublicProfileDisk {
    fn from(value: &ProfileData) -> Self {
        let (kind, kdf_label) = match &value.key_source {
            KeySource::Unset => (KeySourceKindDisk::Unset, None),
            KeySource::Seed { .. } => (KeySourceKindDisk::Seed, None),
            KeySource::HwKey { kdf_label, .. } => {
                (KeySourceKindDisk::HwKey, Some(kdf_label.clone()))
            }
        };

        Self {
            key_source: PublicKeySourceDisk { kind, kdf_label },
            device: value.device.clone(),
            fingerprint: value.fingerprint.clone(),
            server_url: value.server_url.clone(),
            num_keys: value.num_keys.max(1),
            output_path: if value.output_path.trim().is_empty() {
                default_output_path()
            } else {
                value.output_path.clone()
            },
        }
    }
}

impl From<&ProfileData> for SecretProfileDisk {
    fn from(value: &ProfileData) -> Self {
        match &value.key_source {
            KeySource::Unset => Self::default(),
            KeySource::Seed { seed_hex } => Self {
                seed_hex: Some(seed_hex.clone()),
                hw_key_hex: None,
            },
            KeySource::HwKey { hw_key_hex, .. } => Self {
                seed_hex: None,
                hw_key_hex: Some(hw_key_hex.clone()),
            },
        }
    }
}

fn normalize_profile(mut profile: ProfileData) -> Result<ProfileData> {
    profile.key_source = normalize_key_source(&profile.key_source)?;
    normalize_device_info(&mut profile.device);
    profile.fingerprint.value = profile.fingerprint.value.trim().to_owned();
    profile.server_url = normalize_server_url(&profile.server_url)?;
    profile.output_path = normalize_output_path(&profile.output_path)?;
    Ok(profile)
}

fn normalize_key_source(key_source: &KeySource) -> Result<KeySource> {
    Ok(match key_source {
        KeySource::Unset => KeySource::Unset,
        KeySource::Seed { seed_hex } => {
            let seed_hex = seed_hex.trim().to_owned();
            if seed_hex.is_empty() {
                KeySource::Unset
            } else {
                KeySource::Seed { seed_hex }
            }
        }
        KeySource::HwKey {
            hw_key_hex,
            kdf_label,
        } => {
            let hw_key_hex = hw_key_hex.trim().to_owned();
            let kdf_label = kdf_label.trim().to_owned();

            if hw_key_hex.is_empty() {
                KeySource::Unset
            } else if kdf_label.is_empty() {
                return Err(AppError::MissingKdfLabel.into());
            } else {
                KeySource::HwKey {
                    hw_key_hex,
                    kdf_label,
                }
            }
        }
    })
}

fn normalize_device_info(device: &mut DeviceInfo) {
    device.brand = device.brand.trim().to_owned();
    device.model = device.model.trim().to_owned();
    device.device = device.device.trim().to_owned();
    device.product = device.product.trim().to_owned();
    device.manufacturer = device.manufacturer.trim().to_owned();
    device.vb_state = device.vb_state.trim().to_ascii_lowercase();
    device.os_version = device.os_version.trim().to_owned();
    device.security_level = device.security_level.trim().to_ascii_lowercase();
    device.bootloader_state = device.bootloader_state.trim().to_ascii_lowercase();
    device.dice_issuer = device.dice_issuer.trim().to_owned();
    device.dice_subject = device.dice_subject.trim().to_owned();
    device.vbmeta_digest = device
        .vbmeta_digest
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
}

fn normalize_server_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::MissingServerUrl.into());
    }

    let mut url =
        Url::parse(trimmed).map_err(|_| AppError::InvalidServerUrl(trimmed.to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::InvalidServerUrl(trimmed.to_owned()).into());
    }

    url.set_query(None);
    url.set_fragment(None);

    let normalized_path = match url.path().trim_end_matches('/') {
        "" | "/" => "/v1".to_owned(),
        other => other.to_owned(),
    };
    url.set_path(&normalized_path);

    Ok(url.to_string())
}

fn normalize_output_path(value: &str) -> Result<String> {
    let normalized = value.trim().replace('\\', "/");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Ok(default_output_path());
    }

    if trimmed.ends_with('/') {
        return Err(AppError::InvalidOutputPath(trimmed.to_owned()).into());
    }

    let mut relative = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => relative.push(part),
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(AppError::InvalidOutputPath(trimmed.to_owned()).into());
            }
        }
    }

    if relative.as_os_str().is_empty() || relative.file_name().is_none() {
        return Err(AppError::InvalidOutputPath(trimmed.to_owned()).into());
    }

    Ok(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceInfo, KeySource, ProfileData, normalize_output_path, normalize_profile,
        normalize_server_url,
    };

    #[test]
    fn default_profile_is_blank() {
        let profile = ProfileData::default();
        assert!(matches!(profile.key_source, KeySource::Unset));
        assert_eq!(profile.device, DeviceInfo::default());
        assert!(profile.fingerprint.value.is_empty());
        assert_eq!(profile.num_keys, 1);
    }

    #[test]
    fn normalize_server_url_adds_default_version_path() {
        assert_eq!(
            normalize_server_url("https://remoteprovisioning.googleapis.com").unwrap(),
            "https://remoteprovisioning.googleapis.com/v1",
        );
    }

    #[test]
    fn normalize_server_url_trims_trailing_slash() {
        assert_eq!(
            normalize_server_url("https://remoteprovisioning.googleapis.com/v1/").unwrap(),
            "https://remoteprovisioning.googleapis.com/v1",
        );
    }

    #[test]
    fn normalize_profile_coerces_blank_hw_key_to_unset() {
        let profile = normalize_profile(ProfileData {
            key_source: KeySource::HwKey {
                hw_key_hex: "   ".into(),
                kdf_label: "rkp_bcc_km".into(),
            },
            ..ProfileData::default()
        })
        .unwrap();

        assert!(matches!(profile.key_source, KeySource::Unset));
    }

    #[test]
    fn normalize_profile_keeps_patch_levels_as_provided() {
        let profile = normalize_profile(ProfileData {
            device: DeviceInfo {
                system_patch_level: 202601,
                ..DeviceInfo::default()
            },
            ..ProfileData::default()
        })
        .unwrap();

        assert_eq!(profile.device.boot_patch_level, 0);
        assert_eq!(profile.device.system_patch_level, 202601);
        assert_eq!(profile.device.vendor_patch_level, 0);
    }

    #[test]
    fn normalize_output_path_rejects_module_escape() {
        let error = normalize_output_path("../outside.xml").unwrap_err();
        assert!(error.to_string().contains("output path"));
    }

    #[test]
    fn normalize_output_path_normalizes_windows_separators() {
        assert_eq!(
            normalize_output_path(r"var\outputs\keybox.xml").unwrap(),
            "var/outputs/keybox.xml",
        );
    }
}
