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

fn boot_fixture() -> (tempfile::TempDir, Paths) {
    let (root, paths) = fixture();
    write(
        &paths.root.join("proc/sys/kernel/random/boot_id"),
        "boot-test",
    )
    .unwrap();
    write(
        &paths.module("duck-toolbox").join("module.prop"),
        "id=duck-toolbox",
    )
    .unwrap();
    (root, paths)
}
#[test]
fn boot_publishes_readiness_only_after_the_ordered_pass_and_skips_duplicates() {
    let (_root, paths) = boot_fixture();
    run_boot(&paths, || {
        assert!(!crate::gate::startup_ready_at(&paths.root));
        Ok(())
    })
    .unwrap();
    assert!(crate::gate::startup_ready_at(&paths.root));
    run_boot(&paths, || {
        panic!("must not mutate global props after readiness")
    })
    .unwrap();
}
#[test]
fn failed_boot_replaces_stale_readiness_and_can_be_retried() {
    let (_root, paths) = boot_fixture();
    write(&paths.file("ready"), "old-boot:old-build").unwrap();
    assert!(run_boot(&paths, || anyhow::bail!("property write failed")).is_err());
    assert!(!crate::gate::startup_ready_at(&paths.root));
    assert_eq!(
        read(&paths.file("startup-failed"), 64).unwrap().as_deref(),
        Some("boot-test")
    );
    run_boot(&paths, || Ok(())).unwrap();
    assert!(crate::gate::startup_ready_at(&paths.root));
    assert!(!paths.file("startup-failed").exists());
}
#[test]
fn concurrent_boot_does_not_publish_failure_or_run_a_second_pass() {
    let (_root, paths) = boot_fixture();
    private_dir(&paths.data).unwrap();
    let owner = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.file("boot.lock"))
        .unwrap();
    owner.try_lock().unwrap();
    assert_eq!(
        run_boot(&paths, || panic!("another boot owns the pass")).unwrap()["boot_in_progress"],
        true
    );
    assert!(!paths.file("startup-failed").exists());
    assert!(!crate::gate::startup_ready_at(&paths.root));
}

#[test]
fn deletion_wins_over_an_overlapping_set_in_both_property_plans() {
    let config = crate::config::Config::from_toml("[[apps]]\npackage='com.app'\nmodel='Phone'\n[apps.custom_props]\n'ro.product.model'='__DELETE__'\n").unwrap();
    let merged = config.get_merged_config("com.app").unwrap();
    let (properties, _) = crate::config::Config::build_merged_property_map_with_explicit(&merged);
    let deleted = crate::config::Config::build_delete_props_list(&merged);
    assert!(deleted.contains(&"ro.product.model".into()));
    assert!(!properties.contains_key("ro.product.model"));
}
#[test]
fn a_new_boot_clears_only_the_previous_boot_global_failure_marker() {
    let (_root, paths) = boot_fixture();
    write(
        &paths.file("cleanup-failed"),
        "previous-boot:global_cleanup_pending",
    )
    .unwrap();
    run_boot(&paths, || Ok(())).unwrap();
    assert!(!paths.file("cleanup-failed").exists());
    fs::remove_file(paths.file("ready")).unwrap();
    write(
        &paths.file("cleanup-failed"),
        "boot-test:global_cleanup_pending",
    )
    .unwrap();
    run_boot(&paths, || Ok(())).unwrap();
    assert!(paths.file("cleanup-failed").exists());
}

#[cfg(unix)]
#[test]
fn boot_adapter_runs_legacy_then_integrity_then_fallback_before_readiness() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, paths) = boot_fixture();
    let module = paths.module("duck-toolbox");
    write(
        &module.join("prop.sh"),
        "printf 'prop %s\\n' \"$*\" >> \"${0%/*}/order\"\n",
    )
    .unwrap();
    write(
        &module.join("play-integrity/pif-engine"),
        "#!/bin/sh\nprintf 'integrity %s\\n' \"$*\" >> \"$PIF_ENGINE_MODULE/../order\"\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(
        module.join("play-integrity/pif-engine"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    run_boot(&paths, || crate::boot::stages(&module)).unwrap();
    assert!(crate::gate::startup_ready_at(&paths.root));
    assert_eq!(
        fs::read_to_string(module.join("order")).unwrap(),
        "prop \nintegrity stage late\nprop --fallback\n"
    );
    fs::remove_file(paths.file("ready")).unwrap();
    write(&module.join("prop.sh"), "exit 1\n").unwrap();
    assert!(run_boot(&paths, || crate::boot::stages(&module)).is_err());
    assert!(!crate::gate::startup_ready_at(&paths.root));
}

#[cfg(unix)]
#[test]
fn slow_integrity_boot_finishes_before_readiness_without_premature_fallback() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, paths) = boot_fixture();
    let module = paths.module("duck-toolbox");
    write(
        &module.join("prop.sh"),
        "printf 'prop %s\\n' \"$*\" >> \"${0%/*}/order\"\n",
    )
    .unwrap();
    let engine = module.join("play-integrity/pif-engine");
    write(
        &engine,
        "#!/bin/sh\nsleep 5.2\nprintf 'integrity complete\\n' >> \"$PIF_ENGINE_MODULE/../order\"\n",
    )
    .unwrap();
    fs::set_permissions(&engine, fs::Permissions::from_mode(0o755)).unwrap();
    run_boot(&paths, || crate::boot::stages(&module)).unwrap();
    assert!(crate::gate::startup_ready_at(&paths.root));
    assert_eq!(
        fs::read_to_string(module.join("order")).unwrap(),
        "prop \nintegrity complete\n"
    );
}

