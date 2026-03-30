use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use duckd::{
    features::rkp::cose_dice::{DeviceKeys, build_csr, generate_ec_keypair},
    runtime::profile::{DeviceInfo, FingerprintConfig, KeySource, ProfileData},
};

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "duckd-cli-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("ui")).unwrap();
    fs::write(
        root.join("module.prop"),
        "id=duck-toolbox\nname=Duck ToolBox\n",
    )
    .unwrap();
    root
}

fn valid_device_info() -> DeviceInfo {
    DeviceInfo {
        brand: "google".into(),
        model: "Pixel".into(),
        device: "pixel".into(),
        product: "pixel".into(),
        manufacturer: "Google".into(),
        fused: 1,
        vb_state: "green".into(),
        os_version: "13".into(),
        security_level: "tee".into(),
        bootloader_state: "locked".into(),
        boot_patch_level: 20260101,
        system_patch_level: 202601,
        vendor_patch_level: 20260101,
        vbmeta_digest: Some("33".repeat(32)),
        dice_issuer: "CN=Android".into(),
        dice_subject: "CN=Android".into(),
    }
}

fn run(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = run_output(root, args);

    assert!(
        output.status.success(),
        "command failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_output(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_duckd"))
        .env("DUCK_TOOLBOX_ROOT", root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn profile_show_returns_default_profile_json() {
    let root = temp_root();
    let payload = run(&root, &["rkp", "profile", "show", "--json"]);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"], "rkp.profile.show");
    assert_eq!(payload["data"]["profile"]["num_keys"], 1);
}

#[test]
fn profile_save_then_info_round_trip() {
    let root = temp_root();
    let profile = ProfileData {
        key_source: KeySource::Seed {
            seed_hex: "11".repeat(32),
        },
        device: DeviceInfo::default(),
        fingerprint: FingerprintConfig {
            value: "duck/device/model:13/ABC/1:user/release-keys".into(),
        },
        server_url: "https://example.invalid".into(),
        num_keys: 2,
        output_path: "var/outputs/custom-keybox.xml".into(),
    };

    let mut child = Command::new(env!("CARGO_BIN_EXE_duckd"))
        .env("DUCK_TOOLBOX_ROOT", &root)
        .args(["rkp", "profile", "save", "--stdin-json", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(child.stdin.take().unwrap(), &profile).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let info = run(&root, &["rkp", "info", "--json"]);
    assert_eq!(info["ok"], true);
    assert_eq!(
        info["data"]["fingerprint"],
        "duck/device/model:13/ABC/1:user/release-keys"
    );
    assert_eq!(info["data"]["num_keys"], 2);
    assert_eq!(info["data"]["mode"], "direct-seed");
    assert_eq!(info["command"], "rkp.info");
}

#[test]
fn verify_command_emits_json_contract() {
    let root = temp_root();
    let keys = DeviceKeys::from_seed([0x11; 32]);
    let ec_key = generate_ec_keypair().unwrap();
    let csr = build_csr(
        &keys,
        &[0x22; 32],
        &[ec_key.cose_public],
        &[0x33; 32],
        &[0x44; 8],
        &valid_device_info(),
        2,
    )
    .unwrap();

    let csr_path = root.join("var").join("outputs").join("fixture.cbor");
    fs::create_dir_all(csr_path.parent().unwrap()).unwrap();
    fs::write(&csr_path, csr.csr_bytes).unwrap();

    let verify = run(
        &root,
        &["rkp", "verify", "var/outputs/fixture.cbor", "--json"],
    );
    assert_eq!(verify["ok"], true);
    assert_eq!(verify["command"], "rkp.verify");
    assert_eq!(verify["data"]["report"]["signature_valid"], true);
}

#[test]
fn keybox_preflight_rejects_incomplete_device_profile() {
    let root = temp_root();
    let profile = ProfileData {
        key_source: KeySource::Seed {
            seed_hex: "11".repeat(32),
        },
        device: DeviceInfo::default(),
        fingerprint: FingerprintConfig {
            value: "duck/device/model:13/ABC/1:user/release-keys".into(),
        },
        server_url: "https://example.invalid".into(),
        num_keys: 1,
        output_path: "var/outputs/keybox.xml".into(),
    };

    let mut save = Command::new(env!("CARGO_BIN_EXE_duckd"))
        .env("DUCK_TOOLBOX_ROOT", &root)
        .args(["rkp", "profile", "save", "--stdin-json", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(save.stdin.take().unwrap(), &profile).unwrap();
    let save_output = save.wait_with_output().unwrap();
    assert!(save_output.status.success());

    let output = run_output(&root, &["rkp", "keybox", "--json"]);
    assert!(!output.status.success());

    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["command"], "rkp.keybox");
    assert_eq!(payload["error"]["code"], "missing_device_field");
}
