//! Public process interface for the separately built Device Faker engine.
mod boot;
#[path = "../../engine/src/bounded.rs"]
mod bounded;
#[allow(dead_code, unused_imports)]
#[path = "../../engine/src/config/mod.rs"]
mod config;
#[allow(dead_code)]
#[path = "../../engine/src/density.rs"]
mod density;
mod document;
mod error;
#[cfg(test)]
#[path = "../../engine/src/focus.rs"]
mod focus;
#[allow(dead_code)]
#[path = "../../engine/src/toolbox.rs"]
mod gate;
#[allow(dead_code)]
#[path = "../../engine/src/global_marker.rs"]
mod global_marker;
#[cfg(all(test, unix, not(target_os = "android")))]
#[path = "../../engine/src/hooks.rs"]
mod hooks;
#[cfg(all(test, unix, not(target_os = "android")))]
#[path = "../../engine/src/jni_policy.rs"]
mod jni_policy;
#[allow(dead_code)]
#[path = "../../engine/src/process_identity.rs"]
mod process_identity;
mod state;

use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::io::{Read, Write};

fn execute() -> Result<Value> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["request", operation] => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(128 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 128 * 1024 {
                bail!("input exceeds 128 KiB");
            }
            let input = if bytes.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(&bytes)?
            };
            state::request(operation, &input)
        }
        ["stage", "setup"] => state::setup(),
        ["stage", "boot"] => state::boot(),
        ["stage", "ready"] => state::ready(),
        ["stage", "blocked"] => state::blocked(),
        ["stage", "uninstall"] => state::uninstall(),
        _ => bail!(
            "usage: device-faker-engine request OPERATION | stage setup/boot/ready/blocked/uninstall"
        ),
    }
}

fn main() {
    let (output, code) = match execute() {
        Ok(data) => (json!({"ok": true, "api": 1, "data": data}), 0),
        Err(error) => {
            let domain = error.downcast_ref::<crate::error::Error>();
            let code = domain.map(|value| value.code).unwrap_or_else(|| {
                if error.downcast_ref::<toml::de::Error>().is_some()
                    || error
                        .downcast_ref::<config::ConfigValidationError>()
                        .is_some()
                {
                    "invalid_config"
                } else {
                    "engine_failed"
                }
            });
            (
                json!({"ok":false,"api":1,"error":{"code":code,"params":domain.map(|e|e.params.clone()).unwrap_or_else(||json!({})),"message":format!("{error:#}")}}),
                1,
            )
        }
    };
    let _ = writeln!(std::io::stdout().lock(), "{output}");
    std::process::exit(code);
}
