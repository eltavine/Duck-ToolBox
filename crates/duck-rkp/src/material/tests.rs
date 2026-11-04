use super::load;
use crate::{
    cbor::{as_array, bytes, decode, encode, int, text},
    cose::{DICE_SUBJECT_PUB_KEY, DeviceKeys, build_sig_structure, generate_ec_keypair},
    profile::DeviceInfo,
    verify::verify_csr,
};
use ciborium::value::Value;
use serde_json::json;
use std::{fs, path::PathBuf};

fn chain(seed: [u8; 32], noncanonical: bool) -> Vec<u8> {
    let keys = DeviceKeys::from_seed(seed);
    let payload = Value::Map(vec![
        (int(2), text("leaf")),
        (int(1), text("root")),
        (
            int(DICE_SUBJECT_PUB_KEY),
            bytes(encode(&keys.cose_key()).unwrap()),
        ),
        (int(-4_670_553), bytes(vec![0x20])),
    ]);
    let mut encoded = Vec::new();
    if noncanonical {
        ciborium::ser::into_writer(&payload, &mut encoded).unwrap();
    } else {
        encoded = encode(&payload).unwrap();
    }
    let protected = vec![0xa1, 0x18, 0x01, 0x27];
    let input = build_sig_structure("Signature1", &protected, &encoded).unwrap();
    encode(&Value::Array(vec![
        keys.cose_key(),
        Value::Array(vec![
            bytes(protected),
            Value::Map(vec![]),
            bytes(encoded),
            bytes(keys.sign(&input)),
        ]),
    ]))
    .unwrap()
}

fn bundle(chain: &[u8], kind: &str, material: &[u8]) -> serde_json::Value {
    json!({"version":1,"dice_chain_hex":hex::encode(chain),"signing":{"kind":kind,"bytes_hex":hex::encode(material)},"device":DeviceInfo::default(),"issuance":{"fingerprint":"test/device:13/build/1:user/release-keys","server_url":"https://example.test/v1","protocol":"v3"}})
}

fn file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn imported_chain_signs_new_challenge_without_rewriting_signed_bytes() {
    let original = chain([0x11; 32], true);
    let dir = tempfile::tempdir().unwrap();
    let path = file(
        &dir,
        "bundle.json",
        &serde_json::to_vec(&bundle(&original, "ed25519-seed", &[0x11; 32])).unwrap(),
    );
    let material = load(Some(&path), None, None, None).unwrap();
    assert_eq!(material.report().dice_entries, 2);
    assert!(material.report().key_matches);
    let csr = material
        .build_csr(&[0x55; 32], &[generate_ec_keypair().unwrap().cose_public])
        .unwrap();
    assert!(
        csr.windows(original.len())
            .any(|candidate| candidate == original)
    );
    assert!(verify_csr(&csr).unwrap().signature_valid);
    let decoded = decode(&csr).unwrap();
    let request = as_array(&decoded, "request").unwrap();
    let signed = as_array(&request[3], "signed").unwrap();
    let payload = decode(crate::cbor::as_bytes(&signed[2], "payload").unwrap()).unwrap();
    assert_eq!(
        as_array(&payload, "signed payload").unwrap()[0],
        bytes(vec![0x55; 32])
    );
}

#[test]
fn mismatched_material_and_unknown_derivation_are_rejected() {
    let original = chain([0x11; 32], false);
    let dir = tempfile::tempdir().unwrap();
    let path = file(
        &dir,
        "bundle.json",
        &serde_json::to_vec(&bundle(&original, "ed25519-seed", &[0x12; 32])).unwrap(),
    );
    assert!(load(Some(&path), None, None, None).is_err());
    let mut value = bundle(&original, "cdi-attest", &[0x11; 32]);
    value["signing"]["derivation"] = json!("vendor-unknown");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load(Some(&path), None, None, None).is_err());
}

