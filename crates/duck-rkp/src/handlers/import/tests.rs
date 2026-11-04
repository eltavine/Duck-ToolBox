use super::{Operation, run};
use crate::{
    cbor::{MAX_INPUT_BYTES, bytes, encode, int, text},
    cli::ImportArgs,
    cose::{
        DICE_KEY_USAGE, DICE_SUBJECT_PUB_KEY, DeviceKeys, build_sig_structure, generate_ec_keypair,
    },
    material,
    profile::DeviceInfo,
    verify::verify_dice_chain,
};
use ciborium::value::Value;
use duck_core::AppPaths;
use serde_json::json;
use std::fs;

fn signed_chain(descriptor_size: usize) -> Vec<u8> {
    let keys = DeviceKeys::from_seed([0x11; 32]);
    let payload = encode(&Value::Map(vec![
        (int(1), text("root")),
        (int(2), text("leaf")),
        (
            int(DICE_SUBJECT_PUB_KEY),
            bytes(encode(&keys.cose_key()).unwrap()),
        ),
        (int(DICE_KEY_USAGE), bytes(vec![0x20])),
        (int(-4_670_546), bytes(vec![0x22; descriptor_size])),
    ]))
    .unwrap();
    let protected = vec![0xa1, 0x18, 0x01, 0x27];
    let signature = keys.sign(&build_sig_structure("Signature1", &protected, &payload).unwrap());
    encode(&Value::Array(vec![
        keys.cose_key(),
        Value::Array(vec![
            bytes(protected),
            Value::Map(vec![]),
            bytes(payload),
            bytes(signature),
        ]),
    ]))
    .unwrap()
}

