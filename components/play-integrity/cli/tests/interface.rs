// SPDX-License-Identifier: GPL-3.0-only
// Independent CLI integration tests; no toolbox executable or runtime is involved.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn request(root: &std::path::Path, op: &str, input: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
        .args(["request", op])
        .env("PIF_ENGINE_DATA", root.join("data"))
        .env("PIF_ENGINE_MODULE", root.join("module"))
        .env("PIF_ENGINE_SYSROOT", root.join("sys"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fixture(script: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("module/scripts")).unwrap();
    fs::write(root.path().join("module/scripts/autopif4.sh"), script).unwrap();
    fs::write(root.path().join("module/scripts/migrate.sh"), "exit 0\n").unwrap();
    root
}
fn enable(root: &std::path::Path) {
    let result = request(root, "options", json!({"enabled":true}));
    assert_eq!(result["ok"], true, "{result}");
}
fn terminal(root: &std::path::Path, id: &Value) -> Value {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let result = request(root, "task-status", json!({"id":id}));
        if result["data"]["state"] != "running" {
            return result;
        }
        assert!(Instant::now() < end, "worker failed to terminate");
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn configuration_and_boot_stages_require_explicit_enabling() {
    let root = fixture("exit 0\n");
    fs::write(
        root.path().join("module/scripts/stage.sh"),
        "printf '%s\\n' \"$1\" >> \"$PIF_ENGINE_DATA/stage.trace\"\n",
    )
    .unwrap();
    let status = request(root.path(), "status", json!({}));
    assert_eq!(status["data"]["options"]["enabled"], false);
    assert_eq!(status["data"]["state"], "disabled");
    assert_eq!(request(root.path(), "task-start", json!({}))["ok"], false);
    let preview = request(root.path(), "preview", json!({"content":"MODEL=Saved"}));
    let applied = request(
        root.path(),
        "apply",
        json!({"content":"MODEL=Saved","revision":preview["data"]["revision"]}),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    let status = request(root.path(), "status", json!({}));
    assert_eq!(status["data"]["has_config"], true);
    assert_eq!(status["data"]["lifecycle_eligible"], false);
    assert_eq!(status["data"]["options"]["enabled"], false);
    let saved = request(root.path(), "options", json!({"mode":"scripts-only"}));
    assert_eq!(saved["data"]["options"]["enabled"], false);
    for phase in ["setup", "early", "late"] {
        let output = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
            .args(["stage", phase])
            .env("PIF_ENGINE_DATA", root.path().join("data"))
            .env("PIF_ENGINE_MODULE", root.path().join("module"))
            .env("PIF_ENGINE_SYSROOT", root.path().join("sys"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{phase}: {output:?}");
    }
    assert_eq!(
        fs::read_to_string(root.path().join("data/stage.trace")).unwrap(),
        "cleanup\n"
    );
    assert_eq!(
        request(root.path(), "power", json!({}))["data"]["enabled"],
        false
    );
    assert_eq!(
        request(root.path(), "config", json!({}))["data"]["entries"]["MODEL"],
        "Saved"
    );
    enable(root.path());
    let status = request(root.path(), "status", json!({}));
    assert_eq!(status["data"]["options"]["enabled"], true);
    assert_eq!(status["data"]["state"], "scripts-only");
    assert_eq!(status["data"]["lifecycle_eligible"], true);
}
#[test]
fn generation_previews_without_publishing_and_import_keeps_source() {
    let root = fixture(
        "echo 'fetching fixture'; printf 'MODEL=Fixture\nSECURITY_PATCH=2026-10-05\n' > custom.pif.prop\n",
    );
    enable(root.path());
    let status = request(root.path(), "status", json!({}));
    assert_eq!(status["data"]["options"]["enabled"], true);
    let started = request(root.path(), "task-start", json!({"strong":true}));
    assert_eq!(started["ok"], true, "{started}");
    let result = terminal(root.path(), &started["data"]["id"]);
    assert_eq!(result["data"]["state"], "completed", "{result}");
    assert_eq!(result["data"]["result"]["entries"]["MODEL"], "Fixture");
    assert!(!root.path().join("data/custom.pif.json").exists());
    fs::create_dir_all(root.path().join("sys/source")).unwrap();
    fs::write(
        root.path().join("sys/source/pif.prop"),
        "MODEL=Original\nEXTRA=keep",
    )
    .unwrap();
    let imported = request(
        root.path(),
        "import-preview",
        json!({"source":"/source/pif.prop"}),
    );
    assert_eq!(imported["data"]["entries"]["EXTRA"], "keep");
    assert_eq!(
        fs::read_to_string(root.path().join("sys/source/pif.prop")).unwrap(),
        "MODEL=Original\nEXTRA=keep"
    );
}
#[test]
fn cancellation_terminates_worker_and_preserves_live_config() {
    let root = fixture("sleep 30; echo MODEL=unexpected > custom.pif.prop\n");
    enable(root.path());
    let started = request(root.path(), "task-start", json!({}));
    let id = &started["data"]["id"];
    assert_eq!(
        request(root.path(), "task-cancel", json!({"id":id}))["ok"],
        true
    );
    let result = terminal(root.path(), id);
    assert_eq!(result["data"]["state"], "cancelled", "{result}");
    assert!(!root.path().join("data/custom.pif.json").exists());
}

#[cfg(unix)]
#[test]
fn every_job_exit_stops_descendants_that_ignore_termination() {
    struct Cleanup(Vec<i32>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for pid in &self.0 {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
    }
    for scenario in ["cancel", "timeout", "overflow", "complete"] {
        let ending = match scenario {
            "overflow" => "head -c 1200000 /dev/zero; sleep 30",
            "complete" => "printf 'MODEL=Fixture\\n' > custom.pif.prop",
            _ => "sleep 30",
        };
        let root = fixture(&format!(
            r#"
echo $$ > "$PIF_ENGINE_DATA/generator.pid"
(trap '' TERM; while :; do printf x >> "$PIF_ENGINE_DATA/heartbeat"; sleep 0.02; done) &
while [ ! -s "$PIF_ENGINE_DATA/heartbeat" ]; do sleep 0.01; done
{ending}
"#
        ));
        enable(root.path());
        let started = request(root.path(), "task-start", json!({"timeout_seconds":1}));
        assert_eq!(started["ok"], true, "{started}");
        let mut cleanup = Cleanup(vec![started["data"]["pid"].as_u64().unwrap() as i32]);
        let heartbeat = root.path().join("data/heartbeat");
        let end = Instant::now() + Duration::from_secs(5);
        while fs::metadata(&heartbeat).is_err() {
            assert!(Instant::now() < end, "generator did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        cleanup.0.push(
            fs::read_to_string(root.path().join("data/generator.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap(),
        );
        let id = &started["data"]["id"];
        if scenario == "cancel" {
            assert_eq!(
                request(root.path(), "task-cancel", json!({"id":id}))["ok"],
                true
            );
        }
        let finished = terminal(root.path(), id);
        let expected = match scenario {
            "cancel" => "cancelled",
            "complete" => "completed",
            _ => "failed",
        };
        assert_eq!(
            finished["data"]["state"], expected,
            "{scenario}: {finished}"
        );
        let before = fs::metadata(&heartbeat).unwrap().len();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            fs::metadata(&heartbeat).unwrap().len(),
            before,
            "{scenario}: background writer survived job completion"
        );
        assert!(!root.path().join("data/custom.pif.json").exists());
    }
}
#[test]
fn options_conflicts_and_stale_apply_are_explicit() {
    let root = fixture("exit 1\n");
    enable(root.path());
    let conflict = root.path().join("sys/data/adb/modules/playintegrityfix");
    fs::create_dir_all(&conflict).unwrap();
    fs::write(conflict.join("module.prop"), "id=playintegrityfix").unwrap();
    assert_eq!(
        request(root.path(), "status", json!({}))["data"]["state"],
        "conflict"
    );
    fs::write(conflict.join("disable"), "").unwrap();
    let preview = request(root.path(), "preview", json!({"content":"MODEL=A"}));
    let applied = request(
        root.path(),
        "apply",
        json!({"content":"MODEL=A","revision":preview["data"]["revision"]}),
    );
    assert_eq!(applied["ok"], true);
    assert_eq!(
        request(
            root.path(),
            "apply",
            json!({"content":"MODEL=B","revision":preview["data"]["revision"]})
        )["ok"],
        false
    );
    let disabled = request(
        root.path(),
        "options",
        json!({"enabled":false,"mode":"scripts-only"}),
    );
    assert_eq!(disabled["ok"], true);
    assert_eq!(
        request(root.path(), "status", json!({}))["data"]["state"],
        "disabled"
    );
    assert_eq!(
        request(
            root.path(),
            "options",
            json!({"app_replace_list":"/system/../data"})
        )["ok"],
        false
    );
}
#[test]
fn insecure_fetch_is_rejected_without_network() {
    let output = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
        .args(["fetch", "-O", "/tmp/not-created", "http://localhost/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("only HTTPS"));
}

#[test]
fn magisk_directory_is_not_zygisk_runtime_evidence() {
    let root = fixture("exit 0\n");
    fs::create_dir_all(root.path().join("sys/data/adb/magisk")).unwrap();
    let result = request(root.path(), "status", json!({}));
    assert_eq!(result["data"]["zygisk_detected"], false);
    let provider = root.path().join("sys/data/adb/modules/zygisksu");
    fs::create_dir_all(&provider).unwrap();
    fs::write(provider.join("module.prop"), "id=zygisksu").unwrap();
    assert_eq!(
        request(root.path(), "status", json!({}))["data"]["zygisk_detected"],
        true
    );
    fs::write(provider.join("disable"), "").unwrap();
    assert_eq!(
        request(root.path(), "status", json!({}))["data"]["zygisk_detected"],
        false
    );
}

#[test]
fn standalone_file_migration_preserves_shell_metacharacters_as_data() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let source = root.path().join("source.json");
    let output = root.path().join("custom.pif.json");
    let marker = root.path().join("must-not-exist");
    let payload = format!(r#"$(touch {}) ' \" quoted"#, marker.display());
    fs::write(
        &source,
        serde_json::to_vec(&json!({"EXTRA":payload,"MODEL":"a"})).unwrap(),
    )
    .unwrap();
    fs::write(&output, "original").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
        .arg("migrate-file")
        .arg(&source)
        .arg(&output)
        .args(["--advanced", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let migrated: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(migrated["EXTRA"], payload);
    assert_eq!(
        fs::read_to_string(root.path().join("custom.pif.json.bak")).unwrap(),
        "original"
    );
    assert!(!marker.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[test]
fn timed_out_jobs_fail_without_publishing() {
    let root = fixture("sleep 30; echo MODEL=unexpected > custom.pif.prop\n");
    enable(root.path());
    let started = request(root.path(), "task-start", json!({"timeout_seconds":1}));
    let result = terminal(root.path(), &started["data"]["id"]);
    assert_eq!(result["data"]["state"], "failed", "{result}");
    assert!(
        result["data"]["error"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    assert!(!root.path().join("data/custom.pif.json").exists());
    assert_eq!(
        request(root.path(), "task-start", json!({"timeout_seconds":121}))["ok"],
        false
    );
}

#[test]
fn lifecycle_uses_cleanup_when_disabled_and_records_the_boot_options() {
    let root = fixture("exit 0\n");
    fs::write(
        root.path().join("module/scripts/stage.sh"),
        "printf '%s\\n' \"$1\" >> \"$PIF_ENGINE_DATA/stage.trace\"\n",
    )
    .unwrap();
    request(root.path(), "options", json!({"enabled":false}));
    let output = Command::new(env!("CARGO_BIN_EXE_pif-engine"))
        .args(["stage", "early"])
        .env("PIF_ENGINE_DATA", root.path().join("data"))
        .env("PIF_ENGINE_MODULE", root.path().join("module"))
        .env("PIF_ENGINE_SYSROOT", root.path().join("sys"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("data/stage.trace")).unwrap(),
        "cleanup\ncleanup\n"
    );
    let evidence: Value =
        serde_json::from_slice(&fs::read(root.path().join("data/boot.json")).unwrap()).unwrap();
    assert_eq!(evidence["options"]["enabled"], false);
}

#[test]
fn disable_cancels_generation_and_survives_broken_options() {
    let root = fixture("sleep 30; echo MODEL=unexpected > custom.pif.prop\n");
    enable(root.path());
    let started = request(root.path(), "task-start", json!({}));
    fs::write(root.path().join("data/options.json"), "{broken").unwrap();
    let stopped = request(root.path(), "disable", json!({}));
    assert_eq!(stopped["ok"], true, "{stopped}");
    assert_eq!(stopped["data"]["enabled"], false);
    assert_eq!(
        terminal(root.path(), &started["data"]["id"])["data"]["state"],
        "cancelled"
    );
    assert_eq!(
        request(root.path(), "power", json!({}))["data"]["enabled"],
        false
    );
    assert_eq!(request(root.path(), "task-start", json!({}))["ok"], false);
    assert_eq!(
        fs::read_to_string(root.path().join("data/options.json")).unwrap(),
        "{broken"
    );
}

#[cfg(unix)]
#[test]
fn magisk_adapter_journals_cli_changes_and_retains_external_edits() {
    use std::os::unix::fs::PermissionsExt;
    let root = fixture("exit 0\n");
    fs::write(root.path().join("module/scripts/stage.sh"), "exit 0\n").unwrap();
    enable(root.path());
    let magisk_dir = root.path().join("sys/data/adb/magisk");
    fs::create_dir_all(&magisk_dir).unwrap();
    let magisk = magisk_dir.join("magisk");
    fs::write(&magisk,r#"#!/bin/sh
base=${0%/*}
case "$2" in
status) exit 0 ;;
ls) cat "$base/list" ;;
rm) awk -F'|' -v pkg="$3" -v proc="$4" '$1!=pkg || $2!=proc' "$base/list" > "$base/next"; mv "$base/next" "$base/list" ;;
add) printf '%s|%s\n' "$3" "$4" >> "$base/list" ;;
esac
"#).unwrap();
    fs::set_permissions(&magisk, fs::Permissions::from_mode(0o700)).unwrap();
    let original =
        "com.google.android.gms|com.google.android.gms\ncom.android.vending|com.android.vending\n";
    fs::write(magisk_dir.join("list"), original).unwrap();
    let preview = request(root.path(), "preview", json!({"content":"MODEL=A"}));
    assert_eq!(
        request(
            root.path(),
            "apply",
            json!({"content":"MODEL=A","revision":preview["data"]["revision"]})
        )["ok"],
        true
    );
    let early = || {
        Command::new(env!("CARGO_BIN_EXE_pif-engine"))
            .args(["stage", "early"])
            .env("PIF_ENGINE_DATA", root.path().join("data"))
            .env("PIF_ENGINE_MODULE", root.path().join("module"))
            .env("PIF_ENGINE_SYSROOT", root.path().join("sys"))
            .output()
            .unwrap()
    };
    assert!(early().status.success());
    assert_eq!(fs::read_to_string(magisk_dir.join("list")).unwrap(), "");
    let stopped = request(root.path(), "disable", json!({}));
    assert_eq!(stopped["data"]["cleanup_pending"], false, "{stopped}");
    let entries = |s: &str| {
        s.lines()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        entries(&fs::read_to_string(magisk_dir.join("list")).unwrap()),
        entries(original)
    );
    assert_eq!(
        request(
            root.path(),
            "options",
            json!({"enabled":true,"expected_stop_revision":stopped["data"]["stop_revision"]})
        )["ok"],
        true
    );
    assert!(early().status.success());
    fs::write(
        magisk_dir.join("list"),
        "com.google.android.gms|com.google.android.gms.external\n",
    )
    .unwrap();
    let stopped = request(root.path(), "disable", json!({}));
    assert_eq!(stopped["data"]["cleanup_pending"], true);
    let saved = fs::read_to_string(magisk_dir.join("list")).unwrap();
    assert!(saved.contains(".external"));
    assert!(!saved.contains("com.google.android.gms|com.google.android.gms\n"));
    // Resolve only the GMS conflict; Play Store was already restored independently.
    fs::write(
        magisk_dir.join("list"),
        "com.android.vending|com.android.vending\n",
    )
    .unwrap();
    assert_eq!(
        request(root.path(), "disable", json!({}))["data"]["cleanup_pending"],
        false
    );
    assert_eq!(
        entries(&fs::read_to_string(magisk_dir.join("list")).unwrap()),
        entries(original)
    );
}