#[test]
fn raw_chain_can_validate_without_issuance_defaults() {
    let original = chain([0x11; 32], false);
    let dir = tempfile::tempdir().unwrap();
    let raw = file(&dir, "chain.cbor", &original);
    let signing=file(&dir,"signing.json",br#"{"kind":"ed25519-seed","bytes_hex":"1111111111111111111111111111111111111111111111111111111111111111"}"#);
    let material = load(None, Some(&raw), Some(&signing), None).unwrap();
    assert!(material.device().is_none());
    assert!(material.issuance().is_none());
    assert!(material.build_csr(&[0; 32], &[]).is_err());
}

fn chain_with_subject(subject: Value) -> Vec<u8> {
    let root = DeviceKeys::from_seed([0x11; 32]);
    let payload = encode(&Value::Map(vec![
        (int(1), text("root")),
        (int(2), text("leaf")),
        (int(DICE_SUBJECT_PUB_KEY), bytes(encode(&subject).unwrap())),
        (int(-4_670_553), bytes(vec![0x20])),
    ]))
    .unwrap();
    let protected = vec![0xa1, 0x01, 0x27];
    let signature = root.sign(&build_sig_structure("Signature1", &protected, &payload).unwrap());
    encode(&Value::Array(vec![
        root.cose_key(),
        Value::Array(vec![
            bytes(protected),
            Value::Map(vec![]),
            bytes(payload),
            bytes(signature),
        ]),
    ]))
    .unwrap()
}

#[test]
fn direct_ec_scalars_sign_with_the_leaf_curve_of_a_mixed_chain() {
    use p256::elliptic_curve::sec1::ToSec1Point;
    for (kind, curve, algorithm, scalar) in [
        ("p256-scalar", 1, -7, vec![0x22; 32]),
        ("p384-scalar", 2, -35, vec![0x33; 48]),
    ] {
        let public = if curve == 1 {
            p256::SecretKey::from_slice(&scalar)
                .unwrap()
                .public_key()
                .to_sec1_point(false)
                .as_bytes()
                .to_vec()
        } else {
            p384::SecretKey::from_slice(&scalar)
                .unwrap()
                .public_key()
                .to_sec1_point(false)
                .as_bytes()
                .to_vec()
        };
        let width = (public.len() - 1) / 2;
        let subject = Value::Map(vec![
            (int(1), int(2)),
            (int(3), int(algorithm)),
            (int(-1), int(curve)),
            (int(-2), bytes(public[1..1 + width].to_vec())),
            (int(-3), bytes(public[1 + width..].to_vec())),
        ]);
        let original = chain_with_subject(subject);
        let dir = tempfile::tempdir().unwrap();
        let path = file(
            &dir,
            "bundle.json",
            &serde_json::to_vec(&bundle(&original, kind, &scalar)).unwrap(),
        );
        let material = load(Some(&path), None, None, None).unwrap();
        assert_eq!(material.algorithm(), algorithm);
        assert_eq!(material.root_algorithm(), -8);
        let public = [generate_ec_keypair().unwrap().cose_public];
        let csr = material.build_csr(&[0x44; 32], &public).unwrap();
        for fingerprint in [None, Some("source/device/build")] {
            let submitted = crate::request::with_fingerprint(&csr, fingerprint).unwrap();
            let report =
                crate::workflow::validate_request_binding(&submitted, &[0x44; 32], &public)
                    .unwrap();
            assert!(report.signature_valid);
            assert_eq!(report.unverified_fingerprint.as_deref(), fingerprint);
            assert!(
                submitted
                    .windows(original.len())
                    .any(|bytes| bytes == original)
            );
        }
    }
}

#[test]
fn open_dice_cdi_matches_the_official_zero_input_public_key() {
    // Open DICE include/dice/known_test_values.h ZeroInput CDI and Ed25519 certificate
    let cdi =
        hex::decode("fbfc679771342eeacb908659ce49d6b63b4535da2c51433d7f04efa6319e0c19").unwrap();
    let public =
        hex::decode("0d14e5de292eb1c8b31beae43ab55d8e9dc014b73eaa83b925a0788cc62e5c8d").unwrap();
    let subject = Value::Map(vec![
        (int(1), int(1)),
        (int(3), int(-8)),
        (int(-1), int(6)),
        (int(-2), bytes(public)),
    ]);
    let original = chain_with_subject(subject);
    let mut value = bundle(&original, "cdi-attest", &cdi);
    value["signing"]["derivation"] = json!("open-dice-hkdf-sha512-ed25519");
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "bundle.json", &serde_json::to_vec(&value).unwrap());
    let material = load(Some(&path), None, None, None).unwrap();
    assert_eq!(material.report().material_type, "cdi-attest");
    assert!(
        verify_csr(
            &material
                .build_csr(&[0x44; 32], &[generate_ec_keypair().unwrap().cose_public])
                .unwrap()
        )
        .unwrap()
        .signature_valid
    );
    value["signing"]["kind"] = json!("auto");
    value["signing"]["derivation"] = json!("auto");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        load(Some(&path), None, None, None)
            .unwrap()
            .report()
            .material_type,
        "cdi-attest"
    );
}

