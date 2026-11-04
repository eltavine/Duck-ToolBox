use anyhow::Result;
use duck_core::AppPaths;
use serde_json::{Value, json};

use super::{ensure_request_context, resolve_runtime};
use crate::{
    artifacts::{explicit_output, save_issued},
    cli::KeyboxArgs,
    cose::{DeviceKeys, build_experimental_csr_v3},
    crypto_kdf::resolve_seed,
    workflow::issue,
};

pub(crate) async fn run(paths: &AppPaths, args: &KeyboxArgs) -> Result<Value> {
    let resolved = resolve_runtime(paths, &args.shared, Some(1), args.output.clone())?;
    let server_url = ensure_request_context(&resolved.profile)?;
    let output = if args.output.is_some()
        || resolved.profile.output_path != crate::profile::DEFAULT_OUTPUT_PATH
    {
        explicit_output(paths, Some(&resolved.profile.output_path))?
    } else {
        None
    };
    let keys = DeviceKeys::from_seed_with_curve(
        resolve_seed(&resolved.profile.key_source)?,
        resolved.profile.curve,
    );
    let issued = issue(
        &resolved.profile.fingerprint.value,
        resolved.profile.fingerprint.include_fingerprint,
        &server_url,
        1,
        |challenge, public| {
            build_experimental_csr_v3(&keys, challenge, public, &resolved.profile.device)
        },
    )
    .await?;
    let saved = save_issued(paths, &issued, "rkp-experimental-", output.as_deref())?;
    let key = &saved.keys[0];
    Ok(json!({
        "mode":"experimental", "csr_path":saved.csr_path, "keybox_path":key.keybox_path,
        "device_id":key.device_id, "chain_summary":key.summary,
        "server_accepted":true, "certificates_verified":true, "keybox_exported":true,
    }))
}
