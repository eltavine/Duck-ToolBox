use super::{
    DEFAULT_DICE_ISSUER, DEFAULT_DICE_SUBJECT, DEFAULT_GENERIC_BOOT_PATCH_LEVEL,
    DEFAULT_GENERIC_FINGERPRINT, DEFAULT_GENERIC_SYSTEM_PATCH_LEVEL,
    DEFAULT_GENERIC_VENDOR_PATCH_LEVEL, DeviceInfo, DiceCurve, KeySource, ProfileData,
    normalize::{normalize_output_path, normalize_profile, normalize_server_url},
};

#[test]
fn default_profile_uses_generic_rkp_defaults() {
    let profile = ProfileData::default();
    assert!(matches!(profile.key_source, KeySource::Unset));
    assert_eq!(profile.curve, DiceCurve::Ed25519);
    assert_eq!(profile.device, DeviceInfo::default());
    assert_eq!(
        profile.device.system_patch_level,
        DEFAULT_GENERIC_SYSTEM_PATCH_LEVEL
    );
    assert_eq!(profile.fingerprint.value, DEFAULT_GENERIC_FINGERPRINT);
    assert!(!profile.fingerprint.include_fingerprint);
    assert_eq!(profile.num_keys, 1);
}

#[test]
fn fingerprint_policy_survives_storage_and_explicit_overrides_without_changing_defaults() {
    use super::{RunOverrides, resolve_profile, save_profile, show_profile};
    let directory = tempfile::tempdir().unwrap();
    let paths = duck_core::AppPaths::for_root(directory.path(), directory.path());
    let legacy: super::FingerprintConfig =
        serde_json::from_str(r#"{"value":"original/build"}"#).unwrap();
    assert!(!legacy.include_fingerprint);
    let mut profile = ProfileData::default();
    profile.fingerprint.include_fingerprint = true;
    save_profile(&paths, None, &profile).unwrap();
    assert!(
        show_profile(&paths, None)
            .unwrap()
            .fingerprint
            .include_fingerprint
    );
    let resolved = resolve_profile(
        &paths,
        &RunOverrides {
            include_fingerprint: Some(false),
            ..RunOverrides::default()
        },
    )
    .unwrap();
    assert!(!resolved.profile.fingerprint.include_fingerprint);
    assert!(
        show_profile(&paths, None)
            .unwrap()
            .fingerprint
            .include_fingerprint
    );
}

#[test]
fn normalize_server_url_adds_default_version_path() {
    assert_eq!(
        normalize_server_url("https://remoteprovisioning.googleapis.com").unwrap(),
        "https://remoteprovisioning.googleapis.com/v1",
    );
}

#[test]
fn normalize_server_url_trims_trailing_slash() {
    assert_eq!(
        normalize_server_url("https://remoteprovisioning.googleapis.com/v1/").unwrap(),
        "https://remoteprovisioning.googleapis.com/v1",
    );
}

#[test]
fn normalize_profile_coerces_blank_hw_key_to_unset() {
    let profile = normalize_profile(ProfileData {
        key_source: KeySource::HwKey {
            hw_key_hex: "   ".into(),
            kdf_label: "rkp_bcc_km".into(),
        },
        ..ProfileData::default()
    })
    .unwrap();

    assert!(matches!(profile.key_source, KeySource::Unset));
}

#[test]
fn normalize_profile_keeps_patch_levels_as_provided() {
    let profile = normalize_profile(ProfileData {
        device: DeviceInfo {
            system_patch_level: 202601,
            ..DeviceInfo::default()
        },
        ..ProfileData::default()
    })
    .unwrap();

    assert_eq!(
        profile.device.boot_patch_level,
        DEFAULT_GENERIC_BOOT_PATCH_LEVEL
    );
    assert_eq!(profile.device.system_patch_level, 202601);
    assert_eq!(
        profile.device.vendor_patch_level,
        DEFAULT_GENERIC_VENDOR_PATCH_LEVEL
    );
}

#[test]
fn normalize_profile_applies_default_dice_names() {
    let profile = normalize_profile(ProfileData::default()).unwrap();

    assert_eq!(profile.device.dice_issuer, DEFAULT_DICE_ISSUER);
    assert_eq!(profile.device.dice_subject, DEFAULT_DICE_SUBJECT);
}

#[test]
fn normalize_output_path_rejects_module_escape() {
    let error = normalize_output_path("../outside.xml").unwrap_err();
    assert!(error.to_string().contains("output path"));
}

#[test]
fn normalize_output_path_normalizes_windows_separators() {
    assert_eq!(
        normalize_output_path(r"var\outputs\keybox.xml").unwrap(),
        "var/outputs/keybox.xml",
    );
}
