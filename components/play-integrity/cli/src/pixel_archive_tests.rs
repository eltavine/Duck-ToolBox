// SPDX-License-Identifier: GPL-3.0-only
use super::*;
#[test]
fn commit_feed_accepts_only_the_first_entry_commit_id() {
    let first = "a".repeat(40);
    let entry = format!("<entry><id>tag:github.com,2008:Grit::Commit/{first}</id></entry>");
    assert_eq!(
        feed_revision(&format!(
            "<feed><id>repository</id>{entry}<entry><id>later</id></entry></feed>"
        ))
        .unwrap(),
        first
    );
    for invalid in [
        String::new(),
        entry.replace(&first, "main"),
        entry.replace(&first, "../escape"),
        format!("<entry><id>bad</id></entry>{entry}"),
    ] {
        assert!(feed_revision(&invalid).is_err());
    }
}
fn fixture(channel: &str) -> (tempfile::TempDir, Engine, Catalog, String) {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine {
        data: root.path().join("data"),
        module: root.path().join("module"),
        sysroot: root.path().join("sys"),
    };
    let product = if channel == "release" {
        "bluejay"
    } else {
        "bluejay_beta"
    };
    let release = if channel == "canary" { "CANARY" } else { "17" };
    let suffix = if channel == "release" {
        String::new()
    } else {
        format!("_{channel}")
    };
    let digest = "a".repeat(64);
    let index: Index = serde_json::from_value(json!({"generated":"2026-10-30T00:00:00Z","devices":{"bluejay":{
        "model":"Pixel 6a","first_api_level":32,"channels":{channel:{"latest":format!("bluejay/bluejay{suffix}_123.prop"),
        "build":"ID.1","incremental":"123","security_patch":"2026-10-05","android":"17","vbmeta_digest":digest}}}}})).unwrap();
    let catalog = Catalog {
        source: SOURCE.into(),
        source_revision: "b".repeat(40),
        fetched: now().unwrap(),
        index,
    };
    let raw = format!(
        "ro.product.build.fingerprint=google/{product}/bluejay:{release}/ID.1/123:user/release-keys\n\
        ro.system.build.fingerprint=google/generic_system_google/generic:{release}/ID.1/123:user/release-keys\n\
        ro.product.product.name={product}\nro.product.product.brand=google\nro.product.product.manufacturer=Google\n\
        ro.product.product.device=bluejay\nro.product.product.model=Pixel 6a\nro.product.first_api_level=32\n\
        ro.build.id=ID.1\nro.product.build.id=ID.1\nro.build.version.incremental=123\nro.product.build.version.incremental=123\n\
        ro.product.build.version.release_or_codename={release}\nro.product.build.version.release=17\nro.build.version.release=17\n\
        ro.build.version.security_patch=2026-10-05\nro.boot.vbmeta.digest={digest}\nro.vendor.build.security_patch=2026-10-05\n\
        ro.hardware.egl=mali\nro.debuggable=1\nspoofProvider=1\nMODEL=Untrusted override\n"
    );
    (root, engine, catalog, raw)
}
#[test]
fn archive_candidates_use_product_identity_and_preserve_power_and_configuration() {
    for channel in CHANNELS {
        let (_root, engine, catalog, raw) = fixture(channel);
        validate_index(&catalog.index).unwrap();
        files::atomic(&engine.data.join("disabled"), b"test-stop").unwrap();
        files::atomic(
            &engine.data.join("custom.pif.json"),
            br#"{"MODEL":"Existing","spoofProvider":"0","spoofProps":"0"}"#,
        )
        .unwrap();
        let before = config::current(&engine).unwrap();
        let result = candidate(&engine, &catalog, "bluejay", channel, &raw).unwrap();
        assert_eq!(result["entries"]["MODEL"], "Pixel 6a");
        assert_eq!(result["entries"]["DEVICE_INITIAL_SDK_INT"], "32");
        assert_eq!(result["entries"]["SECURITY_PATCH"], "2026-10-05");
        assert_eq!(result["entries"]["spoofProps"], "0");
        assert_eq!(result["entries"]["spoofProvider"], "0");
        for key in [
            "ro.boot.vbmeta.digest",
            "ro.hardware.egl",
            "ro.debuggable",
            "SDK_INT",
        ] {
            assert!(result["entries"].get(key).is_none(), "{key}");
        }
        assert_eq!(config::current(&engine).unwrap(), before);
        assert!(crate::power::disabled(&engine));
        let saved = crate::library::save_profile(
            &engine,
            &json!({"name":"Archive","content":result["json"],"provenance":result["provenance"]}),
        )
        .unwrap();
        assert_eq!(saved["profiles"][0]["provenance"], result["provenance"]);
        let restored =
            crate::library::preview_profile(&engine, &json!({"id":saved["profiles"][0]["id"]}))
                .unwrap();
        assert_eq!(restored["entries"], result["entries"]);
        assert_eq!(restored["provenance"], result["provenance"]);
    }
}
#[test]
fn mismatched_identity_and_duplicate_properties_fail_closed() {
    let (_root, engine, catalog, raw) = fixture("canary");
    for invalid in [
        raw.replace("google/bluejay_beta/bluejay:", "google/bluejay_beta/lynx:"),
        raw.replace("/ID.1/123:", "/ID.1/124:"),
        raw.replace(
            "ro.build.version.security_patch=2026-10-05",
            "ro.build.version.security_patch=2026-09-05",
        ),
        raw.replace(
            "ro.product.first_api_level=32",
            "ro.product.first_api_level=35",
        ),
        raw.replace(
            "ro.product.product.model=Pixel 6a",
            "ro.product.product.model=Pixel 7a",
        ),
        format!("{raw}ro.product.product.device=bluejay\n"),
    ] {
        assert!(candidate(&engine, &catalog, "bluejay", "canary", &invalid).is_err());
    }
    assert!(!engine.data.join("custom.pif.json").exists());
}
#[test]
fn malformed_catalog_paths_dates_and_revisions_are_rejected() {
    let (_root, engine, catalog, _) = fixture("release");
    let value = serde_json::to_value(&catalog).unwrap();
    for invalid in [
        "",
        "2026-02-29T00:00:00Z",
        "2026-10-30T24:00:00Z",
        "2026-10-30T00:00:60Z",
        "2026-10-30T00:00:00+08:00",
        "😀😀😀😀😀",
    ] {
        assert!(!generated_date(invalid), "{invalid}");
    }
    for (field, invalid) in [
        ("latest", "../../other.prop"),
        ("security_patch", "2026-02-29"),
        ("incremental", "123/other"),
        ("vbmeta_digest", "not-a-hash"),
    ] {
        let mut altered = value.clone();
        altered["devices"]["bluejay"]["channels"]["release"][field] = json!(invalid);
        assert!(
            validate_index(&serde_json::from_value::<Catalog>(altered).unwrap().index).is_err()
        );
    }
    let input = json!({"source_revision":catalog.source_revision});
    files::write_json(&engine.data.join(CACHE), &value).unwrap();
    assert!(cached(&engine, &input).is_ok());
    assert!(cached(&engine, &json!({"source_revision":"c".repeat(40)})).is_err());
    for time in [now().unwrap() - 86401, now().unwrap() + 600] {
        let mut altered = value.clone();
        altered["fetched"] = json!(time);
        files::write_json(&engine.data.join(CACHE), &altered).unwrap();
        assert!(cached(&engine, &input).is_err());
    }
}
#[test]
fn vendor_patch_is_independent_and_provenance_cannot_be_attached_to_a_different_build() {
    let (_root, engine, catalog, raw) = fixture("release");
    let raw = raw.replace(
        "ro.vendor.build.security_patch=2026-10-05",
        "ro.vendor.build.security_patch=2026-09-05",
    );
    let candidate = candidate(&engine, &catalog, "bluejay", "release", &raw).unwrap();
    assert!(candidate["suggested_patch"].is_null());
    assert!(
        candidate["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "patch_scope")
    );
    let boot_raw = raw.replace(
        "ro.vendor.build.security_patch=2026-09-05",
        "ro.vendor.build.security_patch=2026-10-05\nro.vendor.boot_security_patch=2026-09-05",
    );
    let boot_candidate =
        super::candidate(&engine, &catalog, "bluejay", "release", &boot_raw).unwrap();
    assert_eq!(
        boot_candidate["entries"]["ro.vendor.boot_security_patch"],
        "2026-09-05"
    );
    assert!(boot_candidate["suggested_patch"].is_null());
    let entries = config::parse(candidate["json"].as_str().unwrap(), "json").unwrap();
    for (key, invalid) in [
        ("device", "lynx"),
        ("build", "OTHER"),
        ("incremental", "124"),
        ("security_patch", "2026-09-05"),
        ("channel", "unknown"),
        ("source_revision", "main"),
    ] {
        let mut value = candidate["provenance"].clone();
        value[key] = json!(invalid);
        assert!(provenance(&value, &entries).is_err());
    }
}

#[test]
fn archive_snapshots_reject_changed_identity_even_when_build_labels_still_match() {
    let (_root, engine, catalog, raw) = fixture("release");
    let candidate = candidate(&engine, &catalog, "bluejay", "release", &raw).unwrap();
    let original = config::parse(candidate["json"].as_str().unwrap(), "json").unwrap();
    for (field, replacement) in [
        (
            "FINGERPRINT",
            "google/bluejay/bluejay:16/ID.1/123:user/release-keys",
        ),
        ("MODEL", "Pixel 7a"),
        ("DEVICE_INITIAL_SDK_INT", "33"),
        ("ro.product.model", "Pixel 7a"),
    ] {
        let mut entries = original.clone();
        entries.insert(field.into(), replacement.into());
        assert!(
            provenance(&candidate["provenance"], &entries).is_err(),
            "{field}"
        );
    }
}

#[test]
fn sha512_boot_digest_remains_reference_data() {
    let (_root, engine, mut catalog, raw) = fixture("release");
    let digest = "c".repeat(128);
    catalog
        .index
        .devices
        .get_mut("bluejay")
        .unwrap()
        .channels
        .get_mut("release")
        .unwrap()
        .vbmeta_digest = digest.clone();
    validate_index(&catalog.index).unwrap();
    let candidate = candidate(
        &engine,
        &catalog,
        "bluejay",
        "release",
        &raw.replace(&"a".repeat(64), &digest),
    )
    .unwrap();
    let entries = config::parse(candidate["json"].as_str().unwrap(), "json").unwrap();
    assert_eq!(
        provenance(&candidate["provenance"], &entries).unwrap()["vbmeta_digest"],
        digest
    );
    assert!(!entries.contains_key("ro.boot.vbmeta.digest"));
}
