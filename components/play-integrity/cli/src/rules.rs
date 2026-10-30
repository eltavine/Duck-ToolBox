// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: non-mutating configuration consistency diagnostics.
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const BUILD_PROPERTIES: &[(&str, &str)] = &[
    ("BRAND", "ro.product.brand"),
    ("MANUFACTURER", "ro.product.manufacturer"),
    ("MODEL", "ro.product.model"),
    ("DEVICE", "ro.product.device"),
    ("PRODUCT", "ro.product.name"),
    ("FINGERPRINT", "ro.build.fingerprint"),
    ("ID", "ro.build.id"),
    ("RELEASE", "ro.build.version.release"),
    ("INCREMENTAL", "ro.build.version.incremental"),
    ("TYPE", "ro.build.type"),
    ("TAGS", "ro.build.tags"),
    ("SECURITY_PATCH", "ro.build.version.security_patch"),
    ("DEVICE_INITIAL_SDK_INT", "ro.product.first_api_level"),
];

// Match PropertyHook's exact-key precedence and ordered leading-* suffix rules.
fn property<'a>(entries: &'a BTreeMap<String, String>, name: &str) -> Option<(&'a str, &'a str)> {
    entries
        .get_key_value(name)
        .filter(|(_, value)| !value.is_empty())
        .or_else(|| {
            entries.iter().find(|(key, value)| {
                !value.is_empty()
                    && key
                        .strip_prefix('*')
                        .is_some_and(|suffix| name.ends_with(suffix))
            })
        })
        .map(|(key, value)| (key.as_str(), value.as_str()))
}

fn system_patch(entries: &BTreeMap<String, String>) -> Option<&str> {
    let build = (entries.get("spoofBuild").is_none_or(|v| v != "0"))
        .then(|| entries.get("SECURITY_PATCH").map(String::as_str))
        .flatten();
    let prop = (entries.get("spoofProps").is_none_or(|v| v != "0"))
        .then(|| property(entries, "ro.build.version.security_patch").map(|(_, value)| value))
        .flatten();
    if matches!((build, prop), (Some(a), Some(b)) if a != b) {
        return None;
    }
    build.or(prop).filter(|value| valid_patch_date(value))
}

fn partition_conflicts<'a>(entries: &'a BTreeMap<String, String>, date: &str) -> Vec<&'a str> {
    if entries.get("spoofProps").is_some_and(|value| value == "0") {
        return Vec::new();
    }
    // OMK resolves vendor/boot patch properties independently from the system patch.
    [
        "ro.vendor.build.security_patch",
        "ro.vendor.boot_security_patch",
    ]
    .into_iter()
    .filter_map(|name| property(entries, name))
    .filter(|(_, value)| *value != date)
    .map(|(key, _)| key)
    .collect()
}

pub fn suggested_patch(entries: &BTreeMap<String, String>) -> Option<&str> {
    system_patch(entries).filter(|date| partition_conflicts(entries, date).is_empty())
}

pub fn valid_patch_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..].parse().unwrap_or(0);
    let days = match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}

