use crate::{
    fail, inventory,
    kernel::{self, Operations},
    model::{Config, Device, Stats, Status, valid_device},
    store::{Original, Profile, Store},
};
use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "WireRequest")]
pub struct Request {
    pub device: String,
    pub revision: String,
    pub confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_profile: Option<Profile>,
    #[serde(flatten)]
    pub operation: Operation,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    device: String,
    revision: String,
    confirmed: bool,
    action: String,
    boot_profile: Option<Profile>,
    config: Option<Config>,
}
impl TryFrom<WireRequest> for Request {
    type Error = String;
    fn try_from(v: WireRequest) -> std::result::Result<Self, String> {
        let operation = match (v.action.as_str(), v.config) {
            ("apply", Some(config)) => Operation::Apply { config },
            ("restore", None) => Operation::Restore,
            _ => return Err("apply requires config; restore cannot include config".into()),
        };
        Ok(Self {
            device: v.device,
            revision: v.revision,
            confirmed: v.confirmed,
            boot_profile: v.boot_profile,
            operation,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Operation {
    Apply { config: Config },
    Restore,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResult {
    pub device: String,
    pub action: String,
    pub before: Stats,
    pub after: Stats,
    pub active: bool,
}
impl Request {
    pub fn check(&self, ctx: &Context) -> Result<()> {
        ensure!(
            self.confirmed,
            fail("confirmation_required", "confirm the operation first")
        );
        ensure!(
            valid_device(&self.device)
                && self.revision.len() == 64
                && self.revision.bytes().all(|c| c.is_ascii_hexdigit()),
            fail("invalid_input", "invalid device or revision")
        );
        ensure!(
            crate::capabilities::supported(&ctx.sysroot),
            fail(
                "unsupported_environment",
                "operations require Android ARM64 root outside a test sysroot"
            )
        );
        Ok(())
    }
    pub fn action(&self) -> &str {
        match self.operation {
            Operation::Apply { .. } => "apply",
            Operation::Restore => "restore",
        }
    }
}
#[cfg(test)]
pub fn validate<'a>(
    status: &'a Status,
    device: &str,
    revision: &str,
    config: &Config,
) -> Result<&'a Device> {
    validate_state(status, device, revision, config, false)
}
fn validate_state<'a>(
    status: &'a Status,
    device: &str,
    revision: &str,
    config: &Config,
    restoring: bool,
) -> Result<&'a Device> {
    let device = status
        .devices
        .iter()
        .find(|d| d.name == device)
        .ok_or_else(|| fail("state_changed", "device no longer exists"))?;
    ensure!(
        device.revision == revision,
        fail(
            "state_changed",
            "configuration changed; refresh before applying"
        )
    );
    ensure!(
        device.blocked.is_empty(),
        fail(
            "device_protected",
            format!("device cannot be reset: {}", device.blocked.join(", "))
        )
    );
    if !restoring {
        config.validate(status.memory_total)?;
        ensure!(
            config.memory_limit == device.stats.memory_limit.unwrap_or(0),
            fail(
                "state_changed",
                "memory limit changed; refresh before applying"
            )
        );
    } else {
        ensure!(
            crate::model::valid_algorithm(&config.algorithm)
                && config.priority.is_none_or(|p| p <= 32767),
            fail("state_unavailable", "invalid captured configuration")
        );
    }
    ensure!(
        device.algorithms.contains(&config.algorithm),
        fail("invalid_input", "algorithm is not exposed by this kernel")
    );
    if let Some(swap) = &device.swap
        && !configuration_matches(device, config)
    {
        let available = status
            .memory_available
            .ok_or_else(|| fail("state_unavailable", "MemAvailable unavailable"))?;
        let reserve = (status.memory_total / 10).max(256 << 20);
        ensure!(
            available > swap.used.saturating_add(reserve),
            fail(
                "insufficient_memory",
                "swapoff needs more available memory; close applications and retry"
            )
        );
    }
    Ok(device)
}
pub fn configuration_matches(device: &Device, config: &Config) -> bool {
    device.size == config.size_bytes
        && device.algorithm.as_ref() == Some(&config.algorithm)
        && device.stats.memory_limit == Some(config.memory_limit)
        && device.swap.is_some() == (config.size_bytes > 0)
        && (config.size_bytes == 0
            || device
                .swap
                .as_ref()
                .and_then(|s| u16::try_from(s.priority).ok())
                == config.priority)
}
/// Stop at the first failed stage. A partial transition is never described as rolled back.
pub fn transition(ops: &mut impl Operations, was_active: bool, config: &Config) -> Result<()> {
    if was_active {
        ops.swapoff()?;
    }
    ops.reset()?;
    ops.algorithm(&config.algorithm)?;
    if config.size_bytes > 0 {
        ops.size(config.size_bytes)?;
    }
    if config.memory_limit > 0 {
        ops.limit(config.memory_limit)?;
    }
    if config.size_bytes == 0 {
        return Ok(());
    }
    ops.format()?;
    ops.swapon(config.priority)?;
    Ok(())
}
pub fn perform(ctx: &Context, request: Request) -> Result<OperationResult> {
    request.check(ctx)?;
    let store = Store::open(ctx)?;
    let mut saved = store.load()?;
    let status = inventory::status(&ctx.sysroot)?;
    if let Some(profile) = &request.boot_profile {
        ensure!(
            matches!(&request.operation, Operation::Apply { config } if request.device == profile.device && config.size_bytes == profile.config.size_bytes && config.algorithm == profile.config.algorithm && config.priority == profile.config.priority),
            fail(
                "invalid_input",
                "boot request does not match its authorization"
            )
        );
        saved.claim_boot(profile, &status.boot_id)?;
        store.save(&saved)?;
    }
    let current_device = status
        .devices
        .iter()
        .find(|d| d.name == request.device)
        .ok_or_else(|| fail("state_changed", "device disappeared"))?;
    let config = match &request.operation {
        Operation::Apply { config } => config.clone(),
        Operation::Restore => saved
            .originals
            .iter()
            .find(|v| {
                v.device == request.device
                    && v.boot_id == status.boot_id
                    && v.generation.as_ref() == Some(&current_device.generation)
            })
            .map(|v| v.config.clone())
            .ok_or_else(|| {
                fail(
                    "original_unavailable",
                    "no original configuration captured in this boot",
                )
            })?,
    };
    let device = validate_state(
        &status,
        &request.device,
        &request.revision,
        &config,
        matches!(request.operation, Operation::Restore),
    )?;
    let changing = !configuration_matches(device, &config);
    if changing {
        kernel::preflight(device, &config)?;
    }
    saved.originals.retain(|v| {
        v.boot_id == status.boot_id
            && (v.device != request.device || v.generation.as_ref() == Some(&device.generation))
    });
    if changing && !saved.originals.iter().any(|v| v.device == request.device) {
        saved.originals.push(Original {
            device: request.device.clone(),
            generation: Some(device.generation.clone()),
            boot_id: status.boot_id.clone(),
            config: device.config()?,
        });
        store.save(&saved)?;
    }
    if changing {
        transition(
            &mut kernel::Android {
                device,
                config: &config,
            },
            device.swap.is_some(),
            &config,
        )?;
    }
    let observed = inventory::status(&ctx.sysroot).map_err(|e| {
        fail(
            "operation_unknown",
            format!("cannot verify the completed operation: {e}"),
        )
    })?;
    let after = observed
        .devices
        .iter()
        .find(|v| v.name == request.device)
        .ok_or_else(|| fail("operation_unknown", "device disappeared after operation"))?;
    ensure!(
        after.generation == device.generation && configuration_matches(after, &config),
        fail(
            "operation_unknown",
            "observed configuration differs from the requested state; refresh before proceeding"
        )
    );
    Ok(OperationResult {
        device: request.device.clone(),
        action: request.action().into(),
        before: device.stats.clone(),
        after: after.stats.clone(),
        active: after.swap.is_some(),
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub profile: Profile,
    pub revision: String,
    pub confirmed: bool,
}
pub fn save_profile(ctx: &Context, request: SaveRequest) -> Result<Profile> {
    ensure!(
        request.confirmed,
        fail("confirmation_required", "confirm the saved plan first")
    );
    ensure!(
        crate::capabilities::supported(&ctx.sysroot),
        fail(
            "unsupported_environment",
            "saving requires Android ARM64 root"
        )
    );
    let store = Store::open(ctx)?;
    let mut saved = store.load()?;
    let status = inventory::status(&ctx.sysroot)?;
    // Saving does not swap pages out. Memory headroom is checked at actual application time.
    let d = status
        .devices
        .iter()
        .find(|v| v.name == request.profile.device)
        .ok_or_else(|| fail("state_changed", "device disappeared"))?;
    ensure!(
        d.revision == request.revision && d.blocked.is_empty(),
        fail("state_changed", "device changed or is protected")
    );
    request.profile.config.validate(status.memory_total)?;
    ensure!(
        request.profile.config.memory_limit == d.stats.memory_limit.unwrap_or(0),
        fail(
            "state_changed",
            "memory limit changed; refresh before saving"
        )
    );
    ensure!(
        d.algorithms.contains(&request.profile.config.algorithm),
        fail("invalid_input", "unsupported algorithm")
    );
    saved
        .profiles
        .retain(|p| p.device != request.profile.device);
    saved.profiles.push(request.profile.clone());
    store.save(&saved)?;
    Ok(request.profile)
}
pub fn forget_profile(ctx: &Context, device: &str) -> Result<bool> {
    ensure!(
        valid_device(device),
        fail("invalid_input", "invalid device")
    );
    let store = Store::open(ctx)?;
    let mut saved = store.load()?;
    saved.profiles.retain(|p| p.device != device);
    store.save(&saved)?;
    Ok(true)
}