#[test]
fn auto_recognizes_a_supported_key_only_after_public_key_matching() {
    let original = chain([0x11; 32], false);
    let dir = tempfile::tempdir().unwrap();
    let path = file(
        &dir,
        "bundle.json",
        &serde_json::to_vec(&bundle(&original, "auto", &[0x11; 32])).unwrap(),
    );
    assert_eq!(
        load(Some(&path), None, None, None)
            .unwrap()
            .report()
            .material_type,
        "ed25519-seed"
    );
    fs::write(
        &path,
        serde_json::to_vec(&bundle(&original, "auto", &[0x12; 32])).unwrap(),
    )
    .unwrap();
    let error = load(Some(&path), None, None, None).err().unwrap();
    assert_eq!(super::error_code(&error), Some("material_key_mismatch"));
}

#[test]
fn invalid_configuration_and_conflicting_sources_are_rejected() {
    let original = chain([0x11; 32], false);
    let dir = tempfile::tempdir().unwrap();
    let mut value = bundle(&original, "ed25519-seed", &[0x11; 32]);
    value["issuance"]["protocol"] = json!("v1");
    let path = file(&dir, "bundle.json", &serde_json::to_vec(&value).unwrap());
    let error = load(Some(&path), None, None, None).err().unwrap();
    assert_eq!(super::error_code(&error), Some("material_configuration"));
    assert!(load(Some(&path), Some(&path), None, None).is_err());
    value["issuance"]["protocol"] = json!("v3");
    value["device"]["brand"] = json!("");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load(Some(&path), None, None, None).is_err());
}

#[test]
fn malformed_json_and_private_scalars_are_rejected_without_echoing_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "bad.json", br#"{"version":1,"version":1}"#);
    assert!(load(Some(&path), None, None, None).is_err());
    let raw = file(&dir, "chain.cbor", &chain([0x11; 32], false));
    let signing=file(&dir,"signing.json",br#"{"kind":"p256-scalar","bytes_hex":"0000000000000000000000000000000000000000000000000000000000000000"}"#);
    assert!(load(None, Some(&raw), Some(&signing), None).is_err());
    fs::write(
        &signing,
        br#"{"kind":"ed25519-seed","bytes_hex":"SENSITIVE_VALUE"}"#,
    )
    .unwrap();
    let error = load(None, Some(&raw), Some(&signing), None).err().unwrap();
    assert!(!format!("{error:#}").contains("SENSITIVE_VALUE"));
}

#[test]
fn indefinite_chain_item_is_spliced_verbatim_and_tampering_is_rejected() {
    let mut original = chain([0x11; 32], true);
    original[0] = 0x9f;
    original.push(0xff);
    let dir = tempfile::tempdir().unwrap();
    let path = file(
        &dir,
        "bundle.json",
        &serde_json::to_vec(&bundle(&original, "ed25519-seed", &[0x11; 32])).unwrap(),
    );
    let material = load(Some(&path), None, None, None).unwrap();
    let csr = material
        .build_csr(&[], &[generate_ec_keypair().unwrap().cose_public])
        .unwrap();
    assert_eq!(&csr[3..3 + original.len()], original);
    let last = original.len() - 2;
    original[last] ^= 1;
    fs::write(
        &path,
        serde_json::to_vec(&bundle(&original, "ed25519-seed", &[0x11; 32])).unwrap(),
    )
    .unwrap();
    assert!(load(Some(&path), None, None, None).is_err());
}

