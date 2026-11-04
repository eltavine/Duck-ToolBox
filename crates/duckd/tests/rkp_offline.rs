use std::{fs, path::Path, process::Command};

use duck_core::AppPaths;
use duck_rkp::{
    profile::{FingerprintConfig, KeySource, ProfileData, save_profile},
    verify::verify_csr,
};
use serde_json::Value;

fn fixture(fingerprint: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("bin")).unwrap();
    fs::write(root.join("module.prop"), "id=duck-toolbox\n").unwrap();
    let profile = ProfileData {
        key_source: KeySource::Seed {
            seed_hex: "31".repeat(32),
        },
        fingerprint: FingerprintConfig {
            value: fingerprint.into(),
            include_fingerprint: false,
        },
        server_url: "http://127.0.0.1:9/v1".into(),
        ..ProfileData::default()
    };
    save_profile(&AppPaths::for_root(root, root), None, &profile).unwrap();
    directory
}

fn run(root: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_duckd"))
        .env("DUCK_TOOLBOX_ROOT", root)
        .env("DUCK_TOOLBOX_DATA_ROOT", root)
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn offline_cli_accepts_incomplete_online_context_and_honors_requested_key_count() {
    let directory = fixture("");
    for endpoint in ["", "not-an-endpoint"] {
        let (success, result) = run(
            directory.path(),
            &[
                "rkp",
                "provision",
                "--offline",
                "--num-keys",
                "3",
                "--server-url",
                endpoint,
            ],
        );
        assert!(success, "{result}");
        assert_eq!(result["data"]["diagnostic_only"], true);
        assert_eq!(result["data"]["local_verify"]["keys_to_sign"], 3);
        let bytes = fs::read(result["data"]["csr_path"].as_str().unwrap()).unwrap();
        let report = verify_csr(&bytes).unwrap();
        assert!(report.signature_valid);
        assert_eq!(report.keys_to_sign, 3);
    }
    let (success, result) = run(directory.path(), &["rkp", "provision"]);
    assert!(!success);
    assert_eq!(result["error"]["code"], "missing_fingerprint");
}

#[test]
fn offline_cli_rejects_excessive_key_counts_without_output() {
    let directory = fixture("synthetic/device/fingerprint");
    let paths = AppPaths::for_root(directory.path(), directory.path());
    let (success, result) = run(
        directory.path(),
        &["rkp", "provision", "--offline", "--num-keys", "17"],
    );
    assert!(!success, "{result}");
    assert_eq!(result["ok"], false);
    assert_eq!(fs::read_dir(paths.outputs_dir).unwrap().count(), 0);
}

#[test]
fn offline_cli_emits_the_selected_fingerprint_metadata_without_network_access() {
    let directory = fixture("synthetic/source/build");
    for (flag, expected) in [("true", Some("synthetic/source/build")), ("false", None)] {
        let (success, result) = run(
            directory.path(),
            &[
                "rkp",
                "provision",
                "--offline",
                "--include-fingerprint",
                flag,
            ],
        );
        assert!(success, "{result}");
        let csr = fs::read(result["data"]["csr_path"].as_str().unwrap()).unwrap();
        assert_eq!(
            verify_csr(&csr).unwrap().unverified_fingerprint.as_deref(),
            expected
        );
    }
    let (success, result) = run(
        directory.path(),
        &[
            "rkp",
            "provision",
            "--offline",
            "--include-fingerprint",
            "invalid",
        ],
    );
    assert!(!success);
    assert_eq!(result["error"]["code"], "usage_error");
}
