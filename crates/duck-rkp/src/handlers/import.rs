use anyhow::{Context, Result, ensure};
use duck_core::AppPaths;
use serde_json::{Value, json};

use crate::{
    artifacts::{private_run, save_issued},
    cli::ImportArgs,
    cose::generate_ec_keypair,
    material,
    private_output::write_new_verified,
    validate::validate_device_info_for_request,
    workflow::{issue, validate_request_binding},
};

pub(crate) enum Operation {
    Validate,
    Diagnose,
    Issue,
}

pub(crate) async fn run(
    paths: &AppPaths,
    args: &ImportArgs,
    operation: Operation,
) -> Result<Value> {
    let bundle = args.bundle.as_deref().map(|p| paths.resolve_in_root(p));
    let chain = args.chain.as_deref().map(|p| paths.resolve_in_root(p));
    let signing = args.signing.as_deref().map(|p| paths.resolve_in_root(p));
    let config = args.config.as_deref().map(|p| paths.resolve_in_root(p));
    let material = material::load(
        bundle.as_deref(),
        chain.as_deref(),
        signing.as_deref(),
        config.as_deref(),
    )?;
    if matches!(operation, Operation::Validate) {
        let ready = material.device().is_some() && material.issuance().is_some();
        if ready {
            // the largest challenge and fixed-size P-256 key bound online request size
            let public = [generate_ec_keypair()?.cose_public];
            build_request(&material, args.include_fingerprint, &[0; 64], &public)
                .context("imported material cannot prepare a CSR for online issuance")
                .context(material::MaterialError::Configuration)?;
        }
        return Ok(json!({
            "stage": "validated", "material": material.report(),
            "ready_for_issuance": ready,
        }));
    }
    let device = material
        .device()
        .context("explicit device information is required")?;
    validate_device_info_for_request(device)?;
    if matches!(operation, Operation::Diagnose) {
        let mut challenge = [0u8; 32];
        getrandom::fill(&mut challenge)
            .map_err(|_| anyhow::anyhow!("diagnostic challenge unavailable"))?;
        let key = generate_ec_keypair()?;
        let public = [key.cose_public];
        let csr = build_request(&material, args.include_fingerprint, &challenge, &public)?;
        let report = validate_request_binding(&csr, &challenge, &public)?;
        let directory = private_run(paths, "rkp-diagnostic-")?;
        let path = directory.path().join("diagnostic-only.cbor");
        write_new_verified(&path, &csr, |bytes| {
            validate_request_binding(bytes, &challenge, &public).map(|_| ())
        })?;
        let _ = directory.keep();
        return Ok(json!({"stage":"diagnostic", "material":material.report(),
            "csr_path":path.display().to_string(), "local_verify":report, "diagnostic_only":true}));
    }
    let config = material
        .issuance()
        .context("explicit issuance configuration is required")?;
    ensure!(
        config.protocol == "v3",
        "unsupported request protocol; only v3 is implemented for material import"
    );
    let issued = issue(
        &config.fingerprint,
        args.include_fingerprint
            .unwrap_or(config.include_fingerprint),
        &config.server_url,
        1,
        |challenge, public| material.build_csr(challenge, public),
    )
    .await?;
    let saved = save_issued(paths, &issued, "rkp-import-", None)?;
    let key = &saved.keys[0];
    Ok(json!({
        "stage":"exported", "material":material.report(), "csr_path":saved.csr_path,
        "keybox_path":key.keybox_path, "device_id":key.device_id, "chain_summary":key.summary,
        "local_verify":issued.report, "server_accepted":true,
        "certificates_verified":true, "keybox_exported":true,
    }))
}

fn build_request(
    material: &material::ImportedMaterial,
    include: Option<bool>,
    challenge: &[u8],
    public: &[ciborium::value::Value],
) -> Result<Vec<u8>> {
    let config = material.issuance();
    let fingerprint = if include.unwrap_or(config.is_some_and(|value| value.include_fingerprint)) {
        Some(
            config
                .context("explicit issuance configuration is required to include a fingerprint")?
                .fingerprint
                .as_str(),
        )
    } else {
        None
    };
    crate::request::with_fingerprint(&material.build_csr(challenge, public)?, fingerprint)
}

#[cfg(test)]
mod tests;
