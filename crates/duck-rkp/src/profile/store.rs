//! Profile persistence. Public fields go to `profile.toml`; key material is split into
//! `profile.secrets.toml` so the public half can be shared without leaking secrets.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use duck_core::{AppPaths, fs::write_string_atomic};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::RkpError;

use super::{
    model::{
        DeviceInfo, DiceCurve, FingerprintConfig, KeySource, ProfileData, default_num_keys,
        default_output_path, default_server_url,
    },
    normalize::{normalize_diagnostic_profile, normalize_profile},
};

#[derive(Debug, Clone, Default)]
pub struct RunOverrides {
    pub profile_name: Option<String>,
    pub seed_hex: Option<String>,
    pub hw_key_hex: Option<String>,
    pub kdf_label: Option<String>,
    pub curve: Option<DiceCurve>,
    pub server_url: Option<String>,
    pub include_fingerprint: Option<bool>,
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
    curve: DiceCurve,
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

pub fn validate_profile_name(profile_name: Option<&str>) -> Result<()> {
    if let Some(name) = profile_name
        && name != "default"
    {
        return Err(RkpError::UnsupportedProfile(name.to_owned()).into());
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
    resolve_profile_with(paths, overrides, normalize_profile)
}

pub(crate) fn resolve_diagnostic_profile(
    paths: &AppPaths,
    overrides: &RunOverrides,
) -> Result<ResolvedProfile> {
    resolve_profile_with(paths, overrides, normalize_diagnostic_profile)
}

fn resolve_profile_with(
    paths: &AppPaths,
    overrides: &RunOverrides,
    normalize: fn(ProfileData) -> Result<ProfileData>,
) -> Result<ResolvedProfile> {
    validate_profile_name(overrides.profile_name.as_deref())?;

    let mut profile = normalize(load_profile(paths)?)?;

    if let Some(seed_hex) = overrides.seed_hex.clone() {
        profile.key_source = KeySource::Seed { seed_hex };
    }

    if let Some(hw_key_hex) = overrides.hw_key_hex.clone() {
        let kdf_label = overrides
            .kdf_label
            .clone()
            .ok_or(RkpError::MissingKdfLabel)?;
        profile.key_source = KeySource::HwKey {
            hw_key_hex,
            kdf_label,
        };
    }

    if let Some(server_url) = overrides.server_url.clone() {
        profile.server_url = server_url;
    }

    if let Some(include) = overrides.include_fingerprint {
        profile.fingerprint.include_fingerprint = include;
    }

    if let Some(curve) = overrides.curve {
        profile.curve = curve;
    }

    if let Some(num_keys) = overrides.num_keys {
        profile.num_keys = num_keys.max(1);
    }

    if let Some(output_path) = overrides.output_path.clone() {
        profile.output_path = output_path;
    }

    profile = normalize(profile)?;

    let output_path = paths.resolve_in_root(&profile.output_path);
    Ok(ResolvedProfile {
        profile,
        output_path,
    })
}

fn load_profile(paths: &AppPaths) -> Result<ProfileData> {
    let public = match duck_core::fs::read_optional(&paths.profile_path)? {
        Some(content) => {
            toml::from_str::<PublicProfileDisk>(&content).context("parse profile.toml")?
        }
        None => PublicProfileDisk::from(&ProfileData::default()),
    };

    let secrets = match duck_core::fs::read_optional(&paths.profile_secrets_path)? {
        Some(content) => {
            toml::from_str::<SecretProfileDisk>(&content).context("parse profile.secrets.toml")?
        }
        None => SecretProfileDisk::default(),
    };

    Ok(profile_from_disk(public, &secrets))
}

fn profile_from_disk(public: PublicProfileDisk, secrets: &SecretProfileDisk) -> ProfileData {
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

    ProfileData {
        key_source,
        curve: public.curve,
        device: public.device,
        fingerprint: public.fingerprint,
        server_url: public.server_url,
        num_keys: public.num_keys.max(1),
        output_path: non_blank_output_path(public.output_path),
    }
}

fn non_blank_output_path(value: String) -> String {
    if value.trim().is_empty() {
        default_output_path()
    } else {
        value
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
            curve: value.curve,
            device: value.device.clone(),
            fingerprint: value.fingerprint.clone(),
            server_url: value.server_url.clone(),
            num_keys: value.num_keys.max(1),
            output_path: non_blank_output_path(value.output_path.clone()),
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
