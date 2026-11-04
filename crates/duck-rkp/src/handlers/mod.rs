use std::fs;

use anyhow::{Context, Result, anyhow};
use duck_core::AppPaths;
use serde_json::{Value, json};

use crate::{
    cli::{SharedRunArgs, VerifyArgs},
    error::RkpError,
    profile::{
        ProfileData, ResolvedProfile, RunOverrides, resolve_diagnostic_profile, resolve_profile,
    },
    validate::validate_device_info_for_request,
    verify::verify_csr,
};

pub(crate) mod import;
pub(crate) mod info;
pub(crate) mod keybox;
pub(crate) mod profile;
pub(crate) mod provision;

pub(crate) fn verify(paths: &AppPaths, args: &VerifyArgs) -> Result<Value> {
    let file = paths.resolve_in_root(&args.csr_file);
    let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
    let report = verify_csr(&bytes)?;

    Ok(json!({
        "path": file.display().to_string(),
        "report": report,
    }))
}

fn resolve_runtime(
    paths: &AppPaths,
    shared: &SharedRunArgs,
    num_keys: Option<u32>,
    output_path: Option<String>,
) -> Result<ResolvedProfile> {
    resolve_profile(paths, &runtime_overrides(shared, num_keys, output_path))
}

fn resolve_runtime_offline(
    paths: &AppPaths,
    shared: &SharedRunArgs,
    num_keys: Option<u32>,
) -> Result<ResolvedProfile> {
    resolve_diagnostic_profile(paths, &runtime_overrides(shared, num_keys, None))
}

fn runtime_overrides(
    shared: &SharedRunArgs,
    num_keys: Option<u32>,
    output_path: Option<String>,
) -> RunOverrides {
    RunOverrides {
        profile_name: shared.profile.clone(),
        seed_hex: shared.seed.clone(),
        hw_key_hex: shared.hw_key.clone(),
        kdf_label: shared.kdf_label.clone(),
        curve: shared.curve,
        server_url: shared.server_url.clone(),
        include_fingerprint: shared.include_fingerprint,
        num_keys,
        output_path,
    }
}

/// Fails fast, before any network traffic, when the profile cannot produce a valid request.
fn ensure_request_context(profile: &ProfileData) -> Result<String> {
    if profile.fingerprint.value.trim().is_empty() {
        return Err(RkpError::MissingFingerprint.into());
    }

    validate_device_info_for_request(&profile.device)?;
    Ok(profile.server_url.trim().to_owned())
}

fn random_bytes(len: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![0_u8; len];
    getrandom::fill(&mut bytes).map_err(|error| anyhow!("fill random bytes: {error}"))?;
    Ok(bytes)
}
