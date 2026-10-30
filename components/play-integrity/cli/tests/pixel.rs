// SPDX-License-Identifier: GPL-3.0-only
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
fn request(root: &Path, operation: &str, input: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
        .args(["request", operation])
        .env("PIF_ENGINE_DATA", root.join("data"))
        .env("PIF_ENGINE_SYSROOT", root.join("sys"))
        .env("PIF_ENGINE_MODULE", root.join("module"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap()
}
#[test]
fn pixel_preview_and_snapshot_preserve_power_and_report_unknown_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert_eq!(
        request(root, "pixel-devices", json!({}))["data"]["default"],
        "bluejay"
    );
    request(root, "disable", json!({}));
    std::fs::write(root.join("data/pixel-bluejay.json"),json!({"source":"https://flash.android.com/","device":"bluejay","fetched":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),"builds":[{"id":"ID.1","incremental":"123","release":"16","fingerprint":"google/bluejay/bluejay:16/ID.1/123:user/release-keys"}]}).to_string()).unwrap();
    let before = request(root, "config", json!({}));
    let preview = request(root, "pixel-preview", json!({"build":"ID.1"}));
    assert_eq!(preview["ok"], true, "{preview}");
    let candidate = &preview["data"];
    assert_eq!(candidate["entries"]["MODEL"], "Pixel 6a");
    assert!(candidate["entries"].get("SECURITY_PATCH").is_none());
    assert!(candidate["suggested_patch"].is_null());
    let saved = request(
        root,
        "profile-save",
        json!({"name":"Release","content":candidate["json"],"provenance":candidate["provenance"]}),
    );
    assert_eq!(saved["ok"], true, "{saved}");
    assert_eq!(saved["data"]["profiles"][0]["provenance"]["build"], "ID.1");
    assert_eq!(
        saved["data"]["profiles"][0]["provenance"],
        candidate["provenance"]
    );
    for field in [
        "MODEL",
        "DEVICE_INITIAL_SDK_INT",
        "FINGERPRINT",
        "INCREMENTAL",
        "RELEASE",
        "MANUFACTURER",
        "ro.build.fingerprint",
        "ro.product.model",
        "ro.product.first_api_level",
    ] {
        let mut changed = candidate["entries"].clone();
        changed[field] = if field.contains("api_level") || field == "DEVICE_INITIAL_SDK_INT" {
            json!("35")
        } else {
            json!("Changed")
        };
        assert_eq!(
            request(
                root,
                "profile-save",
                json!({"name":"Changed","content":changed.to_string(),"provenance":candidate["provenance"]})
            )["ok"],
            false,
            "{field}"
        );
    }
    for field in [
        "identity_bound",
        "device",
        "build",
        "fingerprint",
        "model",
        "first_api_level",
        "incremental",
    ] {
        let mut changed = candidate["provenance"].clone();
        changed.as_object_mut().unwrap().remove(field);
        assert_eq!(
            request(
                root,
                "profile-save",
                json!({"name":"Incomplete","content":candidate["json"],"provenance":changed})
            )["ok"],
            false,
            "{field}"
        );
    }
    let legacy = request(
        root,
        "profile-save",
        json!({"name":"Legacy","content":candidate["json"],"provenance":{
            "source":"https://flash.android.com/","device":"bluejay","build":"ID.1","fetched":candidate["provenance"]["fetched"],"patch_known":false
        }}),
    );
    assert_eq!(legacy["ok"], true, "{legacy}");
    let legacy = legacy["data"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Legacy")
        .unwrap();
    assert_eq!(legacy["provenance"]["identity_bound"], false);
    assert_eq!(
        request(root, "profile-preview", json!({"id":legacy["id"]}))["ok"],
        true
    );
    assert_eq!(request(root, "config", json!({})), before);
    assert_eq!(request(root, "power", json!({}))["data"]["enabled"], false);
    assert_eq!(
        request(root, "pixel-preview", json!({"build":"other"}))["ok"],
        false
    );
    let cache_path = root.join("data/pixel-bluejay.json");
    let mut cache: Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    cache["builds"][0]["release"] = json!("15");
    std::fs::write(&cache_path, cache.to_string()).unwrap();
    assert_eq!(
        request(root, "pixel-preview", json!({"build":"ID.1"}))["ok"],
        false
    );
    let mut corrupt = candidate["provenance"].clone();
    corrupt["build"] = json!("wrong");
    assert_eq!(
        request(
            root,
            "profile-save",
            json!({"name":"Wrong","content":candidate["json"],"provenance":corrupt})
        )["ok"],
        false
    );
}
#[test]
fn activation_and_sanitized_report_survive_independent_requests() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let p = request(root, "preview", json!({"content":"MODEL=Test"}));
    let mut apply = p["data"].clone();
    apply["content"] = apply["json"].clone();
    apply["format"] = json!("json");
    assert_eq!(request(root, "apply", apply)["ok"], true);
    assert_eq!(
        request(root, "status", json!({}))["data"]["process_restart_required"],
        true
    );
    let revision = request(root, "config", json!({}))["data"]["revision"].clone();
    let result = request(
        root,
        "record-import",
        json!({"revision":revision,"report":{"requestDetails":{"requestPackageName":"example.checker","nonce":"do-not-store"},"deviceIntegrity":{"deviceRecognitionVerdict":["MEETS_DEVICE_INTEGRITY"]},"accountDetails":{"appLicensingVerdict":"UNLICENSED"}}}),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        result["data"]["records"][0]["results"],
        json!(["omitted", "pass", "omitted"])
    );
    assert!(
        !std::fs::read_to_string(root.join("data/test-records.json"))
            .unwrap()
            .contains("do-not-store")
    );
    assert_eq!(
        request(
            root,
            "record-import",
            json!({"revision":"old","report":{"requestDetails":{"requestPackageName":"example.checker"},"deviceIntegrity":{}}})
        )["ok"],
        false
    );
}

