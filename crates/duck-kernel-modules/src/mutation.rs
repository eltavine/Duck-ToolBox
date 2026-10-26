use std::{fs, path::Path};

use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};

use crate::{ModuleError, files, inventory, kernel};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadRequest {
    pub path: String,
    pub sha256: String,
    #[serde(default)]
    pub parameters: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnloadRequest {
    pub name: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", content = "request", rename_all = "snake_case")]
pub enum Request {
    Load(LoadRequest),
    Unload(UnloadRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Load,
    Unload,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResult {
    pub name: String,
    pub action: Action,
    pub observed_loaded: Option<bool>,
}

impl Request {
    pub fn action(&self) -> Action {
        match self {
            Self::Load(_) => Action::Load,
            Self::Unload(_) => Action::Unload,
        }
    }
    pub fn check(&self, ctx: &Context) -> Result<()> {
        let confirmed = match self {
            Self::Load(v) => v.confirmed,
            Self::Unload(v) => v.confirmed,
        };
        ensure!(confirmed, ModuleError::ConfirmationRequired);
        ensure!(kernel::supported(&ctx.sysroot), ModuleError::Unsupported);
        Ok(())
    }
}

pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        // Work on the opened inode so chmod cannot follow a substituted final symlink.
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)?;
        directory.set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn open_lock(directory: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let lock = options.open(directory.join("mutation.lock"))?;
    ensure!(
        lock.metadata()?.is_file(),
        "operation lock must be a regular file"
    );
    Ok(lock)
}

pub fn validate_load(ctx: &Context, request: &LoadRequest) -> Result<(files::FileInfo, Vec<u8>)> {
    ensure!(
        request.parameters.len() <= 4096 && !request.parameters.chars().any(char::is_control),
        ModuleError::InvalidInput(
            "parameters must be at most 4096 bytes without control characters".into()
        )
    );
    let (info, bytes) = files::inspect(&ctx.sysroot, &request.path)?;
    ensure!(info.sha256 == request.sha256, ModuleError::FileChanged);
    validate_kernel_name(&info.name)?;
    ensure!(
        info.architecture == files::Architecture::Aarch64
            && info.elf_class == 64
            && info.little_endian,
        ModuleError::InvalidInput("loading requires a little-endian ELF64 arm64 module".into())
    );
    // With symbol CRCs, the kernel's same_magic() can ignore the release prefix.
    // Keep the release comparison informational; never set an ignore-vermagic flag.
    ensure!(
        info.metadata
            .get("vermagic")
            .is_some_and(|values| values.len() == 1 && !values[0].trim().is_empty()),
        ModuleError::InvalidInput("module must contain one nonempty vermagic entry".into())
    );
    let status = inventory::status(&ctx.sysroot)?;
    ensure!(
        status.proc_available,
        ModuleError::Unavailable("cannot read /proc/modules".into())
    );
    ensure!(
        !status.modules.iter().any(|v| v.name == info.name),
        ModuleError::InvalidInput("module is already visible in the kernel".into())
    );
    Ok((info, bytes))
}

pub fn validate_unload(ctx: &Context, name: &str) -> Result<()> {
    validate_kernel_name(name)?;
    let details = inventory::details(&ctx.sysroot, name)?;
    ensure!(
        details.module.source == "loaded",
        ModuleError::Busy("not present in /proc/modules; it may be built in".into())
    );
    ensure!(
        details.module.state.as_deref() == Some("Live"),
        ModuleError::Busy("module is not in Live state".into())
    );
    ensure!(
        details.module.references == Some(0)
            && details.module.used_by.is_empty()
            && details.holders.is_empty(),
        ModuleError::Busy("module has references or dependent modules".into())
    );
    Ok(())
}

fn validate_kernel_name(name: &str) -> Result<()> {
    ensure!(
        inventory::valid_name(name) && name.len() <= kernel::MAX_NAME_BYTES,
        ModuleError::InvalidInput("ARM64 module names must fit in 55 bytes".into())
    );
    Ok(())
}

pub fn perform(ctx: &Context, request: Request) -> Result<OperationResult> {
    request.check(ctx)?;
    let directory = ctx.paths.var_dir.join("kernel-modules");
    private_dir(&directory)?;
    let lock = open_lock(&directory)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Err(ModuleError::OperationBusy.into()),
        Err(fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    ensure!(
        inventory::status(&ctx.sysroot)?.modules_disabled != Some(true),
        ModuleError::Disabled
    );
    let (name, action) = match request {
        Request::Load(request) => {
            let (info, bytes) = validate_load(ctx, &request)?;
            kernel::load(&bytes, &request.parameters)?;
            (info.name, Action::Load)
        }
        Request::Unload(request) => {
            validate_unload(ctx, &request.name)?;
            kernel::unload(&request.name)?;
            (request.name, Action::Unload)
        }
    };
    let observed_loaded = inventory::status(&ctx.sysroot)
        .ok()
        .filter(|v| v.proc_available)
        .map(|v| {
            v.modules
                .iter()
                .any(|m| m.source == "loaded" && m.name == name)
        });
    Ok(OperationResult {
        name,
        action,
        observed_loaded,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::{open_lock, private_dir};
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn private_directory_and_lock_reject_final_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::set_permissions(&external, fs::Permissions::from_mode(0o755)).unwrap();
        let directory = temp.path().join("private");
        symlink(&external, &directory).unwrap();
        assert!(private_dir(&directory).is_err());
        assert_eq!(
            fs::metadata(&external).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::remove_file(&directory).unwrap();
        private_dir(&directory).unwrap();
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let target = external.join("target");
        fs::write(&target, "unchanged").unwrap();
        symlink(&target, directory.join("mutation.lock")).unwrap();
        assert!(open_lock(&directory).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        fs::remove_file(directory.join("mutation.lock")).unwrap();
        assert!(open_lock(&directory).unwrap().try_lock().is_ok());
    }
}
