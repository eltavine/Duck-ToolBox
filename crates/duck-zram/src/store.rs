use crate::{
    fail,
    model::{Config, MAX_DEVICES, valid_device},
};
use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub device: String,
    pub config: Config,
    pub apply_at_boot: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Original {
    pub device: String,
    pub boot_id: String,
    pub config: Config,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootResult {
    pub device: String,
    pub boot_id: String,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub originals: Vec<Original>,
    pub boot_results: Vec<BootResult>,
    pub attempted_boot: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
            originals: vec![],
            boot_results: vec![],
            attempted_boot: None,
        }
    }
}

pub struct Store {
    directory: PathBuf,
    _lock: fs::File,
}
impl Store {
    pub fn open(ctx: &Context) -> Result<Self> {
        let directory = ctx.paths.var_dir.join("zram");
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(&directory)?;
            file.set_permissions(fs::Permissions::from_mode(0o700))?;
        }
        let mut opts = fs::OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = opts.open(directory.join("mutation.lock"))?;
        ensure!(
            lock.metadata()?.is_file(),
            fail("state_unavailable", "lock is not a regular file")
        );
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => {
                return Err(fail("operation_busy", "another ZRAM operation is running"));
            }
            Err(fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        Ok(Self {
            directory,
            _lock: lock,
        })
    }
    pub fn load(&self) -> Result<State> {
        let mut opts = fs::OpenOptions::new();
        opts.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = match opts.open(self.directory.join("state.json")) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(e.into()),
        };
        ensure!(
            file.metadata()?.is_file(),
            fail("state_unavailable", "state is not a regular file")
        );
        let mut bytes = Vec::new();
        file.take(131_073).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 131_072,
            fail("state_unavailable", "saved state exceeds limit")
        );
        let state: State = serde_json::from_slice(&bytes)?;
        ensure!(
            state.version == 1
                && state.profiles.len() <= MAX_DEVICES
                && state.originals.len() <= MAX_DEVICES
                && state.boot_results.len() <= MAX_DEVICES,
            fail("state_unavailable", "invalid saved state version or size")
        );
        let mut names = std::collections::HashSet::new();
        for p in &state.profiles {
            ensure!(
                valid_device(&p.device) && names.insert(&p.device),
                fail("state_unavailable", "invalid or repeated profile device")
            );
        }
        Ok(state)
    }
    pub fn save(&self, state: &State) -> Result<()> {
        ensure!(
            state.profiles.len() <= MAX_DEVICES
                && state.originals.len() <= MAX_DEVICES
                && state.boot_results.len() <= MAX_DEVICES,
            fail("state_unavailable", "saved entry limit reached")
        );
        let bytes = serde_json::to_vec_pretty(state)?;
        ensure!(
            bytes.len() <= 131_072,
            fail("state_unavailable", "saved state exceeds limit")
        );
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(self.directory.join("state.json"))?;
        fs::File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
