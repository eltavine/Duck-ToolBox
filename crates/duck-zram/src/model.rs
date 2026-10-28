use crate::fail;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_DEVICES: usize = 32;
pub fn valid_device(name: &str) -> bool {
    name.strip_prefix("zram").is_some_and(|n| {
        !n.is_empty()
            && n.len() <= 6
            && n.bytes().all(|c| c.is_ascii_digit())
            && (n == "0" || !n.starts_with('0'))
    })
}
pub fn valid_algorithm(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Zero means deactivate and reset. Positive sizes are bytes.
    pub size_bytes: u64,
    pub algorithm: String,
    /// None selects the kernel's automatic (negative) priority.
    pub priority: Option<u16>,
    pub memory_limit: u64,
}
impl Config {
    pub fn validate(&self, total_memory: u64) -> Result<()> {
        ensure!(
            valid_algorithm(&self.algorithm),
            fail("invalid_input", "invalid compression algorithm")
        );
        ensure!(
            self.priority.is_none_or(|n| n <= 32767),
            fail("invalid_input", "priority must be automatic or 0..32767")
        );
        let bytes = self.size_bytes;
        ensure!(
            self.size_bytes == 0 || (bytes >= 16 << 20 && bytes <= total_memory.saturating_mul(2)),
            fail(
                "invalid_input",
                "size must be 16 MiB to twice physical memory, or zero to disable"
            )
        );
        ensure!(
            self.memory_limit <= total_memory,
            fail("invalid_input", "memory limit exceeds physical memory")
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    pub original: Option<u64>,
    pub compressed: Option<u64>,
    pub memory_used: Option<u64>,
    pub memory_limit: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Swap {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub used: u64,
    pub priority: i32,
    #[serde(skip)]
    pub identity: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub name: String,
    pub identity: String,
    pub path: Option<String>,
    pub size: u64,
    pub initialized: bool,
    pub algorithms: Vec<String>,
    pub algorithm: Option<String>,
    pub swap: Option<Swap>,
    pub stats: Stats,
    pub blocked: Vec<String>,
    pub revision: String,
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub operations_supported: bool,
    pub mmd_managed: Option<bool>,
    pub memory_total: u64,
    pub memory_available: Option<u64>,
    pub swap_in_pages: Option<u64>,
    pub swap_out_pages: Option<u64>,
    pub boot_id: String,
    pub devices: Vec<Device>,
    pub swaps: Vec<Swap>,
    pub warnings: Vec<String>,
}
