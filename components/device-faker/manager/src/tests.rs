use super::*;

fn fixture() -> (tempfile::TempDir, Paths) {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        root: root.path().into(),
        data: root.path().join("data/adb/duck-device-faker"),
    };
    (root, paths)
}
#[test]
fn previews_preserve_multi_user_priority_and_flag_overlaps() {
    let content = r#"
[templates.first]
packages = ["com.example.app", "com.example.app@999"]
model = "First"
[templates.second]
packages = ["com.example.app"]
model = "Second"
[[apps]]
package = "com.example.app@999"
brand = "Direct"
"#;
    let parsed = crate::config::Config::from_toml(content).unwrap();
    let merged = parsed.get_merged_config("com.example.app@999").unwrap();
    assert_eq!(merged.brand.as_deref(), Some("Direct"));
    assert!(merged.model.is_none());
    assert_eq!(
        parsed
            .get_merged_config("com.example.app")
            .unwrap()
            .model
            .as_deref(),
        Some("First")
    );
    let summary = inspect(content).unwrap();
    assert_eq!(summary["warnings"].as_array().unwrap().len(), 2);
    assert!(!summary["global_effects"].as_bool().unwrap());
}
#[test]
fn validation_rejects_invalid_targets_and_density_and_reports_global_changes() {
    for content in [
        "[[apps]]\nmodel='No package'",
        "[[apps]]\npackage='com.app@x'",
        "[[apps]]\npackage='com.app'\ndpi=1",
        "[[apps]]\npackage='com.app'\nsdk_int=4294967295",
        "[[apps]]\npackage='com.app'\nsdk_int=0",
    ] {
        assert!(inspect(content).is_err());
    }
    assert_eq!(
        inspect("[[apps]]\npackage='com.app'\ncompanion_resetprop=true").unwrap()["global_effects"],
        true
    );
    assert!(inspect(&"x".repeat(MAX_CONFIG + 1)).is_err());
}
#[test]
fn disabling_invalidates_a_stale_enable_without_taking_the_write_lock() {
    let (_root, paths) = fixture();
    let _lock = paths.lock().unwrap();
    write(&paths.file("enabled"), "initial").unwrap();
    assert!(paths.enabled().unwrap());
    let stopped = stop(&paths).unwrap();
    assert_ne!(stopped["stop_revision"], "initial");
    assert!(!paths.enabled().unwrap());
    write(&paths.file("enabled"), "initial").unwrap();
    assert!(!paths.enabled().unwrap());
    write(&paths.file("config/config.toml"), "invalid[").unwrap();
    assert!(snapshot(&paths.content().unwrap())["error"].is_string());
    assert!(stop(&paths).is_ok());
}
#[test]
fn atomic_config_writes_are_private_and_stale_revisions_fail() {
    let (_root, paths) = fixture();
    write(&paths.file("config/config.toml"), EMPTY).unwrap();
    assert!(matches_revision(&json!({"revision":"stale"}), EMPTY).is_err());
    assert!(matches_revision(&json!({"revision":revision(EMPTY)}), EMPTY).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(paths.file("config/config.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
