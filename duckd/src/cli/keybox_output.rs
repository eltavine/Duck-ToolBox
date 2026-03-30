use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use duckd::{
    features::rkp::keybox_xml::CertificateChainSummary,
    runtime::{
        paths::AppPaths,
        profile::{DEFAULT_OUTPUT_PATH, ResolvedProfile},
    },
};

#[derive(Debug, Serialize)]
pub struct KeyboxData {
    pub mode: String,
    pub cdi_leaf_pubkey_hex: String,
    pub challenge_hex: String,
    pub csr_path: String,
    pub keybox_path: String,
    pub keybox_xml: String,
    pub device_id: String,
    pub chain_summary: CertificateChainSummary,
}

pub fn resolve_keybox_output_path(
    paths: &AppPaths,
    resolved: &ResolvedProfile,
    explicit_output: bool,
) -> Result<PathBuf> {
    if explicit_output || !uses_default_keybox_output(&resolved.profile.output_path) {
        return Ok(resolved.output_path.clone());
    }

    for attempt in 0..32_u32 {
        let suffix = unique_suffix(attempt)?;
        let candidate = paths.outputs_dir.join(format!("keybox_{suffix}.xml"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!(
        "failed to resolve a unique keybox output path under {}",
        paths.outputs_dir.display()
    )
}

fn uses_default_keybox_output(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.is_empty() || trimmed == DEFAULT_OUTPUT_PATH
}

fn unique_suffix(attempt: u32) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis();

    if attempt == 0 {
        return Ok(timestamp.to_string());
    }

    Ok(format!("{timestamp}_{attempt}"))
}
