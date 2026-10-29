//! Trusted ARM64 Linux environments; platform operations remain feature-local.
mod archive;
mod cli;
mod guest_path;
mod jobs;
mod lifecycle;
mod model;
#[allow(unsafe_code)]
mod mounts;
#[allow(unsafe_code)]
mod native;
mod scope;
#[cfg(any(target_os = "linux", target_os = "android", test))]
mod scripts;
mod store;
mod supervisor;
#[cfg(test)]
mod tests;

pub use cli::FEATURE;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
struct Error {
    code: &'static str,
    message: String,
}
fn fail(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    Error {
        code,
        message: message.into(),
    }
    .into()
}
fn code_of(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<Error>()
        .map_or("linux_env_error", |e| e.code)
}
