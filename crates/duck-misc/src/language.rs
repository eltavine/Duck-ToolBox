use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_BYTES: u64 = 65536;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Invalid language configuration")]
    InvalidConfig,
    #[error("Language rules exceed the 64 KiB configuration limit")]
    ConfigTooLarge,
    #[error("The saved configuration changed; refresh before saving")]
    StaleRevision,
    #[error("Another language configuration operation is running")]
    Busy,
    #[error("Add at least one application rule before enabling")]
    EmptyRules,
    #[error("Enable the module and reboot to load the language runtime")]
    RuntimeUnavailable,
}

pub(crate) fn code_of(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<Error>() {
        Some(Error::InvalidConfig) => "invalid_config",
        Some(Error::ConfigTooLarge) => "config_too_large",
        Some(Error::StaleRevision) => "stale_revision",
        Some(Error::Busy) => "operation_busy",
        Some(Error::EmptyRules) => "empty_rules",
        Some(Error::RuntimeUnavailable) => "runtime_unavailable",
        None => "state_unavailable",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub package_name: String,
    pub user_id: u32,
    pub locale: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rules: Vec<Rule>,
}

#[derive(Serialize)]
pub struct Status {
    config: Option<Config>,
    config_error: Option<String>,
    revision: String,
    enabled: bool,
    payload_installed: bool,
    parent_active: bool,
    restart_required: bool,
}

fn directory(ctx: &Context) -> PathBuf {
    ctx.paths.var_dir.join("misc/language")
}

fn open_read(path: &std::path::Path) -> Result<Option<fs::File>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    match options.open(path) {
        Ok(file) => {
            ensure!(file.metadata()?.is_file(), Error::InvalidConfig);
            Ok(Some(file))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn raw(ctx: &Context) -> Result<Vec<u8>> {
    let Some(file) = open_read(&directory(ctx).join("rules.json"))? else {
        return Ok(vec![]);
    };
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, Error::InvalidConfig);
    Ok(bytes)
}

fn activation(ctx: &Context) -> Result<Vec<u8>> {
    let Some(file) = open_read(&directory(ctx).join("enabled"))? else {
        return Ok(vec![]);
    };
    let mut bytes = vec![];
    file.take(65).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64, Error::InvalidConfig);
    Ok(bytes)
}

fn revision(bytes: &[u8], activation: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"duck-misc/language/v1\0");
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    hash.update(activation);
    hex::encode(hash.finalize())
}

fn parse(bytes: &[u8]) -> Result<Config> {
    if bytes.is_empty() {
        return Ok(Config::default());
    }
    let mut config: Config = serde_json::from_slice(bytes).map_err(|_| Error::InvalidConfig)?;
    validate(&mut config)?;
    Ok(config)
}

fn valid_package(name: &str) -> bool {
    name.len() <= 255
        && name.contains('.')
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_alphabetic()
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
}

// Accept the language[-Script][-REGION] forms used by Android; reject extensions,
// private-use tags and ambiguous input rather than silently discarding subtags.
fn normalize_locale(tag: &str) -> Option<String> {
    let parts: Vec<_> = tag.split('-').collect();
    let language = *parts.first()?;
    if language.eq_ignore_ascii_case("und")
        || !(2..=3).contains(&language.len())
        || !language.bytes().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let mut result = vec![language.to_ascii_lowercase()];
    let mut index = 1;
    if parts
        .get(index)
        .is_some_and(|v| v.len() == 4 && v.bytes().all(|c| c.is_ascii_alphabetic()))
    {
        let script = parts[index].to_ascii_lowercase();
        result.push(format!(
            "{}{}",
            script[..1].to_ascii_uppercase(),
            &script[1..]
        ));
        index += 1;
    }
    if let Some(region) = parts.get(index) {
        if !((region.len() == 2 && region.bytes().all(|c| c.is_ascii_alphabetic()))
            || (region.len() == 3 && region.bytes().all(|c| c.is_ascii_digit())))
        {
            return None;
        }
        result.push(region.to_ascii_uppercase());
        index += 1;
    }
    (index == parts.len()).then(|| result.join("-"))
}