#[test]
fn json_preflight_rejects_deep_wide_duplicate_and_trailing_inputs() {
    assert!(super::json::parse::<serde_json::Value>(br#"{"a":1,"a":2}"#).is_err());
    assert!(super::json::parse::<serde_json::Value>(b"{}{}").is_err());
    let nested = format!("{}0{}", "[".repeat(17), "]".repeat(17));
    assert!(super::json::parse::<serde_json::Value>(nested.as_bytes()).is_err());
    let wide = serde_json::to_vec(&(0..65).collect::<Vec<_>>()).unwrap();
    assert!(super::json::parse::<serde_json::Value>(&wide).is_err());
}

#[test]
fn external_config_can_fill_missing_bundle_fields_without_overriding() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = bundle(&chain([0x11; 32], false), "ed25519-seed", &[0x11; 32]);
    let device = value.as_object_mut().unwrap().remove("device").unwrap();
    let issuance = value.as_object_mut().unwrap().remove("issuance").unwrap();
    let bundle_path = file(&dir, "bundle.json", &serde_json::to_vec(&value).unwrap());
    let config = file(
        &dir,
        "config.json",
        &serde_json::to_vec(&json!({"device":device,"issuance":issuance})).unwrap(),
    );
    let imported = load(Some(&bundle_path), None, None, Some(&config)).unwrap();
    assert!(imported.device().is_some());
    assert!(imported.issuance().is_some());
    fs::write(
        &bundle_path,
        serde_json::to_vec(&bundle(
            &chain([0x11; 32], false),
            "ed25519-seed",
            &[0x11; 32],
        ))
        .unwrap(),
    )
    .unwrap();
    let error = load(Some(&bundle_path), None, None, Some(&config))
        .err()
        .unwrap();
    assert_eq!(super::error_code(&error), Some("material_configuration"));
}

#[test]
fn non_loopback_plain_http_is_rejected_during_material_validation() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = bundle(&chain([0x11; 32], false), "ed25519-seed", &[0x11; 32]);
    value["issuance"]["server_url"] = json!("http://example.test/v1");
    let path = file(&dir, "bundle.json", &serde_json::to_vec(&value).unwrap());
    assert!(load(Some(&path), None, None, None).is_err());
    value["issuance"]["server_url"] = json!("http://127.0.0.1:12345/v1");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load(Some(&path), None, None, None).is_ok());
}

#[test]
fn bad_chain_signature_reports_the_failing_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut original = chain([0x11; 32], false);
    *original.last_mut().unwrap() ^= 1;
    let path = file(
        &dir,
        "bundle.json",
        &serde_json::to_vec(&bundle(&original, "ed25519-seed", &[0x11; 32])).unwrap(),
    );
    let error = load(Some(&path), None, None, None).err().unwrap();
    assert!(error.to_string().contains("DICE chain entry 1"), "{error}");
}

#[test]
fn automatic_selection_distinguishes_ambiguous_from_no_match() {
    let first = super::signing::Signer::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[0x11; 32]));
    let second =
        super::signing::Signer::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[0x11; 32]));
    let error =
        super::signing::select_unique(vec![(first, "first".into()), (second, "second".into())])
            .err()
            .unwrap();
    assert_eq!(super::error_code(&error), Some("material_ambiguous_match"));
}

#[test]
fn missing_input_has_a_stable_structure_error_code() {
    let error = load(None, None, None, None).err().unwrap();
    assert_eq!(super::error_code(&error), Some("material_structure"));
}

#[test]
fn imported_device_does_not_require_experimental_dice_names() {
    let dir = tempfile::tempdir().unwrap();
    let original = chain([0x11; 32], false);
    let mut value = bundle(&original, "ed25519-seed", &[0x11; 32]);
    let device = value["device"].as_object_mut().unwrap();
    device.remove("dice_issuer");
    device.remove("dice_subject");
    let path = file(&dir, "bundle.json", &serde_json::to_vec(&value).unwrap());
    let material = load(Some(&path), None, None, None).unwrap();
    assert!(material.device().unwrap().dice_issuer.is_empty());
    assert!(material.device().unwrap().dice_subject.is_empty());
    let csr = material
        .build_csr(&[0x55; 32], &[generate_ec_keypair().unwrap().cose_public])
        .unwrap();
    assert_eq!(&csr[3..3 + original.len()], original);
    assert!(verify_csr(&csr).unwrap().signature_valid);
}