fn split_inputs(directory: &tempfile::TempDir, chain: &[u8]) -> (AppPaths, ImportArgs) {
    let paths = AppPaths::for_root(directory.path(), directory.path());
    fs::write(directory.path().join("chain.cbor"), chain).unwrap();
    fs::write(
        directory.path().join("signing.json"),
        serde_json::to_vec(&json!({
            "kind": "ed25519-seed", "bytes_hex": "11".repeat(32),
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.path().join("config.json"),
        serde_json::to_vec(&json!({
            "device": DeviceInfo::default(),
            "issuance": {
                "fingerprint": "test/device:13/build/1:user/release-keys",
                "server_url": "https://example.test/v1", "protocol": "v3",
            },
        }))
        .unwrap(),
    )
    .unwrap();
    (
        paths,
        ImportArgs {
            bundle: None,
            chain: Some("chain.cbor".into()),
            signing: Some("signing.json".into()),
            config: Some("config.json".into()),
            include_fingerprint: None,
        },
    )
}

#[tokio::test]
async fn validation_rejects_a_valid_chain_that_cannot_fit_the_complete_csr() {
    let chain = signed_chain(261_900);
    assert!(chain.len() <= MAX_INPUT_BYTES);
    assert_eq!(verify_dice_chain(&chain).unwrap().dice_entries, 2);
    let directory = tempfile::tempdir().unwrap();
    let (paths, args) = split_inputs(&directory, &chain);
    let material = material::load(
        None,
        Some(&directory.path().join("chain.cbor")),
        Some(&directory.path().join("signing.json")),
        Some(&directory.path().join("config.json")),
    )
    .unwrap();
    let error = material
        .build_csr(&[0; 32], &[generate_ec_keypair().unwrap().cose_public])
        .unwrap_err();
    assert!(error.to_string().contains("1..262144 bytes"), "{error}");
    let result = run(&paths, &args, Operation::Validate).await;
    assert!(
        result.is_err(),
        "unconstructable material was reported ready: {result:?}; chain length {}",
        chain.len()
    );
    let error = result.unwrap_err();
    assert_eq!(material::error_code(&error), Some("material_configuration"));
    assert!(format!("{error:#}").contains("1..262144 bytes"));
}

#[tokio::test]
async fn validation_reserves_space_for_a_maximum_length_challenge() {
    let chain = signed_chain(261_494);
    assert_eq!(chain.len(), 261_694);
    let directory = tempfile::tempdir().unwrap();
    let (paths, args) = split_inputs(&directory, &chain);
    let material = material::load(
        None,
        Some(&directory.path().join("chain.cbor")),
        Some(&directory.path().join("signing.json")),
        Some(&directory.path().join("config.json")),
    )
    .unwrap();
    let public = [generate_ec_keypair().unwrap().cose_public];
    let short_csr = material.build_csr(&[0; 32], &public).unwrap();
    assert_eq!(short_csr.len(), 262_128);
    assert_eq!(&short_csr[3..3 + chain.len()], chain);
    let error = material.build_csr(&[0; 64], &public).unwrap_err();
    assert!(error.to_string().contains("1..262144 bytes"), "{error}");
    let result = run(&paths, &args, Operation::Validate).await;
    assert!(
        result.is_err(),
        "32-byte-only readiness was accepted: {result:?}"
    );
    let error = result.unwrap_err();
    assert_eq!(material::error_code(&error), Some("material_configuration"));
    assert!(format!("{error:#}").contains("1..262144 bytes"));
}

#[tokio::test]
async fn validation_accepts_the_complete_csr_size_limit_without_persisting_a_key() {
    let chain = signed_chain(261_478);
    let directory = tempfile::tempdir().unwrap();
    let (paths, args) = split_inputs(&directory, &chain);
    let result = run(&paths, &args, Operation::Validate).await.unwrap();
    assert_eq!(result["ready_for_issuance"], true);
    assert_eq!(result["stage"], "validated");
    assert!(!paths.var_dir.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
    assert_eq!(
        fs::read(directory.path().join("chain.cbor")).unwrap(),
        chain
    );
    let material = material::load(
        None,
        Some(&directory.path().join("chain.cbor")),
        Some(&directory.path().join("signing.json")),
        Some(&directory.path().join("config.json")),
    )
    .unwrap();
    let csr = material
        .build_csr(&[0; 64], &[generate_ec_keypair().unwrap().cose_public])
        .unwrap();
    assert_eq!(csr.len(), MAX_INPUT_BYTES);
    assert_eq!(&csr[3..3 + chain.len()], chain);
}

#[tokio::test]
async fn metadata_size_is_included_in_import_readiness_and_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let (paths, mut args) = split_inputs(&directory, &signed_chain(261_478));
    args.include_fingerprint = Some(true);
    let error = run(&paths, &args, Operation::Validate).await.unwrap_err();
    assert_eq!(material::error_code(&error), Some("material_configuration"));
    assert!(format!("{error:#}").contains("CSR exceeds the input limit"));
    // This endpoint is unreachable; a size error proves issuance stops before HTTP.
    let error = run(&paths, &args, Operation::Issue).await.unwrap_err();
    assert!(format!("{error:#}").contains("CSR exceeds the input limit"));
    assert!(!paths.var_dir.exists());
    args.include_fingerprint = Some(false);
    assert_eq!(
        run(&paths, &args, Operation::Validate).await.unwrap()["ready_for_issuance"],
        true
    );
}

#[tokio::test]
async fn validation_without_complete_configuration_is_successful_but_not_ready() {
    for missing in ["device", "issuance", "both"] {
        let directory = tempfile::tempdir().unwrap();
        let (paths, mut args) = split_inputs(&directory, &signed_chain(261_900));
        if missing == "both" {
            args.config = None;
        } else {
            let config_path = directory.path().join("config.json");
            let mut config: serde_json::Value =
                serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
            config.as_object_mut().unwrap().remove(missing);
            fs::write(config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        }
        let result = run(&paths, &args, Operation::Validate).await.unwrap();
        assert_eq!(result["stage"], "validated");
        assert_eq!(result["ready_for_issuance"], false, "missing {missing}");
        assert!(!paths.var_dir.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
    }
}
