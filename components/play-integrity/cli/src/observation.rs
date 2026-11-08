// SPDX-License-Identifier: GPL-3.0-only
//! Bound log readbacks to the saved revision, boot and currently live process.
use crate::{Engine, command, files};
use serde_json::{Value, json};
use std::{process::Command, time::Duration};

fn live(engine: &Engine, value: &Value, revision: &str, boot: &str) -> bool {
    let Some(pid) = value["pid"]
        .as_u64()
        .filter(|p| *p > 0 && *p <= i32::MAX as u64)
    else {
        return false;
    };
    let Some(process) = value["process"]
        .as_str()
        .filter(|p| ["com.google.android.gms.unstable", "com.android.vending"].contains(p))
    else {
        return false;
    };
    if boot.is_empty() || value["boot_id"] != boot || value["revision"] != revision {
        return false;
    }
    let stat = files::read(&engine.device(&format!("/proc/{pid}/stat"))).unwrap_or_default();
    let token = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(19));
    let name = files::read(&engine.device(&format!("/proc/{pid}/cmdline"))).unwrap_or_default();
    token.is_some_and(|t| value["start"] == t)
        && name.split('\0').next() == Some(process)
        && value["build"].is_object()
        && value["properties"].is_object()
        && value["property_hook"].is_boolean()
}
pub fn parse(engine: &Engine, raw: &str, revision: &str) -> Vec<Value> {
    let boot = files::read(&engine.device("/proc/sys/kernel/random/boot_id")).unwrap_or_default();
    let mut result = Vec::<Value>::new();
    for line in raw.lines().filter(|s| s.len() < 3500) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("observed_at").is_some_and(|v| !v.is_u64()) {
            continue;
        }
        match (
            value.get("private_requested"),
            value.get("private_matching"),
        ) {
            (None, None) => {}
            (Some(requested), Some(matching))
                if requested
                    .as_u64()
                    .zip(matching.as_u64())
                    .is_some_and(|(n, m)| {
                        m <= n && n <= 1024 && (n == 0 || value["property_hook"] == false)
                    }) => {}
            _ => continue,
        }
        if !live(engine, &value, revision, boot.trim()) {
            continue;
        }
        if !["build", "properties"].iter().all(|key| {
            value[key].as_object().is_some_and(|m| {
                m.len() <= 16
                    && m.values()
                        .all(|v| v.as_str().is_some_and(|s| s.len() <= 256))
            })
        }) {
            continue;
        }
        result.retain(|v| v["process"] != value["process"]);
        result.push(value);
    }
    result
}
pub fn read(engine: &Engine, revision: &str) -> Value {
    if engine.sysroot != std::path::Path::new("/") {
        return json!({"processes":[],"read_error":null});
    }
    // A line-count tail is applied before tag filtering and can omit live targets.
    let output = command::run(
        Command::new("logcat").args([
            "-d",
            "-b",
            "main",
            "-v",
            "raw",
            "-s",
            "PIF/Observation:I",
            "*:S",
        ]),
        Duration::from_secs(2),
    );
    match output {
        Ok(output) if output.status.success() => {
            json!({"processes":parse(engine,&String::from_utf8_lossy(&output.stdout),revision),"read_error":null})
        }
        _ => json!({"processes":[],"read_error":"runtime_observation_unavailable"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_boot_revision_pid_and_process_identity_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().join("data"),
            module: root.path().join("module"),
            sysroot: root.path().join("sys"),
        };
        files::atomic(&e.device("/proc/sys/kernel/random/boot_id"), b"boot").unwrap();
        let stat = format!("42 (a process) S {} 123", vec!["0"; 18].join(" "));
        files::atomic(&e.device("/proc/42/stat"), stat.as_bytes()).unwrap();
        files::atomic(&e.device("/proc/42/cmdline"), b"com.android.vending\0").unwrap();
        let v = json!({"pid":42,"process":"com.android.vending","boot_id":"boot","revision":"revision","start":"123","build":{},"properties":{},"property_hook":false});
        assert_eq!(parse(&e, &v.to_string(), "revision").len(), 1);
        for (key, bad) in [
            ("boot_id", json!("old")),
            ("revision", json!("old")),
            ("start", json!("999")),
            ("pid", json!(99)),
            ("process", json!("other")),
        ] {
            let mut bad_v = v.clone();
            bad_v[key] = bad;
            assert!(parse(&e, &bad_v.to_string(), "revision").is_empty());
        }
    }
}