pub fn inspect(entries: &BTreeMap<String, String>) -> Vec<Value> {
    let mut warnings = Vec::new();
    if entries
        .get("spoofVendingSdk")
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|s| s > 0)
        && entries
            .get("spoofVendingFinger")
            .is_some_and(|s| !s.is_empty() && s != "0")
    {
        warnings.push(
            json!({"code":"vending_priority","fields":["spoofVendingSdk","spoofVendingFinger"]}),
        );
    }
    if let Some(date) = system_patch(entries) {
        let mut fields = partition_conflicts(entries, date);
        fields.sort_unstable();
        fields.dedup();
        if !fields.is_empty() {
            warnings.push(json!({"code":"patch_scope", "fields":fields}));
        }
    }
    for key in ["DEVICE_INITIAL_SDK_INT", "ro.board.first_api_level"] {
        if entries.get(key).is_none_or(String::is_empty) {
            warnings.push(json!({"code":"missing_api","fields":[key]}));
        }
    }
    // Build.java reads these properties into separate static fields. A process-local
    // field override does not also replace a later SystemProperties read.
    for &(field, property) in BUILD_PROPERTIES {
        if entries.get("spoofBuild").is_some_and(|v| v == "0")
            || entries.get("spoofProps").is_some_and(|v| v == "0")
        {
            continue;
        }
        if let (Some(a), Some((key, b))) = (entries.get(field), self::property(entries, property))
            && !b.is_empty()
            && a != b
        {
            warnings.push(json!({"code":"mismatch","fields":[field,key]}));
        }
    }
    if entries
        .get("spoofProvider")
        .is_none_or(|value| value.is_empty() || value == "1")
    {
        warnings.push(json!({"code":"provider","fields":["spoofProvider"]}));
    }
    for key in entries.keys().filter(|key| key.contains('*')) {
        if !key.starts_with('*') || key[1..].contains('*') {
            warnings.push(json!({"code":"wildcard","fields":[key]}));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_do_not_rewrite_expert_configuration() {
        let entries = crate::config::parse(
            "MODEL=A\nro.product.model=B\nro.*.name=x\nspoofProvider=1",
            "prop",
        )
        .unwrap();
        let before = entries.clone();
        let warnings = inspect(&entries);
        for code in ["missing_api", "mismatch", "provider", "wildcard"] {
            assert!(warnings.iter().any(|item| item["code"] == code));
        }
        assert_eq!(entries, before);
    }

    #[test]
    fn patch_link_uses_only_enabled_unambiguous_calendar_dates() {
        for (config, expected) in [
            ("SECURITY_PATCH=2026-10-05", Some("2026-10-05")),
            (
                "SECURITY_PATCH=2026-10-05\n*.security_patch=2026-09-05",
                None,
            ),
            (
                "SECURITY_PATCH=2026-10-05\n*.security_patch=2026-09-05\nro.build.version.security_patch=2026-10-05",
                None,
            ),
            ("SECURITY_PATCH=2026-10-05\nspoofBuild=0", None),
            ("*.security_patch=2026-09-05\nspoofProps=0", None),
            (
                "SECURITY_PATCH=2026-10-05\nspoofBuild=0\n*.security_patch=2026-09-05",
                Some("2026-09-05"),
            ),
            ("SECURITY_PATCH=2026-02-29", None),
            ("SECURITY_PATCH=2024-02-29", Some("2024-02-29")),
            ("SECURITY_PATCH=unknown", None),
            (
                "SECURITY_PATCH=\nro.build.version.security_patch=2026-10-05",
                None,
            ),
        ] {
            let entries = crate::config::parse(config, "prop").unwrap();
            assert_eq!(suggested_patch(&entries), expected, "{config}");
        }
    }

    #[test]
    fn consistency_checks_follow_property_hook_precedence_and_switches() {
        let mut entries = crate::config::parse(
            "SECURITY_PATCH=2026-10-05\n*.security_patch=2026-09-05",
            "prop",
        )
        .unwrap();
        assert!(inspect(&entries).iter().any(|w| w["code"] == "mismatch"));
        entries.insert(
            "ro.build.version.security_patch".into(),
            "2026-10-05".into(),
        );
        assert!(!inspect(&entries).iter().any(|w| w["code"] == "mismatch"));
        entries.insert("spoofProps".into(), "0".into());
        entries.remove("ro.build.version.security_patch");
        assert!(!inspect(&entries).iter().any(|w| w["code"] == "mismatch"));
    }
    #[test]
    fn partition_specific_patch_overrides_are_not_replaced_by_a_single_date() {
        for property in [
            "ro.vendor.build.security_patch",
            "ro.vendor.boot_security_patch",
            "*security_patch",
        ] {
            let config = format!(
                "SECURITY_PATCH=2026-10-05\nro.build.version.security_patch=2026-10-05\n{property}=2026-09-05"
            );
            let mut entries = crate::config::parse(&config, "prop").unwrap();
            let before = entries.clone();
            assert_eq!(suggested_patch(&entries), None, "{property}");
            assert!(
                inspect(&entries)
                    .iter()
                    .any(|warning| warning["code"] == "patch_scope")
            );
            assert_eq!(entries, before);
            entries.insert("spoofProps".into(), "0".into());
            assert_eq!(suggested_patch(&entries), Some("2026-10-05"));
        }
        let entries = crate::config::parse("SECURITY_PATCH=2026-10-05\n*security_patch=2026-09-05\nro.build.version.security_patch=2026-10-05\nro.vendor.build.security_patch=2026-10-05\nro.vendor.boot_security_patch=2026-10-05", "prop").unwrap();
        assert_eq!(suggested_patch(&entries), Some("2026-10-05"));
    }
}
