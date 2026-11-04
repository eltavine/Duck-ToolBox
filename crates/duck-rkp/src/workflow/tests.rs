use super::*;
use crate::{
    cose::{DeviceKeys, build_experimental_csr_v3},
    profile::DeviceInfo,
};

#[test]
fn rejects_old_challenge_and_old_public_keys_even_with_a_valid_signature() {
    let keys = DeviceKeys::from_seed([4; 32]);
    let first = generate_ec_keypair().unwrap().cose_public;
    let second = generate_ec_keypair().unwrap().cose_public;
    let csr = build_experimental_csr_v3(
        &keys,
        &[1; 32],
        std::slice::from_ref(&first),
        &DeviceInfo::default(),
    )
    .unwrap();
    assert!(validate_request_binding(&csr, &[2; 32], std::slice::from_ref(&first)).is_err());
    assert!(validate_request_binding(&csr, &[1; 32], &[second]).is_err());
    assert!(validate_request_binding(&csr, &[1; 32], &[first]).is_ok());
}

#[tokio::test]
async fn simulated_server_binds_fresh_request_and_selects_the_matching_response() {
    let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
    let keys = DeviceKeys::from_seed([4; 32]);
    let issued = issue(
        "synthetic/fingerprint",
        false,
        &server.url,
        1,
        |challenge, public| {
            build_experimental_csr_v3(&keys, challenge, public, &DeviceInfo::default())
        },
    )
    .await
    .unwrap();
    assert_eq!(issued.challenge, [9; 32]);
    assert_eq!(issued.keys.len(), 1);
    let directory = tempfile::tempdir().unwrap();
    let paths = duck_core::AppPaths::for_root(directory.path(), directory.path());
    let saved = crate::artifacts::save_issued(&paths, &issued, "simulated-", None).unwrap();
    let xml = std::fs::read_to_string(&saved.keys[0].keybox_path).unwrap();
    crate::keybox_xml::validate_keybox_roundtrip(
        &xml,
        &issued.keys[0].pair.secret_key,
        &issued.keys[0].chain,
        &saved.keys[0].device_id,
    )
    .unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn local_failure_makes_zero_network_requests() {
    let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
    assert!(
        issue("fingerprint", false, &server.url, 1, |_, _| anyhow::bail!(
            "invalid local material"
        ))
        .await
        .is_err()
    );
    assert!(server.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn server_rejection_unknown_result_and_wrong_key_are_errors_without_retry() {
    for (reply, code) in [
        (
            super::mock_server::Reply::WrongKey,
            "certificate_validation_failed",
        ),
        (super::mock_server::Reply::Rejected, "rkp_client_error"),
        (super::mock_server::Reply::Unknown, "submission_unknown"),
    ] {
        let server = super::mock_server::Server::start(reply);
        let keys = DeviceKeys::from_seed([4; 32]);
        let error = issue("fingerprint", false, &server.url, 1, |challenge, public| {
            build_experimental_csr_v3(&keys, challenge, public, &DeviceInfo::default())
        })
        .await
        .err()
        .unwrap();
        assert_eq!(crate::error::code_of(&error), code);
        assert!(!error.to_string().contains("do not reflect"));
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

fn imported_bundle(
    directory: &tempfile::TempDir,
    server_url: &str,
) -> (duck_core::AppPaths, crate::cli::ImportArgs, Vec<u8>) {
    let signing = DeviceKeys::from_seed([0x42; 32]);
    let sample =
        build_experimental_csr_v3(&signing, &[0; 32], &[], &DeviceInfo::default()).unwrap();
    let value = decode(&sample).unwrap();
    let chain = encode(&as_array(&value, "CSR").unwrap()[2]).unwrap();
    let bundle = serde_json::json!({"version":1,"dice_chain_hex":hex::encode(&chain),
        "signing":{"kind":"ed25519-seed","bytes_hex":"42".repeat(32)},
        "device":DeviceInfo::default(),
        "issuance":{"fingerprint":"synthetic/explicit/identity","server_url":server_url,"protocol":"v3"}});
    let path = directory.path().join("bundle.json");
    std::fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    (
        duck_core::AppPaths::for_root(directory.path(), directory.path()),
        crate::cli::ImportArgs {
            bundle: Some(path.display().to_string()),
            chain: None,
            signing: None,
            config: None,
            include_fingerprint: None,
        },
        chain,
    )
}

#[tokio::test]
async fn imported_material_without_old_csr_completes_simulated_issuance_and_export() {
    use crate::handlers::import::{Operation, run};
    let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
    let directory = tempfile::tempdir().unwrap();
    let (paths, args, original_chain) = imported_bundle(&directory, &server.url);
    let validated = run(&paths, &args, Operation::Validate).await.unwrap();
    assert_eq!(validated["ready_for_issuance"], true);
    assert!(server.requests.lock().unwrap().is_empty());
    let diagnostic = run(&paths, &args, Operation::Diagnose).await.unwrap();
    assert_eq!(diagnostic["diagnostic_only"], true);
    assert!(server.requests.lock().unwrap().is_empty());
    let result = run(&paths, &args, Operation::Issue).await.unwrap();
    assert_eq!(result["stage"], "exported");
    assert_eq!(result["server_accepted"], true);
    assert_eq!(result["certificates_verified"], true);
    assert_eq!(result["keybox_exported"], true);
    assert!(result.get("keybox_xml").is_none());
    assert!(!result.to_string().contains(&"42".repeat(32)));
    assert!(std::path::Path::new(result["keybox_path"].as_str().unwrap()).is_file());
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .1
            .windows(original_chain.len())
            .any(|bytes| bytes == original_chain)
    );
}

#[tokio::test]
async fn failed_import_issuance_leaves_no_keybox_and_partial_response_fails() {
    use crate::handlers::import::{Operation, run};
    let server = super::mock_server::Server::start(super::mock_server::Reply::WrongKey);
    let directory = tempfile::tempdir().unwrap();
    let (paths, args, _) = imported_bundle(&directory, &server.url);
    assert!(run(&paths, &args, Operation::Issue).await.is_err());
    assert!(!paths.outputs_dir.exists());

    let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
    let keys = DeviceKeys::from_seed([4; 32]);
    assert!(
        issue("fingerprint", false, &server.url, 2, |challenge, public| {
            build_experimental_csr_v3(&keys, challenge, public, &DeviceInfo::default())
        })
        .await
        .is_err()
    );
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}

fn assert_submission_metadata(csr: &[u8], expected: Option<&str>) {
    let value = decode(csr).unwrap();
    let fields = as_array(&value, "CSR").unwrap();
    assert_eq!(fields.len(), if expected.is_some() { 5 } else { 4 });
    let report = crate::verify::verify_csr(csr).unwrap();
    assert_eq!(report.unverified_fingerprint.as_deref(), expected);
}

#[tokio::test]
async fn both_profile_issuance_commands_submit_and_save_the_selected_metadata() {
    use crate::{
        cli::{KeyboxArgs, ProvisionArgs, SharedRunArgs},
        profile::{KeySource, ProfileData, save_profile},
    };
    for include in [false, true] {
        for keybox in [false, true] {
            let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
            let directory = tempfile::tempdir().unwrap();
            let paths = duck_core::AppPaths::for_root(directory.path(), directory.path());
            let mut profile = ProfileData {
                key_source: KeySource::Seed {
                    seed_hex: "44".repeat(32),
                },
                server_url: server.url.clone(),
                ..ProfileData::default()
            };
            profile.fingerprint.value = "synthetic/profile/build".into();
            profile.fingerprint.include_fingerprint = include;
            save_profile(&paths, None, &profile).unwrap();
            let result = if keybox {
                crate::handlers::keybox::run(
                    &paths,
                    &KeyboxArgs {
                        shared: SharedRunArgs::default(),
                        output: None,
                    },
                )
                .await
                .unwrap()
            } else {
                serde_json::to_value(
                    crate::handlers::provision::run(
                        &paths,
                        &ProvisionArgs {
                            shared: SharedRunArgs::default(),
                            num_keys: None,
                            offline: false,
                        },
                    )
                    .await
                    .unwrap(),
                )
                .unwrap()
            };
            let requests = server.requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            assert_submission_metadata(
                &requests[1].1,
                include.then_some("synthetic/profile/build"),
            );
            assert_eq!(
                std::fs::read(result["csr_path"].as_str().unwrap()).unwrap(),
                requests[1].1
            );
            let challenge = decode(&requests[0].1).unwrap();
            assert_eq!(
                crate::cbor::map_get_text(challenge.as_map().unwrap(), "fingerprint")
                    .unwrap()
                    .as_text(),
                Some("synthetic/profile/build")
            );
        }
    }
}

#[tokio::test]
async fn imported_issuance_honors_configuration_and_explicit_boolean_overrides() {
    use crate::handlers::import::{Operation, run};
    for (configured, override_value, expected) in [
        (false, None, false),
        (true, None, true),
        (false, Some(true), true),
        (true, Some(false), false),
    ] {
        let server = super::mock_server::Server::start(super::mock_server::Reply::Valid);
        let directory = tempfile::tempdir().unwrap();
        let (paths, mut args, original_chain) = imported_bundle(&directory, &server.url);
        let path = std::path::Path::new(args.bundle.as_ref().unwrap());
        let mut bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        bundle["issuance"]["include_fingerprint"] = configured.into();
        std::fs::write(path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        args.include_fingerprint = override_value;
        assert_eq!(
            run(&paths, &args, Operation::Validate).await.unwrap()["ready_for_issuance"],
            true
        );
        let diagnostic = run(&paths, &args, Operation::Diagnose).await.unwrap();
        assert_submission_metadata(
            &std::fs::read(diagnostic["csr_path"].as_str().unwrap()).unwrap(),
            expected.then_some("synthetic/explicit/identity"),
        );
        assert!(server.requests.lock().unwrap().is_empty());
        let result = run(&paths, &args, Operation::Issue).await.unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let sent = &requests[1].1;
        assert_submission_metadata(sent, expected.then_some("synthetic/explicit/identity"));
        assert!(
            sent.windows(original_chain.len())
                .any(|part| part == original_chain)
        );
        assert_eq!(
            std::fs::read(result["csr_path"].as_str().unwrap()).unwrap(),
            *sent
        );
    }
}
