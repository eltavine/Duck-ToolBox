//! Existing ZRAM swap devices, with explicit runtime and boot configuration.
mod cli;
mod inventory;
#[allow(unsafe_code)]
mod kernel;
mod model;
mod mutation;
mod store;
mod worker;

pub use cli::{Command, FEATURE};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
struct ZramError {
    code: &'static str,
    message: String,
}
fn fail(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    ZramError {
        code,
        message: message.into(),
    }
    .into()
}
fn code_of(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<ZramError>()
        .map_or("zram_error", |e| e.code)
}
#[cfg(test)]
mod tests;
