use anyhow::{Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::config::Config;
use crate::error::Error;

pub const MAX_CONFIG: usize = 48 * 1024;
pub const EMPTY: &str = "debug = false\ndefault_force_denylist_unmount = false\n\n[templates]\n";

pub fn revision(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn package(value: &str) -> Result<()> {
    let (name, user) = value
        .split_once('@')
        .map_or((value, None), |(p, u)| (p, Some(u)));
    if name.len() > 255
        || !name.contains('.')
        || name.split('.').any(|part| {
            part.is_empty() || !part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
        || user.is_some_and(|u| {
            u.parse::<u32>()
                .map_or(true, |value| value.to_string() != u)
        })
    {
        bail!(Error::params(
            "invalid_package",
            "Invalid application key",
            json!({"target":value})
        ));
    }
    Ok(())
}

pub fn inspect(content: &str) -> Result<Value> {
    if content.len() > MAX_CONFIG {
        bail!(Error::new(
            "config_too_large",
            "Configuration exceeds 48 KiB"
        ));
    }
    let config = Config::from_toml(content).map_err(|error| {
        if matches!(
            error.downcast_ref(),
            Some(crate::config::ConfigValidationError::DensityRange)
        ) {
            Error::new("density_range", "DPI must be between 120 and 640").into()
        } else {
            error
        }
    })?;
    let mut assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut targets = Vec::new();
    for (name, template) in &config.templates {
        for key in &template.packages {
            package(key)?;
            assignments
                .entry(key.clone())
                .or_default()
                .push(format!("template:{name}"));
            targets.push(key.clone());
        }
    }
    for app in &config.apps {
        package(&app.package)?;
        targets.push(app.package.clone());
        assignments
            .entry(app.package.clone())
            .or_default()
            .push("app".into());
    }
    targets.sort();
    targets.dedup();
    let mut warnings = Vec::new();
    for (key, owners) in assignments {
        if owners.len() > 1 {
            warnings.push(json!({"code":"overlap","target":key,"count":owners.len()}));
        }
    }
    let mut global_effects = false;
    let mut deletions = false;
    for key in &targets {
        if let Some(merged) = config.get_merged_config(key) {
            global_effects |= merged.companion_resetprop || merged.dpi.is_some();
            deletions |=
                !Config::build_delete_props_list(&merged).is_empty() && !merged.companion_resetprop;
        }
    }
    if deletions {
        warnings.push(json!({"code":"delete_requires_global"}));
    }
    if targets.iter().any(|p| {
        matches!(
            p.split('@').next(),
            Some("com.google.android.gms" | "com.android.vending")
        )
    }) {
        warnings.push(json!({"code":"pi_priority"}));
    }
    if !config.default_hide_maps.is_empty()
        || config
            .apps
            .iter()
            .any(|app| app.hide_maps.as_ref().is_some_and(|v| !v.is_empty()))
        || config
            .templates
            .values()
            .any(|value| value.hide_maps.as_ref().is_some_and(|v| !v.is_empty()))
    {
        warnings.push(json!({"code":"mapping_hide_ignored"}));
    }
    Ok(json!({
        "templates": config.templates.keys().collect::<Vec<_>>(),
        "targets": targets, "warnings": warnings, "global_effects": global_effects,
    }))
}

pub fn snapshot(content: &str) -> Value {
    match inspect(content) {
        Ok(summary) => {
            json!({"content": content, "revision": revision(content), "summary": summary, "error": null})
        }
        Err(error) => {
            json!({"content": content, "revision": revision(content), "summary": null, "error": format!("{error:#}")})
        }
    }
}
