use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum MaterialError {
    #[error("{0}")]
    Input(&'static str),
    #[error("{0}")]
    Keybox(&'static str),
    #[error("{0}")]
    Identity(&'static str),
    #[error("identity does not match the CSR; nothing was exported")]
    Mismatch,
    #[error("{0}")]
    Output(&'static str),
}

pub(crate) fn code(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<MaterialError>() {
        Some(MaterialError::Input(_)) => "invalid_input",
        Some(MaterialError::Keybox(_)) => "invalid_keybox",
        Some(MaterialError::Identity(_)) => "invalid_identity",
        Some(MaterialError::Mismatch) => "identity_mismatch",
        Some(MaterialError::Output(_)) => "output_error",
        None => "internal_error",
    }
}
