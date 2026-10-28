use crate::{
    fail,
    model::{Config, MAX_DEVICES, valid_algorithm, valid_boot_id, valid_device},
};
use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub generation: Option<String>,
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
    #[serde(default)]
    pub boot_claimed: Vec<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
            originals: vec![],
            boot_results: vec![],
            attempted_boot: None,
            boot_claimed: vec![],
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
        Self::read_state(&self.directory)
    }
    pub fn snapshot(ctx: &Context) -> Result<(State, bool)> {
        let directory = ctx.paths.var_dir.join("zram");
        match fs::symlink_metadata(&directory) {
            Ok(m) => ensure!(
                m.is_dir(),
                fail(
                    "state_unavailable",
                    "private state directory is not a directory"
                )
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((State::default(), false));
            }
            Err(e) => return Err(e.into()),
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = match options.open(directory.join("mutation.lock")) {
            Ok(lock) => Some(lock),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let busy = if let Some(lock) = &lock {
            ensure!(
                lock.metadata()?.is_file(),
                fail("state_unavailable", "lock is not a regular file")
            );
            match lock.try_lock() {
                Ok(()) => false,
                Err(fs::TryLockError::WouldBlock) => true,
                Err(fs::TryLockError::Error(e)) => return Err(e.into()),
            }
        } else {
            false
        };
        Ok((Self::read_state(&directory)?, busy))
    }
    fn read_state(directory: &std::path::Path) -> Result<State> {
        let mut opts = fs::OpenOptions::new();
        opts.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = match opts.open(directory.join("state.json")) {
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
        state.validate()?;
        Ok(state)
    }
    pub fn save(&self, state: &State) -> Result<()> {
        state.validate()?;
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

impl State {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.profiles.len() <= MAX_DEVICES
                && self.originals.len() <= MAX_DEVICES
                && self.boot_results.len() <= MAX_DEVICES
                && self.boot_claimed.len() <= MAX_DEVICES,
            fail("state_unavailable", "invalid saved state version or size")
        );
        let unique = |names: Vec<&str>| {
            let mut set = std::collections::HashSet::new();
            names.into_iter().all(|n| valid_device(n) && set.insert(n))
        };
        ensure!(
            unique(self.profiles.iter().map(|v| v.device.as_str()).collect())
                && unique(self.originals.iter().map(|v| v.device.as_str()).collect())
                && unique(
                    self.boot_results
                        .iter()
                        .map(|v| v.device.as_str())
                        .collect()
                )
                && unique(self.boot_claimed.iter().map(String::as_str).collect()),
            fail("state_unavailable", "invalid or repeated saved device")
        );
        ensure!(
            self.attempted_boot
                .as_ref()
                .is_none_or(|id| valid_boot_id(id)),
            fail("state_unavailable", "invalid saved boot ID")
        );
        for config in self
            .profiles
            .iter()
            .map(|p| &p.config)
            .chain(self.originals.iter().map(|p| &p.config))
        {
            ensure!(
                valid_algorithm(&config.algorithm) && config.priority.is_none_or(|p| p <= 32767),
                fail("state_unavailable", "invalid saved configuration")
            );
        }
        ensure!(
            self.originals.iter().all(|v| valid_boot_id(&v.boot_id)
                && v.generation
                    .as_ref()
                    .is_none_or(|g| g.len() == 64 && g.bytes().all(|c| c.is_ascii_hexdigit())))
                && self.boot_results.iter().all(|v| valid_boot_id(&v.boot_id)
                    && !v.code.is_empty()
                    && v.code.len() <= 64
                    && v.message.len() <= 8192),
            fail("state_unavailable", "invalid saved original or boot result")
        );
        Ok(())
    }
    pub fn claim_boot(&mut self, profile: &Profile, boot_id: &str) -> Result<()> {
        ensure!(
            profile.apply_at_boot
                && self.attempted_boot.as_deref() == Some(boot_id)
                && self.profiles.contains(profile)
                && self
                    .boot_results
                    .iter()
                    .any(|r| r.device == profile.device && r.boot_id == boot_id)
                && !self.boot_claimed.contains(&profile.device),
            fail(
                "state_changed",
                "saved boot authorization changed or was already used"
            )
        );
        self.boot_claimed.push(profile.device.clone());
        Ok(())
    }
}
