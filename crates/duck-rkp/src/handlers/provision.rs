use duck_core::{AppPaths, Failure};
use serde::Serialize;

use super::{ensure_request_context, random_bytes, resolve_runtime, resolve_runtime_offline};
use crate::{
    artifacts::{SavedKey, private_run, save_issued},
    cli::ProvisionArgs,
    cose::{DeviceKeys, build_experimental_csr_v3, generate_ec_keypair},
    crypto_kdf::resolve_seed,
    private_output::write_new_verified,
    validate::validate_device_info_for_request,
    verify::VerifyReport,
    workflow::{issue, validate_request_binding},
};

#[derive(Serialize)]
pub(crate) struct ProvisionData {
    mode: &'static str,
    curve: String,
    challenge_hex: String,
    csr_path: String,
    csr_len: usize,
    protected_data_len: usize,
    local_verify: VerifyReport,
    cert_chains: Vec<SavedKey>,
    local_test_mode: bool,
    diagnostic_only: bool,
    fetch_eek_error: Option<String>,
    server_submission_error: Option<String>,
}

pub(crate) async fn run(paths: &AppPaths, args: &ProvisionArgs) -> Result<ProvisionData, Failure> {
    let resolved = if args.offline {
        resolve_runtime_offline(paths, &args.shared, args.num_keys)?
    } else {
        resolve_runtime(paths, &args.shared, args.num_keys, None)?
    };
    if !(1..=16).contains(&resolved.profile.num_keys) {
        return Err(anyhow::anyhow!("request must contain between 1 and 16 keys").into());
    }
    let keys = DeviceKeys::from_seed_with_curve(
        resolve_seed(&resolved.profile.key_source)?,
        resolved.profile.curve,
    );
    let build = |challenge: &[u8], public: &[ciborium::value::Value]| {
        build_experimental_csr_v3(&keys, challenge, public, &resolved.profile.device)
    };
    let (csr, challenge, report, saved_keys, csr_path) = if args.offline {
        validate_device_info_for_request(&resolved.profile.device)?;
        let challenge = random_bytes(32)?;
        let public = (0..resolved.profile.num_keys)
            .map(|_| generate_ec_keypair().map(|key| key.cose_public))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let csr = crate::request::with_fingerprint(
            &build(&challenge, &public)?,
            resolved
                .profile
                .fingerprint
                .include_fingerprint
                .then_some(resolved.profile.fingerprint.value.as_str()),
        )?;
        let report = validate_request_binding(&csr, &challenge, &public)?;
        let directory = private_run(paths, "rkp-experimental-diagnostic-")?;
        let path = directory.path().join("diagnostic-only.cbor");
        write_new_verified(&path, &csr, |bytes| {
            validate_request_binding(bytes, &challenge, &public).map(|_| ())
        })?;
        let _ = directory.keep();
        (
            csr,
            challenge,
            report,
            Vec::new(),
            path.display().to_string(),
        )
    } else {
        let server_url = ensure_request_context(&resolved.profile)?;
        let issued = issue(
            &resolved.profile.fingerprint.value,
            resolved.profile.fingerprint.include_fingerprint,
            &server_url,
            resolved.profile.num_keys,
            build,
        )
        .await?;
        let saved = save_issued(paths, &issued, "rkp-experimental-", None)?;
        (
            issued.csr,
            issued.challenge,
            issued.report,
            saved.keys,
            saved.csr_path,
        )
    };
    Ok(ProvisionData {
        mode: "experimental",
        curve: resolved.profile.curve.as_str().into(),
        challenge_hex: hex::encode(challenge),
        csr_len: csr.len(),
        csr_path,
        protected_data_len: 0,
        local_verify: report,
        cert_chains: saved_keys,
        local_test_mode: args.offline,
        diagnostic_only: args.offline,
        fetch_eek_error: None,
        server_submission_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::SharedRunArgs,
        profile::{KeySource, ProfileData, save_profile},
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[tokio::test]
    async fn challenge_failure_is_an_error_without_submission_or_artifacts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8192];
            let n = socket.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).contains(":fetchEekChain"));
            socket
                .write_all(
                    b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let profile = ProfileData {
            key_source: KeySource::Seed {
                seed_hex: "11".repeat(32),
            },
            server_url: format!("http://{address}/v1"),
            ..ProfileData::default()
        };
        save_profile(&paths, None, &profile).unwrap();
        let result = run(
            &paths,
            &ProvisionArgs {
                shared: SharedRunArgs::default(),
                num_keys: None,
                offline: false,
            },
        )
        .await;
        worker.join().unwrap();
        assert!(
            result.is_err(),
            "online failure must not be reported as a successful offline run"
        );
        assert_eq!(std::fs::read_dir(&paths.outputs_dir).unwrap().count(), 0);
    }

    fn diagnostic_profile() -> ProfileData {
        ProfileData {
            key_source: KeySource::Seed {
                seed_hex: "11".repeat(32),
            },
            ..ProfileData::default()
        }
    }

    fn diagnostic_args(num_keys: Option<u32>) -> ProvisionArgs {
        ProvisionArgs {
            shared: SharedRunArgs::default(),
            num_keys,
            offline: true,
        }
    }

    #[tokio::test]
    async fn offline_does_not_require_a_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let mut profile = diagnostic_profile();
        profile.fingerprint.value.clear();
        save_profile(&paths, None, &profile).unwrap();

        let result = run(&paths, &diagnostic_args(None)).await.unwrap();

        assert!(result.diagnostic_only);
        assert!(result.local_verify.signature_valid);
        assert!(result.cert_chains.is_empty());
    }

    #[tokio::test]
    async fn offline_ignores_missing_or_invalid_stored_endpoints() {
        for endpoint in ["", "not a URL", "file:///private/diagnostic"] {
            let dir = tempfile::tempdir().unwrap();
            let paths = AppPaths::for_root(dir.path(), dir.path());
            save_profile(&paths, None, &diagnostic_profile()).unwrap();
            let mut public: toml::Table =
                toml::from_str(&std::fs::read_to_string(&paths.profile_path).unwrap()).unwrap();
            public["server_url"] = toml::Value::String(endpoint.into());
            let stored_profile = toml::to_string(&public).unwrap();
            std::fs::write(&paths.profile_path, &stored_profile).unwrap();

            let result = run(&paths, &diagnostic_args(None)).await.unwrap();

            assert!(result.local_verify.signature_valid);
            assert_eq!(
                std::fs::read_to_string(&paths.profile_path).unwrap(),
                stored_profile
            );
        }
    }

    #[tokio::test]
    async fn offline_ignores_missing_or_invalid_endpoint_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        save_profile(&paths, None, &diagnostic_profile()).unwrap();
        for endpoint in ["", "not a URL", "file:///private/diagnostic"] {
            let mut args = diagnostic_args(None);
            args.shared.server_url = Some(endpoint.into());

            let result = run(&paths, &args).await.unwrap();

            assert!(result.local_verify.signature_valid);
        }
    }

    #[tokio::test]
    async fn offline_uses_saved_key_count_and_command_override() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let mut profile = diagnostic_profile();
        profile.num_keys = 3;
        save_profile(&paths, None, &profile).unwrap();
        for (requested, expected) in [(None, 3), (Some(16), 16), (Some(0), 1)] {
            let result = run(&paths, &diagnostic_args(requested)).await.unwrap();
            let csr = std::fs::read(&result.csr_path).unwrap();
            let report = crate::verify::verify_csr(&csr).unwrap();

            assert_eq!(result.local_verify.keys_to_sign, expected);
            assert_eq!(report.keys_to_sign, expected);
        }
    }

    #[tokio::test]
    async fn offline_rejects_excessive_key_counts_without_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        save_profile(&paths, None, &diagnostic_profile()).unwrap();
        for requested in [17, u32::MAX] {
            let error = run(&paths, &diagnostic_args(Some(requested)))
                .await
                .err()
                .expect("offline key counts must have the same bound as online requests");

            assert!(error.error.to_string().contains("between 1 and 16"));
            assert_eq!(std::fs::read_dir(&paths.outputs_dir).unwrap().count(), 0);
        }
    }

    #[tokio::test]
    async fn incomplete_device_context_is_rejected_offline_and_online() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let mut profile = diagnostic_profile();
        profile.device.brand.clear();
        save_profile(&paths, None, &profile).unwrap();
        for offline in [true, false] {
            let mut args = diagnostic_args(None);
            args.offline = offline;
            let error = run(&paths, &args).await.err().unwrap();

            assert!(error.error.to_string().contains("brand"));
            assert_eq!(std::fs::read_dir(&paths.outputs_dir).unwrap().count(), 0);
        }
    }

    #[tokio::test]
    async fn online_still_requires_a_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(dir.path(), dir.path());
        let mut profile = diagnostic_profile();
        profile.fingerprint.value.clear();
        save_profile(&paths, None, &profile).unwrap();
        let mut args = diagnostic_args(None);
        args.offline = false;

        let error = run(&paths, &args).await.err().unwrap();

        assert!(error.error.to_string().contains("fingerprint"));
        assert_eq!(std::fs::read_dir(&paths.outputs_dir).unwrap().count(), 0);
    }
}
