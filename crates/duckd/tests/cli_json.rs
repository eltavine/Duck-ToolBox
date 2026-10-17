//! End-to-end tests that run the real `duckd` binary and assert on its JSON envelopes.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use duck_rkp::{
    cose::{DeviceKeys, build_csr, generate_ec_keypair},
    profile::{DeviceInfo, DiceCurve, FingerprintConfig, KeySource, ProfileData},
};
use serde_json::Value;

struct Env {
    root: PathBuf,
    data: PathBuf,
    sysroot: PathBuf,
}

fn env(name: &str) -> Env {
    let base = std::env::temp_dir().join(format!(
        "duckd-cli-{name}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let root = base.join("module");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(
        root.join("module.prop"),
        "id=duck-toolbox\nname=Duck ToolBox\n",
    )
    .unwrap();
    let env = Env {
        data: base.join("data"),
        sysroot: base.join("sys"),
        root,
    };
    fs::create_dir_all(&env.sysroot).unwrap();
    env
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn command(env: &Env) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_duckd"));
    command
        .env("DUCK_TOOLBOX_ROOT", &env.root)
        .env("DUCK_TOOLBOX_DATA_ROOT", &env.data)
        .env("DUCK_TOOLBOX_SYSROOT", &env.sysroot);
    command
}

fn run(env: &Env, args: &[&str]) -> Value {
    let output = command(env).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_json(env: &Env, args: &[&str], stdin: &Value) -> std::process::Output {
    let mut child = command(env)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(child.stdin.take().unwrap(), stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn install_backend(env: &Env, id: &str, version_code: u64) {
    let dir = env.sysroot.join(format!("data/adb/modules/{id}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("module.prop"),
        format!("id={id}\nname={id}\nversionCode={version_code}\n"),
    )
    .unwrap();
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

#[test]
fn every_envelope_reports_api_version() {
    let env = env("api");
    let payload = run(&env, &["rkp", "profile", "show", "--json"]);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["api"], 1);
    assert_eq!(payload["command"], "rkp.profile.show");
    assert_eq!(payload["data"]["profile"]["curve"], "ed25519");
}

#[test]
fn profile_save_then_info_round_trip() {
    let env = env("round-trip");
    let profile = ProfileData {
        key_source: KeySource::Seed {
            seed_hex: "11".repeat(32),
        },
        curve: DiceCurve::P256,
        device: DeviceInfo::default(),
        fingerprint: FingerprintConfig {
            value: "duck/device/model:13/ABC/1:user/release-keys".into(),
        },
        server_url: "https://example.invalid".into(),
        num_keys: 2,
        output_path: "var/outputs/custom-keybox.xml".into(),
    };

    let saved = run_json(
        &env,
        &["rkp", "profile", "save", "--stdin-json"],
        &serde_json::to_value(&profile).unwrap(),
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let info = run(&env, &["rkp", "info", "--json"]);
    assert_eq!(info["data"]["curve"], "p256");
    assert_eq!(info["data"]["num_keys"], 2);
    assert_eq!(info["data"]["mode"], "direct-seed");
    assert_eq!(info["data"]["public_key_hex"].as_str().unwrap().len(), 128);
}

#[test]
fn verify_command_emits_report() {
    let env = env("verify");
    let keys = DeviceKeys::from_seed([0x11; 32]);
    let ec_key = generate_ec_keypair().unwrap();
    let csr = build_csr(
        &keys,
        &[0x22; 32],
        std::slice::from_ref(&ec_key.cose_public),
        &[0x33; 32],
        &[0x44; 8],
        &valid_device_info(),
        2,
    )
    .unwrap();

    let csr_path = env.data.join("var/outputs/fixture.cbor");
    fs::create_dir_all(csr_path.parent().unwrap()).unwrap();
    fs::write(&csr_path, csr.csr_bytes).unwrap();

    let verify = run(
        &env,
        &["rkp", "verify", "var/outputs/fixture.cbor", "--json"],
    );
    assert_eq!(verify["ok"], true);
    assert_eq!(verify["data"]["report"]["signature_valid"], true);
}

#[test]
fn keybox_preflight_rejects_blank_device_field() {
    let env = env("preflight");
    let mut device = DeviceInfo::default();
    device.brand.clear();
    let profile = ProfileData {
        key_source: KeySource::Seed {
            seed_hex: "11".repeat(32),
        },
        device,
        ..ProfileData::default()
    };
    run_json(
        &env,
        &["rkp", "profile", "save", "--stdin-json"],
        &serde_json::to_value(&profile).unwrap(),
    );

    let output = command(&env)
        .args(["rkp", "keybox", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["error"]["code"], "missing_device_field");
}

#[test]
fn device_ids_defaults_reports_dry_run_profile() {
    let env = env("device-ids");
    let payload = run(&env, &["device-ids", "defaults", "--json"]);
    assert_eq!(payload["command"], "device-ids.defaults");
    assert_eq!(payload["data"]["ta_name"], "keymaster64");
}

#[test]
fn features_manifest_lists_every_registered_tool() {
    let env = env("features");
    let payload = run(&env, &["features"]);
    assert_eq!(payload["command"], "features");
    let ids: Vec<&str> = payload["data"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|feature| feature["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "rkp",
            "rkp-device",
            "tee-attestation",
            "device-ids",
            "tricky-store",
            "system"
        ]
    );
    assert_eq!(payload["data"]["api"], 1);
    let device = payload["data"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|feature| feature["id"] == "rkp-device")
        .unwrap();
    assert_eq!(device["contract"], 2);
}

#[test]
fn usage_errors_are_json_envelopes() {
    let env = env("usage");
    let output = command(&env)
        .args(["tricky-store", "no-such-command"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["code"], "usage_error");
}

#[test]
fn system_artifacts_reports_runtime_paths() {
    let env = env("artifacts");
    let payload = run(&env, &["system", "artifacts"]);
    assert_eq!(payload["command"], "system.artifacts");
    assert!(
        payload["data"]["log_path"]
            .as_str()
            .unwrap()
            .ends_with("duckd.log")
    );
}

#[test]
fn system_info_reads_module_and_kernel_state() {
    let env = env("system-info");
    fs::create_dir_all(env.sysroot.join("proc/sys/kernel")).unwrap();
    fs::write(env.sysroot.join("proc/sys/kernel/osrelease"), "6.1.99\n").unwrap();
    let payload = run(&env, &["system", "info"]);
    assert_eq!(payload["data"]["module"]["id"], "duck-toolbox");
    assert_eq!(payload["data"]["device"]["kernel_release"], "6.1.99");
    assert!(payload["data"]["root_manager"].is_null());
}

#[test]
fn system_uninstall_marks_magisk_module_for_removal() {
    let env = env("uninstall");
    fs::create_dir_all(env.sysroot.join("data/adb/magisk")).unwrap();
    let module_dir = env.sysroot.join("data/adb/modules/duck-toolbox");
    fs::create_dir_all(&module_dir).unwrap();

    let payload = run(&env, &["system", "uninstall"]);
    assert_eq!(payload["data"]["reboot_required"], true);
    assert!(module_dir.join("remove").is_file());
}

#[test]
fn system_open_url_rejects_non_web_schemes() {
    let env = env("open-url");
    let output = command(&env)
        .args(["system", "open-url", "intent://evil"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["error"]["code"], "invalid_url");
}

#[test]
fn describe_summarizes_feature_status() {
    let env = env("describe");
    install_backend(&env, "tricky_store", 246);
    let payload = run(&env, &["describe"]);
    let description = payload["data"]["description"].as_str().unwrap();
    assert!(
        description.contains("(TS): 0 targets, no keybox"),
        "{description}"
    );
    assert_eq!(payload["data"]["applied"], false);
}

#[test]
fn tricky_store_entry_links_the_webroot() {
    let env = env("entry");
    fs::create_dir_all(env.root.join("webroot")).unwrap();
    install_backend(&env, "tricky_store", 246);

    let payload = run(&env, &["tricky-store", "entry", "enable"]);
    assert_eq!(payload["data"]["linked"], true);
    let link = env.sysroot.join("data/adb/modules/tricky_store/webroot");
    assert_eq!(fs::read_link(&link).unwrap(), env.root.join("webroot"));

    run(&env, &["tricky-store", "entry", "remove"]);
    assert!(!link.exists());
}

#[test]
fn command_log_keeps_outcomes_only() {
    let env = env("log");
    run(&env, &["rkp", "profile", "show"]);
    let payload = run(&env, &["system", "log", "--limit", "5"]);
    let entries = payload["data"]["entries"].as_array().unwrap();
    assert_eq!(entries[0]["command"], "rkp.profile.show");
    assert_eq!(entries[0]["ok"], true);
    let raw = fs::read_to_string(env.data.join("var/logs/duckd.log")).unwrap();
    assert!(!raw.contains("\"data\""));
}

#[test]
fn tricky_store_status_without_backend() {
    let env = env("ts-none");
    let payload = run(&env, &["tricky-store", "status", "--json"]);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"], "tricky-store.status");
    assert!(payload["data"]["active"].is_null());
    assert_eq!(payload["data"]["backends"].as_array().unwrap().len(), 0);
}

#[test]
fn tricky_store_keybox_install_rejects_bad_xml() {
    let env = env("ts-bad-keybox");
    install_backend(&env, "tricky_store", 246);
    let source = env.sysroot.join("storage/emulated/0/Download/bad.xml");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "<not-keybox />").unwrap();

    let output = command(&env)
        .args([
            "tricky-store",
            "keybox",
            "install",
            "/storage/emulated/0/Download/bad.xml",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["error"]["code"], "invalid_keybox");
}

#[test]
fn tricky_store_set_aosp_installs_bundled_keybox() {
    let env = env("ts-aosp");
    install_backend(&env, "tricky_store", 246);

    let payload = run(&env, &["tricky-store", "keybox", "set-aosp", "--json"]);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["data"]["summary"]["has_ecdsa"], true);
    assert_eq!(payload["data"]["summary"]["has_rsa"], true);

    let installed = env.sysroot.join("data/adb/tricky_store/keybox.xml");
    assert!(Path::new(&installed).is_file());
}

#[test]
fn tricky_store_keybox_import_accepts_picked_content() {
    let env = env("ts-import");
    install_backend(&env, "tricky_store", 246);
    let aosp = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../duck-tricky-store/assets/aosp-keybox.xml"),
    )
    .unwrap();

    let output = run_json(
        &env,
        &["tricky-store", "keybox", "import", "--stdin-json"],
        &serde_json::json!({ "content": aosp }),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        env.sysroot
            .join("data/adb/tricky_store/keybox.xml")
            .is_file()
    );
}

#[test]
fn system_files_lists_matching_files() {
    let env = env("ts-files");
    let dir = env.sysroot.join("storage/emulated/0/Download");
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("keybox.xml"), "<xml/>").unwrap();
    fs::write(dir.join("notes.txt"), "no").unwrap();

    let payload = run(
        &env,
        &[
            "system",
            "files",
            "--path",
            "/storage/emulated/0/Download",
            "--extension",
            "xml",
        ],
    );
    let names: Vec<&str> = payload["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"nested"));
    assert!(names.contains(&"keybox.xml"));
    assert!(!names.contains(&"notes.txt"));
}
