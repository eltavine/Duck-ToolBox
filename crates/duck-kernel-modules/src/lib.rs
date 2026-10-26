//! Linux kernel modules, independently of root-manager module packages.

mod cli;
mod files;
mod inventory;
#[allow(unsafe_code)]
mod kernel;
mod mutation;
mod worker;

pub use cli::{Command, FEATURE};

use thiserror::Error;

#[derive(Debug, Error)]
enum ModuleError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("kernel state is unavailable: {0}")]
    Unavailable(String),
    #[error("this operation requires an Android ARM64 root process outside a test sysroot")]
    Unsupported,
    #[error("explicit confirmation is required")]
    ConfirmationRequired,
    #[error("module file changed after inspection; inspect it again")]
    FileChanged,
    #[error("the kernel has disabled module loading and unloading")]
    Disabled,
    #[error("module cannot be unloaded: {0}")]
    Busy(String),
    #[error("a kernel module operation is already running")]
    OperationBusy,
    #[error(
        "kernel module operation timed out; its outcome is unknown. Refresh the inventory before taking further action"
    )]
    Timeout,
    #[error(
        "worker result unavailable ({0}); operation outcome is unknown. Refresh the inventory before taking further action"
    )]
    OutcomeUnknown(String),
    #[cfg_attr(
        not(all(target_os = "android", target_arch = "aarch64")),
        allow(dead_code)
    )]
    #[error("kernel rejected the operation: {0}")]
    KernelRejected(String),
    #[error("{message}")]
    WorkerFailure { code: String, message: String },
}

fn code_of(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<ModuleError>() {
        Some(ModuleError::InvalidInput(_)) => "invalid_input",
        Some(ModuleError::Unavailable(_)) => "state_unavailable",
        Some(ModuleError::Unsupported) => "unsupported_environment",
        Some(ModuleError::ConfirmationRequired) => "confirmation_required",
        Some(ModuleError::FileChanged) => "file_changed",
        Some(ModuleError::Disabled) => "modules_disabled",
        Some(ModuleError::Busy(_)) => "module_busy",
        Some(ModuleError::OperationBusy) => "operation_busy",
        Some(ModuleError::Timeout) => "operation_timeout",
        Some(ModuleError::OutcomeUnknown(_)) => "operation_unknown",
        Some(ModuleError::KernelRejected(_)) => "kernel_rejected",
        Some(ModuleError::WorkerFailure { code, .. }) => match code.as_str() {
            "invalid_input" => "invalid_input",
            "state_unavailable" => "state_unavailable",
            "unsupported_environment" => "unsupported_environment",
            "confirmation_required" => "confirmation_required",
            "file_changed" => "file_changed",
            "modules_disabled" => "modules_disabled",
            "module_busy" => "module_busy",
            "operation_busy" => "operation_busy",
            "kernel_rejected" => "kernel_rejected",
            _ => "kernel_modules_error",
        },
        None => "kernel_modules_error",
    }
}

#[cfg(test)]
mod tests;