fn validate(config: &mut Config) -> Result<()> {
    ensure!(config.rules.len() <= 256, Error::InvalidConfig);
    let mut seen = BTreeSet::new();
    for rule in &mut config.rules {
        ensure!(
            valid_package(&rule.package_name) && rule.user_id <= 21473,
            Error::InvalidConfig
        );
        ensure!(
            seen.insert((rule.user_id, rule.package_name.clone())),
            Error::InvalidConfig
        );
        rule.locale = normalize_locale(&rule.locale).ok_or(Error::InvalidConfig)?;
    }
    config
        .rules
        .sort_by(|a, b| (a.user_id, &a.package_name).cmp(&(b.user_id, &b.package_name)));
    ensure!(
        serde_json::to_vec(config)?.len() as u64 <= MAX_BYTES,
        Error::ConfigTooLarge
    );
    Ok(())
}

pub fn status(ctx: &Context) -> Result<Status> {
    let bytes = raw(ctx)?;
    let activation = activation(ctx)?;
    let enabled = active(&activation);
    let config = parse(&bytes);
    Ok(Status {
        config_error: config.as_ref().err().map(|_| "invalid_config".into()),
        config: config.ok(),
        revision: revision(&bytes, &activation),
        enabled,
        payload_installed: payload_installed(ctx),
        parent_active: absent(&ctx.paths.root.join("disable"))
            && absent(&ctx.paths.root.join("remove")),
        restart_required: true,
    })
}

fn active(bytes: &[u8]) -> bool {
    bytes.starts_with(b"1:")
        && bytes.len() > 2
        && bytes[2..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

fn absent(path: &std::path::Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

fn payload_installed(ctx: &Context) -> bool {
    let marker = ctx.paths.root.join("play-integrity/language-spoof.version");
    let Ok(Some(file)) = open_read(&marker) else {
        return false;
    };
    let mut version = vec![];
    file.take(16).read_to_end(&mut version).is_ok()
        && version == b"1\n"
        && ctx.paths.root.join("play-integrity/classes.dex").is_file()
        && ctx.paths.root.join("zygisk/arm64-v8a.so").is_file()
}

fn lock(ctx: &Context) -> Result<fs::File> {
    let directory = directory(ctx);
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let private = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&directory)?;
        private.set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let lock = options.open(directory.join("write.lock"))?;
    ensure!(lock.metadata()?.is_file(), Error::InvalidConfig);
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Err(Error::Busy.into()),
        Err(fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    Ok(lock)
}

fn write(ctx: &Context, name: &str, bytes: &[u8]) -> Result<()> {
    let directory = directory(ctx);
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(directory.join(name))?;
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    revision: String,
    config: Config,
}

fn input<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut bytes = vec![];
    std::io::stdin()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, Error::InvalidConfig);
    serde_json::from_slice(&bytes).map_err(|_| Error::InvalidConfig.into())
}

fn check(ctx: &Context, expected: &str) -> Result<()> {
    ensure!(
        revision(&raw(ctx)?, &activation(ctx)?) == expected,
        Error::StaleRevision
    );
    Ok(())
}

pub fn save(ctx: &Context) -> Result<Status> {
    let request: Save = input()?;
    save_request(ctx, request)
}

fn save_request(ctx: &Context, mut request: Save) -> Result<Status> {
    validate(&mut request.config)?;
    let _lock = lock(ctx)?;
    check(ctx, &request.revision)?;
    write(ctx, "rules.json", &serde_json::to_vec(&request.config)?)?;
    status(ctx)
}

pub fn enable(ctx: &Context) -> Result<Status> {
    let request: Revision = input()?;
    enable_request(ctx, request)
}

fn enable_request(ctx: &Context, request: Revision) -> Result<Status> {
    let _lock = lock(ctx)?;
    check(ctx, &request.revision)?;
    ensure!(!parse(&raw(ctx)?)?.rules.is_empty(), Error::EmptyRules);
    let state = status(ctx)?;
    ensure!(
        state.payload_installed && state.parent_active,
        Error::RuntimeUnavailable
    );
    set_activation(ctx, true)?;
    status(ctx)
}

fn set_activation(ctx: &Context, enabled: bool) -> Result<()> {
    let nonce = tempfile::NamedTempFile::new_in(directory(ctx))?;
    let token = nonce.path().file_name().unwrap().to_string_lossy();
    write(
        ctx,
        "enabled",
        format!("{}:{token}", u8::from(enabled)).as_bytes(),
    )
}

pub fn disable(ctx: &Context) -> Result<serde_json::Value> {
    let _lock = lock(ctx)?;
    set_activation(ctx, false)?;
    Ok(serde_json::json!({"enabled": false, "restart_required": true}))
}

#[cfg(test)]
mod tests;
