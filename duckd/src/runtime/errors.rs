use anyhow::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unsupported profile name `{0}`; only `default` is available")]
    UnsupportedProfile(String),
    #[error("key material is not configured; save a profile or pass `--seed` / `--hw-key`")]
    MissingKeySource,
    #[error("build fingerprint is required before provisioning")]
    MissingFingerprint,
    #[error("RKP server URL is required")]
    MissingServerUrl,
    #[error("invalid RKP server URL `{0}`")]
    InvalidServerUrl(String),
    #[error("output path `{0}` must stay relative to Duck ToolBox and include a file name")]
    InvalidOutputPath(String),
    #[error("`--kdf-label` is required when using `--hw-key`")]
    MissingKdfLabel,
    #[error("seed must be exactly 32 bytes, got {0}")]
    InvalidSeedLength(usize),
    #[error("hardware key must be exactly 16 bytes, got {0}")]
    InvalidHardwareKeyLength(usize),
    #[error("device field `{0}` is required before provisioning")]
    MissingDeviceField(&'static str),
    #[error("device field `{field}` is invalid: {reason}")]
    InvalidDeviceField { field: &'static str, reason: String },
    #[error("RKP server returned unsupported EEK curve `{0}`")]
    UnsupportedEekCurve(i128),
    #[error("invalid RKP server response: {0}")]
    InvalidRkpResponse(String),
    #[error("device not registered: {0}")]
    DeviceNotRegistered(String),
    #[error("RKP client error: {0}")]
    RkpClient(String),
    #[error("RKP server error: {0}")]
    RkpServer(String),
}

pub fn error_code(error: &Error) -> &'static str {
    if let Some(app_error) = error.downcast_ref::<AppError>() {
        return match app_error {
            AppError::UnsupportedProfile(_) => "unsupported_profile",
            AppError::MissingKeySource => "missing_key_source",
            AppError::MissingFingerprint => "missing_fingerprint",
            AppError::MissingServerUrl => "missing_server_url",
            AppError::InvalidServerUrl(_) => "invalid_server_url",
            AppError::InvalidOutputPath(_) => "invalid_output_path",
            AppError::MissingKdfLabel => "missing_kdf_label",
            AppError::InvalidSeedLength(_) => "invalid_seed_length",
            AppError::InvalidHardwareKeyLength(_) => "invalid_hardware_key_length",
            AppError::MissingDeviceField(_) => "missing_device_field",
            AppError::InvalidDeviceField { .. } => "invalid_device_field",
            AppError::UnsupportedEekCurve(_) => "unsupported_eek_curve",
            AppError::InvalidRkpResponse(_) => "invalid_rkp_response",
            AppError::DeviceNotRegistered(_) => "device_not_registered",
            AppError::RkpClient(_) => "rkp_client_error",
            AppError::RkpServer(_) => "rkp_server_error",
        };
    }

    "internal_error"
}
