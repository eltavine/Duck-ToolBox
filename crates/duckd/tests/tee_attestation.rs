#![cfg(feature = "tee-attestation")]

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use duck_rkp::{
    cose::{DeviceKeys, build_csr, generate_ec_keypair},
    profile::DeviceInfo,
};
use serde_json::{Value, json};

fn execute(dir: &std::path::Path, args: &[&str], input: Option<Value>) -> Value {
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(
        dir.join("module.prop"),
        "id=duck-toolbox\nname=Duck ToolBox\n",
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_duckd"));
    command
        .env("DUCK_TOOLBOX_ROOT", dir)
        .env("DUCK_TOOLBOX_DATA_ROOT", dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&input).unwrap().as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.success(), response["ok"].as_bool().unwrap());
    response
}

fn identity_fixture(dir: &std::path::Path) {
    let device = DeviceInfo {
        brand: "test".into(),
        model: "fixture".into(),
        device: "fixture".into(),
        product: "fixture".into(),
        manufacturer: "test".into(),
        fused: 1,
        vb_state: "green".into(),
        os_version: "13".into(),
        security_level: "tee".into(),
        bootloader_state: "locked".into(),
        boot_patch_level: 20260101,
        system_patch_level: 202601,
        vendor_patch_level: 20260101,
        vbmeta_digest: Some("11".repeat(32)),
        dice_issuer: "CN=Android".into(),
        dice_subject: "CN=Android".into(),
    };
    let keys = DeviceKeys::from_seed([0x11; 32]);
    let csr = build_csr(
        &keys,
        &[0x22; 32],
        &[generate_ec_keypair().unwrap().cose_public],
        &[0x33; 32],
        &[0x44; 8],
        &device,
        2,
    )
    .unwrap();
    fs::write(dir.join("csr.cbor"), csr.csr_bytes).unwrap();
    fs::write(dir.join("identity.bin"), [0x11; 32]).unwrap();
    fs::write(dir.join("wrong.bin"), [0x12; 32]).unwrap();
}

#[test]
fn seed_verification_and_export_use_the_same_contract_without_leaking_material() {
    let dir = tempfile::tempdir().unwrap();
    identity_fixture(dir.path());
    let verified = execute(
        dir.path(),
        &[
            "tee-attestation",
            "seed",
            "identity.bin",
            "--csr",
            "csr.cbor",
        ],
        None,
    );
    assert_eq!(verified["data"]["report"]["identity_matches"], true);
    assert!(verified["data"]["artifacts"].is_null());
    assert_eq!(
        fs::read_dir(dir.path().join("var/outputs"))
            .unwrap()
            .count(),
        0
    );
    let exported = execute(
        dir.path(),
        &["tee-attestation", "seed", "--stdin-json"],
        Some(json!({
            "source":{"path":"identity.bin"},"csr":{"path":"csr.cbor"},"export":true,
        })),
    );
    assert_eq!(exported["ok"], true);
    let path = exported["data"]["artifacts"]["material_path"]
        .as_str()
        .unwrap();
    assert_eq!(fs::read(path).unwrap(), [0x11; 32]);
    let log = fs::read_to_string(dir.path().join("var/logs/duckd.log")).unwrap();
    for response in [verified.to_string(), exported.to_string(), log] {
        assert!(!response.contains(&"11".repeat(32)));
    }
}

#[test]
fn mismatched_identity_is_reported_and_cannot_be_exported() {
    let dir = tempfile::tempdir().unwrap();
    identity_fixture(dir.path());
    let verified = execute(
        dir.path(),
        &["tee-attestation", "seed", "wrong.bin", "--csr", "csr.cbor"],
        None,
    );
    assert_eq!(verified["data"]["report"]["identity_matches"], false);
    let blocked = execute(
        dir.path(),
        &[
            "tee-attestation",
            "seed",
            "wrong.bin",
            "--csr",
            "csr.cbor",
            "--export",
        ],
        None,
    );
    assert_eq!(blocked["error"]["code"], "identity_mismatch");
    assert_eq!(
        fs::read_dir(dir.path().join("var/outputs"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn malformed_request_does_not_echo_input_into_the_error_or_history() {
    let dir = tempfile::tempdir().unwrap();
    let marker = "private-material-sentinel";
    let response = execute(
        dir.path(),
        &["tee-attestation", "keybox", "--stdin-json"],
        Some(json!({
            "source":{"base64":"YWJj"},"format":marker,
        })),
    );
    assert_eq!(response["error"]["code"], "invalid_input");
    assert!(!response.to_string().contains(marker));
    assert!(
        !fs::read_to_string(dir.path().join("var/logs/duckd.log"))
            .unwrap()
            .contains(marker)
    );
}
