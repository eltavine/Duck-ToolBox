use crate::{
    fail,
    model::{Environment, MAX_ENVIRONMENTS, valid_id},
};
use anyhow::{Context as _, Result, ensure};
use duck_core::Context;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn base(ctx: &Context) -> PathBuf {
    ctx.paths.var_dir.join("linux-env")
}
pub fn private_dir(path: &Path) -> Result<()> {
    if !path.try_exists()? {
        fs::create_dir_all(path)?;
    }
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        fail(
            "state_unavailable",
            "private directory is not a real directory"
        )
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn open(path: &Path, write: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    if write {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .mode(0o600);
    }
    let file = options
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        fail("state_unavailable", "expected a regular file")
    );
    Ok(file)
}
pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut text = Vec::new();
    open(path, false)?.take(524_289).read_to_end(&mut text)?;
    ensure!(
        text.len() <= 524_288,
        fail("state_unavailable", "state file exceeds limit")
    );
    serde_json::from_slice(&text).map_err(|e| fail("state_unavailable", e.to_string()))
}
pub fn save(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| fail("state_unavailable", "missing state parent"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub struct Store {
    pub base: PathBuf,
    pub lock: File,
}
impl Store {
    pub fn lock(ctx: &Context) -> Result<Self> {
        let base = base(ctx);
        private_dir(&base)?;
        let lock = open(&base.join("operation.lock"), true)?;
        lock.try_lock()
            .map_err(|_| fail("operation_busy", "another environment operation is running"))?;
        Ok(Self { base, lock })
    }
    pub fn env_dir(&self, id: &str) -> Result<PathBuf> {
        env_dir(&self.base, id)
    }
    pub fn environment(&self, id: &str) -> Result<Environment> {
        environment(&self.base, id)
    }
    pub fn save_environment(&self, env: &Environment) -> Result<()> {
        env.validate()?;
        save(&self.env_dir(&env.id)?.join("environment.json"), env)
    }
}
pub fn env_dir(base: &Path, id: &str) -> Result<PathBuf> {
    ensure!(
        valid_id(id),
        fail("invalid_input", "invalid environment ID")
    );
    Ok(base.join(id))
}
pub fn environment(base: &Path, id: &str) -> Result<Environment> {
    let env: Environment = load(&env_dir(base, id)?.join("environment.json"))?;
    env.validate()?;
    ensure!(
        env.id == id,
        fail("state_unavailable", "environment ID differs from directory")
    );
    Ok(env)
}
pub fn list(base: &Path) -> Result<Vec<Environment>> {
    if !base.try_exists()? {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&id) || !entry.file_type()?.is_dir() {
            continue;
        }
        result.push(environment(base, &id)?);
        ensure!(
            result.len() <= MAX_ENVIRONMENTS,
            fail("state_unavailable", "too many environments")
        );
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
pub fn busy(base: &Path) -> Result<bool> {
    locked(&base.join("operation.lock"))
}
pub fn locked(path: &Path) -> Result<bool> {
    if !path.try_exists()? {
        return Ok(false);
    }
    let file = open(path, false)?;
    Ok(file.try_lock().is_err())
}
