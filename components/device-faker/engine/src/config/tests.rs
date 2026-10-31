use super::*;

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
