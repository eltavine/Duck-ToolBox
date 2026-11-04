use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const DEFAULT_SERVER_URL: &str = "https://remoteprovisioning.googleapis.com/v1";
pub const DEFAULT_OUTPUT_PATH: &str = "var/outputs/keybox.xml";
pub const DEFAULT_DICE_ISSUER: &str = "Android";
pub const DEFAULT_DICE_SUBJECT: &str = "KeyMint";
pub const DEFAULT_GENERIC_BRAND: &str = "generic";
pub const DEFAULT_GENERIC_MODEL: &str = "default";
pub const DEFAULT_GENERIC_DEVICE: &str = "default";
pub const DEFAULT_GENERIC_PRODUCT: &str = "default";
pub const DEFAULT_GENERIC_MANUFACTURER: &str = "generic";
pub const DEFAULT_GENERIC_VB_STATE: &str = "green";
pub const DEFAULT_GENERIC_OS_VERSION: &str = "13";
pub const DEFAULT_GENERIC_SECURITY_LEVEL: &str = "tee";
pub const DEFAULT_GENERIC_BOOTLOADER_STATE: &str = "locked";
pub const DEFAULT_GENERIC_BOOT_PATCH_LEVEL: u32 = 20250101;
pub const DEFAULT_GENERIC_SYSTEM_PATCH_LEVEL: u32 = 202501;
pub const DEFAULT_GENERIC_VENDOR_PATCH_LEVEL: u32 = 20250101;
pub const DEFAULT_GENERIC_FINGERPRINT: &str =
    "generic/default/default:13/TP1A.220624.014/0:user/release-keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DiceCurve {
    #[default]
    Ed25519,
    P256,
}

impl DiceCurve {
    pub fn as_str(self) -> &'static str {
        match self {
            DiceCurve::Ed25519 => "ed25519",
            DiceCurve::P256 => "p256",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum KeySource {
    #[default]
    Unset,
    Seed {
        seed_hex: String,
    },
    HwKey {
        hw_key_hex: String,
        kdf_label: String,
    },
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
            brand: DEFAULT_GENERIC_BRAND.into(),
            model: DEFAULT_GENERIC_MODEL.into(),
            device: DEFAULT_GENERIC_DEVICE.into(),
            product: DEFAULT_GENERIC_PRODUCT.into(),
            manufacturer: DEFAULT_GENERIC_MANUFACTURER.into(),
            fused: 1,
            vb_state: DEFAULT_GENERIC_VB_STATE.into(),
            os_version: DEFAULT_GENERIC_OS_VERSION.into(),
            security_level: DEFAULT_GENERIC_SECURITY_LEVEL.into(),
            bootloader_state: DEFAULT_GENERIC_BOOTLOADER_STATE.into(),
            boot_patch_level: DEFAULT_GENERIC_BOOT_PATCH_LEVEL,
            system_patch_level: DEFAULT_GENERIC_SYSTEM_PATCH_LEVEL,
            vendor_patch_level: DEFAULT_GENERIC_VENDOR_PATCH_LEVEL,
            vbmeta_digest: None,
            dice_issuer: DEFAULT_DICE_ISSUER.into(),
            dice_subject: DEFAULT_DICE_SUBJECT.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintConfig {
    pub value: String,
    #[serde(default)]
    pub include_fingerprint: bool,
}

impl Default for FingerprintConfig {
    fn default() -> Self {
        Self {
            value: DEFAULT_GENERIC_FINGERPRINT.into(),
            include_fingerprint: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    #[serde(default)]
    pub key_source: KeySource,
    #[serde(default)]
    pub curve: DiceCurve,
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
            curve: DiceCurve::default(),
            device: DeviceInfo::default(),
            fingerprint: FingerprintConfig::default(),
            server_url: default_server_url(),
            num_keys: default_num_keys(),
            output_path: default_output_path(),
        }
    }
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
