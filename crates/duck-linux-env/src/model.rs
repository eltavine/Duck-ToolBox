use crate::fail;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_ENVIRONMENTS: usize = 16;
pub const MAX_ARCHIVE: u64 = 4 << 30;
pub const MAX_EXPANDED: u64 = 16 << 30;
pub const MAX_ENTRIES: u64 = 250_000;
pub const MAX_LOG: u64 = 1 << 20;

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
pub fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn revision(value: &impl Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub id: String,
    pub command: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub auto_start: bool,
    pub dns: Vec<String>,
    pub services: Vec<Service>,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 120
                && !self.name.chars().any(char::is_control),
            fail("invalid_input", "invalid environment name")
        );
        ensure!(
            self.dns.len() <= 4
                && self
                    .dns
                    .iter()
                    .all(|s| s.parse::<std::net::IpAddr>().is_ok()),
            fail("invalid_input", "DNS requires up to four IP addresses")
        );
        ensure!(
            self.services.len() <= 8,
            fail(
                "invalid_input",
                "at most eight foreground services are supported"
            )
        );
        let mut ids = std::collections::HashSet::new();
        for service in &self.services {
            ensure!(
                valid_id(&service.id)
                    && ids.insert(&service.id)
                    && !service.command.trim().is_empty()
                    && service.command.len() <= 8192
                    && !service.command.contains('\0'),
                fail("invalid_input", "invalid or duplicate service")
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub id: String,
    pub instance: String,
    pub archive_sha256: String,
    pub expanded_bytes: u64,
    pub entries: u64,
    pub created_at: u64,
    pub config: Config,
}
impl Environment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.id)
                && valid_digest(&self.instance)
                && valid_digest(&self.archive_sha256)
                && self.expanded_bytes <= MAX_EXPANDED
                && self.entries <= MAX_ENTRIES,
            fail("state_unavailable", "invalid environment metadata")
        );
        self.config.validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub pid: u32,
    pub start: u64,
    pub boot: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub instance: String,
    pub supervisor: Option<Identity>,
    pub init: Option<Identity>,
    pub phase: String,
    pub message: String,
    pub pid_namespace: bool,
    pub config: Config,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub revision: String,
    pub confirmed: bool,
}
impl Target {
    pub fn check(&self, env: &Environment) -> Result<()> {
        ensure!(
            self.confirmed,
            fail("confirmation_required", "confirm the operation")
        );
        ensure!(
            self.id == env.id && self.revision == revision(env)?,
            fail(
                "state_changed",
                "environment configuration changed; refresh"
            )
        );
        Ok(())
    }
}
