#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn executable(path: &Path, script: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn wrapper_preserves_backend_failures_without_running_a_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let wrapper = dir.path().join("bin/duckctl.sh");
    executable(&wrapper, include_str!("../../../module/bin/duckctl.sh"));
    executable(
        &dir.path().join("target/release/duckd"),
        "#!/bin/sh\nprintf 'fallback\\n'\nexit 0\n",
    );
    for status in [0, 1, 2, 42] {
        executable(
            &dir.path().join("bin/duckd"),
            &format!("#!/bin/sh\nprintf 'backend\\n'\nexit {status}\n"),
        );
        let output = Command::new("sh")
            .arg(&wrapper)
            .env("DUCK_TOOLBOX_ROOT", dir.path())
            .env("DUCK_TOOLBOX_BUSYBOX_REEXEC", "1")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(status));
        assert_eq!(output.stdout, b"backend\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn wrapper_falls_back_after_an_unusable_binary() {
    let dir = tempfile::tempdir().unwrap();
    let wrapper = dir.path().join("bin/duckctl.sh");
    executable(&wrapper, include_str!("../../../module/bin/duckctl.sh"));
    executable(
        &dir.path().join("target/release/duckd"),
        "#!/bin/sh\nprintf 'fallback\\n'\nexit 0\n",
    );
    for status in [126, 127] {
        executable(
            &dir.path().join("bin/duckd"),
            &format!("#!/bin/sh\nexit {status}\n"),
        );
        let output = Command::new("sh")
            .arg(&wrapper)
            .env("DUCK_TOOLBOX_ROOT", dir.path())
            .env("DUCK_TOOLBOX_BUSYBOX_REEXEC", "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"fallback\n");
    }
}
