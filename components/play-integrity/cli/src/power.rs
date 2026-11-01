// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: independent stop gate, readable without valid fingerprint configuration.
use crate::{Engine, files};
pub fn gated(engine: &Engine) -> bool {
    if !engine.data.join("disabled").exists() {
        return false;
    }
    let stopped = files::read(&engine.data.join("disabled"));
    let acknowledged = files::read(&engine.data.join("enabled-token"));
    match (stopped, acknowledged) {
        (Ok(a), Ok(b)) => {
            !engine.data.join("enabled-token").exists()
                || a.is_empty()
                || a.len() > 128
                || b.len() > 128
                || a != b
        }
        _ => true,
    }
}
pub fn disabled(engine: &Engine) -> bool {
    if gated(engine) {
        return true;
    }
    match files::json_file(&engine.data.join("options.json")) {
        Ok(value) if value.is_object() => value.get("enabled").is_none_or(|v| v != true),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_true_enables_the_engine() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine {
            data: root.path().into(),
            module: root.path().into(),
            sysroot: root.path().into(),
        };
        assert!(disabled(&engine));
        for options in [
            "",
            "{}",
            r#"{"mode":"scripts-only"}"#,
            r#"{"enabled":false}"#,
            r#"{"enabled":null}"#,
            r#"{"enabled":"true"}"#,
            r#"{"enabled":1}"#,
            "{broken",
            "[]",
        ] {
            files::atomic(&engine.data.join("options.json"), options.as_bytes()).unwrap();
            assert!(disabled(&engine), "{options}");
        }
        files::atomic(&engine.data.join("options.json"), br#"{"enabled":true}"#).unwrap();
        assert!(!disabled(&engine));
    }
    #[test]
    fn empty_tokens_cannot_acknowledge_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine {
            data: root.path().into(),
            module: root.path().into(),
            sysroot: root.path().into(),
        };
        files::atomic(&engine.data.join("disabled"), b"").unwrap();
        files::atomic(&engine.data.join("enabled-token"), b"").unwrap();
        assert!(gated(&engine));
        files::atomic(&engine.data.join("disabled"), b"valid-stop").unwrap();
        files::atomic(&engine.data.join("enabled-token"), b"valid-stop").unwrap();
        assert!(!gated(&engine));
        files::atomic(&engine.data.join("enabled-token"), b"").unwrap();
        assert!(gated(&engine));
    }
}
