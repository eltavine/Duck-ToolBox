use std::{collections::BTreeMap, fs};

use duck_core::Sysroot;

use super::{State, Vendor, detect};

fn properties(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect()
}

fn state(values: &[(&str, &str)]) -> State {
    let root = tempfile::tempdir().unwrap();
    detect(&properties(values), &Sysroot::new(root.path())).state
}

#[test]
fn recognizes_qualcomm_aliases_without_substring_false_positives() {
    for alias in [
        "QTI",
        " qti\n",
        "QCOM",
        "Qualcomm",
        "QUALCOMM Incorporated",
        "Qualcomm Technologies, Inc.",
        "Qualcomm Technologies Ltd.",
    ] {
        assert_eq!(
            state(&[("ro.soc.manufacturer", alias)]),
            State::Qualcomm,
            "{alias}"
        );
    }
    for unknown in [
        "",
        " ",
        "unknown",
        "n/a",
        "not-qti",
        "qti-other",
        "not Qualcomm",
        "Qualcommish",
        "Qualcomm clone",
    ] {
        assert_eq!(
            state(&[("ro.soc.manufacturer", unknown)]),
            State::Unknown,
            "{unknown}"
        );
    }
}

#[test]
fn known_other_vendors_are_distinct_from_unknown_values() {
    for vendor in [
        "MediaTek",
        "MTK",
        "Samsung Electronics Co., Ltd.",
        "Google",
        "HiSilicon",
        "UNISOC",
        "Spreadtrum",
        "Rockchip",
        "NVIDIA",
    ] {
        assert_eq!(
            state(&[("ro.soc.manufacturer", vendor)]),
            State::Other,
            "{vendor}"
        );
    }
    assert_eq!(
        state(&[("ro.soc.manufacturer", "unlisted vendor")]),
        State::Unknown
    );
}

#[test]
fn falls_back_to_vendor_properties_and_hardware_identifiers() {
    for key in [
        "ro.vendor.soc.manufacturer",
        "ro.hardware",
        "ro.boot.hardware",
    ] {
        assert_eq!(state(&[(key, "qcom")]), State::Qualcomm, "{key}");
    }
    assert_eq!(
        state(&[("ro.soc.manufacturer", "unknown"), ("ro.hardware", "qcom")]),
        State::Qualcomm
    );
    assert_eq!(state(&[("ro.hardware", "not-qcom")]), State::Unknown);
}

#[test]
fn recognizes_bounded_soc_part_numbers_in_soc_fields() {
    for model in [
        "SM8650",
        "SM8650-AB",
        "SM8650Q-AB",
        "sm8550",
        "MSM8998",
        "APQ8084",
        "SDM660",
        "QCM6490",
        "QCS8550",
    ] {
        assert_eq!(
            state(&[("ro.soc.model", model)]),
            State::Qualcomm,
            "{model}"
        );
    }
    for model in [
        "SM-S928B",
        "SM8650 clone",
        "SM865000",
        "SM8",
        "fake-SM8650",
        "SM8650-TOOLONG",
        "SM8650未知",
    ] {
        assert_eq!(state(&[("ro.soc.model", model)]), State::Unknown, "{model}");
    }
    assert_eq!(
        state(&[("ro.vendor.qti.soc_model", "SM8650")]),
        State::Qualcomm
    );
}

#[test]
fn phone_brand_product_model_and_transport_files_do_not_identify_the_soc() {
    let root = tempfile::tempdir().unwrap();
    for path in ["vendor/lib64/libkeymasterdeviceutils.so", "dev/smcinvoke"] {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "present").unwrap();
    }
    let result = detect(
        &properties(&[
            ("ro.product.manufacturer", "Qualcomm"),
            ("ro.product.model", "SM8650"),
        ]),
        &Sysroot::new(root.path()),
    );
    assert_eq!(result.state, State::Unknown);
    assert_eq!(result.blocker(), Some("soc_vendor_unknown"));
}

#[test]
fn kernel_soc_family_is_a_read_only_fallback_with_a_bounded_read() {
    let root = tempfile::tempdir().unwrap();
    let family = root.path().join("sys/devices/soc0/family");
    fs::create_dir_all(family.parent().unwrap()).unwrap();
    fs::write(&family, "Snapdragon\n").unwrap();
    let result = detect(&properties(&[]), &Sysroot::new(root.path()));
    assert_eq!(result.state, State::Qualcomm);
    assert_eq!(result.evidence[0].source, "/sys/devices/soc0/family");
    assert_eq!(result.evidence[0].value, "Snapdragon");
    assert_eq!(result.evidence[0].vendor, Some(Vendor::Qualcomm));
    assert_eq!(fs::read_to_string(&family).unwrap(), "Snapdragon\n");
    for invalid in [vec![b'x'; 257], vec![0xff; 10]] {
        fs::write(&family, invalid).unwrap();
        assert_eq!(
            detect(&properties(&[]), &Sysroot::new(root.path())).state,
            State::Unknown
        );
    }
}

#[test]
fn conflicting_sources_are_reported_instead_of_choosing_the_first() {
    let root = tempfile::tempdir().unwrap();
    let values = properties(&[
        ("ro.soc.manufacturer", "MediaTek"),
        ("ro.soc.model", "SM8650"),
        ("ro.hardware", "qcom"),
    ]);
    let result = detect(&values, &Sysroot::new(root.path()));
    assert_eq!(result.state, State::Conflicting);
    assert_eq!(result.model, "SM8650");
    assert_eq!(result.evidence.len(), 3);
    assert_eq!(result.blocker(), Some("soc_vendor_conflict"));
    assert_eq!(
        detect(
            &properties(&[("ro.soc.manufacturer", "QTI")]),
            &Sysroot::new(root.path())
        )
        .blocker(),
        None
    );
    assert_eq!(
        detect(
            &properties(&[("ro.soc.manufacturer", "MediaTek")]),
            &Sysroot::new(root.path())
        )
        .blocker(),
        Some("qualcomm_soc_required")
    );
}

#[test]
fn pjx110_properties_and_kernel_snapshot_identify_qualcomm_consistently() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("sys/devices/soc0");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("family"), "Snapdragon\n").unwrap();
    fs::write(directory.join("machine"), "PINEAPPLE\n").unwrap();
    let values = properties(&[
        ("ro.soc.manufacturer", "QTI"),
        ("ro.soc.model", "SM8650"),
        ("ro.vendor.qti.soc_model", "SM8650"),
        ("ro.hardware", "qcom"),
        ("ro.boot.hardware", "qcom"),
        ("ro.board.platform", "pineapple"),
        ("ro.product.board", "pineapple"),
        ("ro.product.manufacturer", "OnePlus"),
    ]);
    let result = detect(&values, &Sysroot::new(root.path()));
    assert_eq!(result.state, State::Qualcomm);
    assert_eq!(result.model, "SM8650");
    assert_eq!(result.evidence.len(), 9);
    assert_eq!(result.blocker(), None);
}