#[test]
fn nul_characters_are_rejected_before_a_property_plan_can_be_applied() {
    for content in [
        r#"[[apps]]
package='com.app'
model="bad\u0000value"
"#,
        r#"[[apps]]
package='com.app'
[apps.custom_props]
"ro.bad\u0000key"='__DELETE__'
"#,
        r#"[templates.unused]
model="bad\u0000value"
"#,
    ] {
        assert!(
            crate::config::Config::from_toml(content)
                .unwrap_err()
                .is::<crate::config::ConfigValidationError>()
        );
    }
    assert!(
        crate::config::Config::from_toml(
            r#"[[apps]]
package='com.app'
model='literal\u0000text'
"#
        )
        .is_ok()
    );
}

#[test]
fn uninstall_keeps_a_pending_global_cleanup_visible_to_the_shell() {
    let (_root, paths) = fixture();
    write(&paths.file("enabled"), "initial").unwrap();
    write(
        &paths.file("cleanup-failed"),
        "boot-test:global_cleanup_pending",
    )
    .unwrap();
    assert!(uninstall_at(&paths).is_err());
    assert!(!paths.enabled().unwrap());
    assert!(paths.file("cleanup-failed").exists());
    fs::remove_file(paths.file("cleanup-failed")).unwrap();
    assert!(uninstall_at(&paths).is_ok());
}

#[test]
fn startup_cannot_overwrite_a_same_boot_orphan_but_clears_a_previous_boot() {
    let (_root, paths) = boot_fixture();
    let marker = crate::global_marker::path(&paths.root);
    write(&marker, "boot-test\n2000000000\n42\ninitial\n").unwrap();
    assert!(
        run_boot(&paths, || panic!(
            "orphaned globals must not become a new baseline"
        ))
        .is_err()
    );
    assert!(marker.exists());
    assert!(!crate::gate::startup_ready_at(&paths.root));
    write(&marker, "previous-boot\n2000000000\n42\ninitial\n").unwrap();
    run_boot(&paths, || Ok(())).unwrap();
    assert!(!marker.exists());
    assert!(crate::gate::startup_ready_at(&paths.root));
}

#[cfg(unix)]
#[test]
fn completed_transactions_release_locks_inherited_by_a_forked_child() {
    const CHILD: &str = "DEVICE_FAKER_TEST_INHERITED_STATE_LOCKS";
    if std::env::var_os(CHILD).is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::tests::completed_transactions_release_locks_inherited_by_a_forked_child"
                ])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    struct PausedChild(libc::pid_t);
    impl Drop for PausedChild {
        fn drop(&mut self) {
            // SAFETY: this test owns the unreaped child until this guard is dropped.
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                while libc::waitpid(self.0, std::ptr::null_mut(), 0) < 0 {
                    if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                        break;
                    }
                }
            }
        }
    }
    fn fork_paused() -> PausedChild {
        // SAFETY: the isolated child only calls async-signal-safe pause before being reaped.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            loop {
                unsafe {
                    libc::pause();
                }
            }
        }
        PausedChild(pid)
    }
    let (_root, paths) = boot_fixture();
    {
        let lock = paths.lock().unwrap();
        let _child = fork_paused();
        assert!(paths.lock().is_err(), "an active writer remains exclusive");
        drop(lock);
        assert!(
            paths.lock().is_ok(),
            "an inherited descriptor must not retain a completed write lock"
        );
    }
    let mut child = None;
    run_boot(&paths, || {
        child = Some(fork_paused());
        Ok(())
    })
    .unwrap();
    let _child = child.unwrap();
    fs::remove_file(paths.file("ready")).unwrap();
    let marker = crate::global_marker::path(&paths.root);
    write(&marker, "previous-boot\n2000000000\n42\ninitial\n").unwrap();
    let mut ran = false;
    let result = run_boot(&paths, || {
        ran = true;
        Ok(())
    })
    .unwrap();
    assert_eq!(result["startup_ready"], true);
    assert!(
        ran,
        "an inherited descriptor must not retain a completed boot lock"
    );
    assert!(!marker.exists());
}
