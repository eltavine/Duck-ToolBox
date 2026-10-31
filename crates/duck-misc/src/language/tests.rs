use super::*;

fn context(root: &std::path::Path) -> Context {
    fs::create_dir_all(root.join("play-integrity")).unwrap();
    fs::write(root.join("play-integrity/language-spoof.version"), "1\n").unwrap();
    fs::write(root.join("play-integrity/classes.dex"), "dex fixture").unwrap();
    fs::create_dir_all(root.join("zygisk")).unwrap();
    fs::write(root.join("zygisk/arm64-v8a.so"), "ELF fixture").unwrap();
    Context {
        paths: duck_core::AppPaths::for_root(root, root),
        sysroot: duck_core::Sysroot::new(root),
    }
}
fn config() -> Config {
    Config {
        rules: vec![Rule {
            package_name: "com.example.app".into(),
            user_id: 0,
            locale: "en-us".into(),
        }],
    }
}
fn save_current(ctx: &Context, config: Config) -> Status {
    save_request(
        ctx,
        Save {
            revision: status(ctx).unwrap().revision,
            config,
        },
    )
    .unwrap()
}

#[test]
fn language_tags_are_canonical_and_unsupported_forms_fail_explicitly() {
    for (input, expected) in [
        ("en-us", "en-US"),
        ("zh-hant-tw", "zh-Hant-TW"),
        ("es-419", "es-419"),
        ("ar", "ar"),
    ] {
        assert_eq!(normalize_locale(input).as_deref(), Some(expected));
    }
    for input in [
        "",
        "en_US",
        "en--US",
        "en-US-u-ca-gregory",
        "und",
        "en;id",
        "éé",
        "en-US-foo",
    ] {
        assert!(normalize_locale(input).is_none(), "{input}");
    }
}

#[test]
fn user_scopes_are_independent_but_duplicate_targets_are_rejected() {
    let mut value = config();
    let mut work = value.rules[0].clone();
    work.user_id = 10;
    value.rules.push(work.clone());
    validate(&mut value).unwrap();
    value.rules.push(work);
    assert_eq!(
        code_of(&validate(&mut value).unwrap_err()),
        "invalid_config"
    );
    for package in [
        "com.example;id",
        "com..app",
        "com.example:remote",
        "../app",
        ".a",
        "com.1app",
    ] {
        assert!(!valid_package(package));
    }
}

#[test]
fn oversized_rules_never_replace_the_saved_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let saved = save_current(&ctx, config());
    let rules = (0..256)
        .map(|index| Rule {
            package_name: format!("com.{}.app{index}", "a".repeat(230)),
            user_id: 0,
            locale: "zh-Hant-TW".into(),
        })
        .collect();
    let error = save_request(
        &ctx,
        Save {
            revision: saved.revision.clone(),
            config: Config { rules },
        },
    )
    .err()
    .unwrap();
    assert_eq!(code_of(&error), "config_too_large");
    assert_eq!(status(&ctx).unwrap().revision, saved.revision);
}

#[test]
fn save_never_enables_and_stale_writers_cannot_replace_new_rules() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let initial = status(&ctx).unwrap();
    assert!(!initial.enabled);
    assert!(!directory(&ctx).exists());
    let saved = save_current(&ctx, config());
    assert!(!saved.enabled);
    assert_eq!(saved.config.unwrap().rules[0].locale, "en-US");
    let error = save_request(
        &ctx,
        Save {
            revision: initial.revision,
            config: Config::default(),
        },
    )
    .err()
    .unwrap();
    assert_eq!(code_of(&error), "stale_revision");
    assert_eq!(status(&ctx).unwrap().config.unwrap().rules.len(), 1);
}

#[test]
fn stop_invalidates_even_an_already_disabled_enable_request() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let saved = save_current(&ctx, config());
    disable(&ctx).unwrap();
    let error = enable_request(
        &ctx,
        Revision {
            revision: saved.revision,
        },
    )
    .err()
    .unwrap();
    assert_eq!(code_of(&error), "stale_revision");
    let enabled = enable_request(
        &ctx,
        Revision {
            revision: status(&ctx).unwrap().revision,
        },
    )
    .unwrap();
    assert!(enabled.enabled);
    let bytes = raw(&ctx).unwrap();
    disable(&ctx).unwrap();
    assert!(!status(&ctx).unwrap().enabled);
    assert_eq!(raw(&ctx).unwrap(), bytes);
}

#[test]
fn disabling_corrupt_rules_preserves_them_and_never_requires_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    save_current(&ctx, config());
    write(&ctx, "rules.json", b"broken").unwrap();
    write(&ctx, "enabled", b"1").unwrap();
    assert!(status(&ctx).unwrap().config_error.is_some());
    disable(&ctx).unwrap();
    assert!(!status(&ctx).unwrap().enabled);
    assert_eq!(raw(&ctx).unwrap(), b"broken");
}

#[test]
fn writers_are_serialized_without_blocking_status_reads() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    save_current(&ctx, config());
    let held = lock(&ctx).unwrap();
    assert_eq!(code_of(&lock(&ctx).unwrap_err()), "operation_busy");
    assert_eq!(status(&ctx).unwrap().config.unwrap().rules.len(), 1);
    drop(held);
    assert!(lock(&ctx).is_ok());
}

#[test]
fn enable_requires_a_complete_compatible_payload_and_regular_parent_flags() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    save_current(&ctx, config());
    fs::remove_file(ctx.paths.root.join("play-integrity/classes.dex")).unwrap();
    assert!(!status(&ctx).unwrap().payload_installed);
    let error = enable_request(
        &ctx,
        Revision {
            revision: status(&ctx).unwrap().revision,
        },
    )
    .err()
    .unwrap();
    assert_eq!(code_of(&error), "runtime_unavailable");
    fs::write(
        ctx.paths.root.join("play-integrity/classes.dex"),
        "dex fixture",
    )
    .unwrap();
    fs::write(
        ctx.paths.root.join("play-integrity/language-spoof.version"),
        "2\n",
    )
    .unwrap();
    assert!(!status(&ctx).unwrap().payload_installed);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temp.path().join("missing"), ctx.paths.root.join("disable"))
            .unwrap();
        assert!(!status(&ctx).unwrap().parent_active);
    }
    for invalid in [b"1".as_slice(), b"1:", b"1:\n", b"1:yes/no", b"1:ok\0"] {
        assert!(!active(invalid));
    }
    assert!(active(b"1:.tmp-123_A"));
}

#[cfg(unix)]
#[test]
fn state_is_private_and_final_symlinks_are_never_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    save_current(&ctx, config());
    assert_eq!(
        fs::metadata(directory(&ctx)).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let path = directory(&ctx).join("rules.json");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_file(&path).unwrap();
    symlink(temp.path().join("external"), &path).unwrap();
    assert!(raw(&ctx).is_err());
    disable(&ctx).unwrap();
    assert!(!temp.path().join("external").exists());
}
