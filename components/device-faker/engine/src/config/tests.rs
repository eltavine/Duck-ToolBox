use super::*;

#[test]
fn canonical_custom_properties_update_aliases_before_precise_overrides() {
    let body = "[[apps]]\npackage='com.example.app'\nproduct='Base'\nmarketname='Base'\nsecurity_patch='Base'\n[apps.custom_props]\n'ro.product.name'='Custom product'\n'ro.product.marketname'='Custom market'\n'ro.build.version.security_patch'='Custom patch'\n";
    let aliases = [
        ("ro.build.product", "Custom product"),
        ("ro.vendor.oplus.market.name", "Custom market"),
        ("ro.vendor.build.security_patch", "Custom patch"),
    ];
    for precise in [false, true] {
        let mut content = body.to_string();
        if precise {
            for (key, _) in aliases {
                content += &format!("'{key}'='Precise'\n");
            }
        }
        let config = Config::from_toml(&content).unwrap();
        let merged = config.get_merged_config("com.example.app").unwrap();
        let (map, explicit) = Config::build_merged_property_map_with_explicit(&merged);
        for (key, derived) in aliases {
            assert_eq!(map[key], if precise { "Precise" } else { derived });
            assert_eq!(explicit.contains(key), precise);
        }
        assert_eq!(map["ro.product.name"], "Custom product");
        assert_eq!(
            map["ro.vendor.build.version.security_patch"],
            "Custom patch"
        );
        assert_eq!(map["ro.product.vendor.marketname"], "Custom market");
    }
}

#[test]
fn custom_sdk_overrides_are_validated_before_constructing_a_property_plan() {
    for header in ["[templates.unused]", "[[apps]]\npackage='com.example.app'"] {
        for value in ["0", "2147483648", "__EMPTY__", "35.0", "+35", "035", ""] {
            let content = format!("{header}\ncustom_props={{'ro.build.version.sdk'='{value}'}}");
            assert!(Config::from_toml(&content).is_err(), "accepted {value}");
        }
        for value in ["1", "35", "2147483647", "__DELETE__"] {
            let content = format!("{header}\ncustom_props={{'ro.build.version.sdk'='{value}'}}");
            assert!(Config::from_toml(&content).is_ok());
        }
    }
}

#[test]
fn invalid_numbers_are_rejected_before_resolving_targets() {
    for header in ["[templates.unused]", "[[apps]]\npackage='com.example.app'"] {
        for setting in ["sdk_int=0", "sdk_int=2147483648", "dpi=0", "dpi=641"] {
            assert!(Config::from_toml(&format!("{header}\n{setting}")).is_err());
        }
    }
    let shadowed = "[templates.unused]\npackages=['com.example.app']\nsdk_int=0\n[[apps]]\npackage='com.example.app'\nmodel='Phone'";
    assert!(Config::from_toml(shadowed).is_err());
}

#[test]
fn valid_numbers_and_deletion_survive_shared_parsing() {
    for sdk in ["1", "2147483647", "'__DELETE__'"] {
        for dpi in [DPI_MIN, DPI_MAX] {
            let config = Config::from_toml(&format!(
                "[[apps]]\npackage='com.example.app'\nsdk_int={sdk}\ndpi={dpi}"
            ))
            .unwrap();
            assert_eq!(
                config.get_merged_config("com.example.app").unwrap().dpi,
                Some(dpi)
            );
        }
    }
}

#[test]
fn global_profiles_reject_mutable_properties_without_restricting_private_profiles() {
    for header in ["[templates.unused]", "[[apps]]\npackage='com.example.app'"] {
        for value in ["value", "__DELETE__"] {
            let content = format!(
                "{header}\ncompanion_resetprop=true\ncustom_props={{'sys.demo'='{value}'}}"
            );
            assert!(Config::from_toml(&content).is_err());
            assert!(
                Config::from_toml(
                    &content.replace("companion_resetprop=true", "companion_resetprop=false")
                )
                .is_ok()
            );
            assert!(Config::from_toml(&content.replace("sys.demo", "ro.demo")).is_ok());
        }
    }
}