#[test]
fn all_listed_devices_preview_their_own_identity_without_applying() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    request(root, "disable", json!({}));
    let before = request(root, "config", json!({}));
    let devices = request(root, "pixel-devices", json!({}));
    let devices = devices["data"]["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 39);
    for device in devices {
        let id = device["id"].as_str().unwrap();
        let fingerprint = format!("google/{id}/{id}:17/TEST.1/123:user/release-keys");
        std::fs::write(root.join(format!("data/pixel-{id}.json")), json!({
            "source":"https://flash.android.com/", "device":id,
            "fetched":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            "builds":[{"id":"TEST.1","incremental":"123","release":"17","fingerprint":fingerprint}]
        }).to_string()).unwrap();
        let result = request(
            root,
            "pixel-preview",
            json!({"device":id,"build":"TEST.1","incremental":"123"}),
        );
        assert_eq!(result["ok"], true, "{id}: {result}");
        let entries = &result["data"]["entries"];
        assert_eq!(entries["MODEL"], device["name"]);
        assert_eq!(entries["DEVICE"], id);
        assert_eq!(entries["FINGERPRINT"], fingerprint);
        assert_eq!(entries["ro.product.model"], device["name"]);
        assert_eq!(
            entries["ro.product.first_api_level"],
            entries["DEVICE_INITIAL_SDK_INT"]
        );
        match id {
            "bluejay" => assert_eq!(entries["DEVICE_INITIAL_SDK_INT"], "32"),
            "stallion" => assert_eq!(entries["DEVICE_INITIAL_SDK_INT"], "36"),
            "yogi" => assert_eq!(entries["DEVICE_INITIAL_SDK_INT"], "37"),
            "tangorpro" | "felix" => assert_eq!(entries["DEVICE_INITIAL_SDK_INT"], "33"),
            "sailfish" => assert_eq!(entries["DEVICE_INITIAL_SDK_INT"], "25"),
            _ => {}
        }
    }
    assert_eq!(request(root, "config", json!({})), before);
    assert_eq!(request(root, "power", json!({}))["data"]["enabled"], false);
}

#[test]
fn duplicate_build_labels_require_the_exact_incremental() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    request(root, "disable", json!({}));
    let cache = root.join("data/pixel-stallion.json");
    std::fs::write(cache, json!({"source":"https://flash.android.com/","device":"stallion",
        "fetched":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        "builds":[
            {"id":"SAME.1","incremental":"123","release":"16","fingerprint":"google/stallion/stallion:16/SAME.1/123:user/release-keys"},
            {"id":"SAME.1","incremental":"456","release":"16","fingerprint":"google/stallion/stallion:16/SAME.1/456:user/release-keys"}
        ]}).to_string()).unwrap();
    for incremental in [Value::Null, json!("789"), json!(456)] {
        let mut input = json!({"device":"stallion","build":"SAME.1"});
        if !incremental.is_null() {
            input["incremental"] = incremental;
        }
        assert_eq!(request(root, "pixel-preview", input)["ok"], false);
    }
    let result = request(
        root,
        "pixel-preview",
        json!({"device":"stallion","build":"SAME.1","incremental":"456"}),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        result["data"]["entries"]["FINGERPRINT"],
        "google/stallion/stallion:16/SAME.1/456:user/release-keys"
    );
}
