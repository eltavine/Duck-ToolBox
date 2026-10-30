// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: private, bounded configuration snapshots and user-reported tests.
use crate::{Engine, config, files, text};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
struct Profile {
    id: String,
    name: String,
    created: u64,
    content: String,
    revision: String,
    #[serde(default)]
    provenance: Value,
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn field(input: &Value, key: &str, limit: usize, required: bool) -> Result<String> {
    if input.get(key).is_some_and(|v| !v.is_string()) {
        bail!("{key}: expected text");
    }
    let value = input.get(key).and_then(Value::as_str).unwrap_or("").trim();
    if value.chars().count() > limit
        || value.chars().any(char::is_control)
        || (required && value.is_empty())
    {
        bail!("{key}: invalid text or length");
    }
    Ok(value.into())
}
fn profile_path(engine: &Engine, id: &str) -> Result<std::path::PathBuf> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        bail!("invalid profile id");
    }
    Ok(engine.data.join("profiles").join(format!("{id}.json")))
}
fn read_profile(engine: &Engine, id: &str) -> Result<Profile> {
    let value: Profile = serde_json::from_str(&files::read(&profile_path(engine, id)?)?)?;
    if value.id != id || files::hash(value.content.as_bytes()) != value.revision {
        bail!("profile content or identity changed");
    }
    config::parse(&value.content, "json")?;
    provenance(&json!({"content":value.content,"provenance":value.provenance}))?;
    field(&json!({"name":value.name}), "name", 64, true)?;
    Ok(value)
}
pub fn profiles(engine: &Engine) -> Result<Value> {
    let directory = engine.data.join("profiles");
    let mut profiles = Vec::new();
    let mut invalid = Vec::new();
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            if profiles.len() + invalid.len() >= 12 {
                bail!("profile limit exceeded");
            }
            let id = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
            profile_path(engine, id)?;
            match read_profile(engine, id) {
                Ok(p) => profiles.push(
                    json!({"id":p.id,"name":p.name,"created":p.created,"revision":p.revision,"provenance":p.provenance}),
                ),
                Err(error) => invalid.push(json!({"id":id,"error":format!("{error:#}")})),
            }
        }
    }
    profiles.sort_by_key(|p| {
        (
            std::cmp::Reverse(p["created"].as_u64()),
            p["id"].as_str().unwrap_or("").to_owned(),
        )
    });
    Ok(json!({"profiles":profiles,"invalid_profiles":invalid}))
}
pub fn save_profile(engine: &Engine, input: &Value) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    let existing = profiles(engine)?;
    if ["profiles", "invalid_profiles"]
        .iter()
        .map(|key| existing[key].as_array().map_or(0, Vec::len))
        .sum::<usize>()
        >= 12
    {
        bail!("at most 12 profiles can be saved");
    }
    let name = field(input, "name", 64, true)?;
    let entries = config::parse(text(input, "content")?, "auto")?;
    let content = config::serialize(&entries, "json")?;
    config::parse(&content, "json")?;
    let id = files::hash(
        format!(
            "{}:{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        )
        .as_bytes(),
    );
    let profile = Profile {
        id: id.clone(),
        name,
        created: now()?,
        revision: files::hash(content.as_bytes()),
        content,
        provenance: provenance(input)?,
    };
    files::private_dir(&engine.data.join("profiles"))?;
    if serde_json::to_vec_pretty(&profile)?.len() > 128 * 1024 {
        bail!("profile exceeds 128 KiB");
    }
    files::write_json(
        &profile_path(engine, &id)?,
        &serde_json::to_value(&profile)?,
    )?;
    profiles(engine)
}
pub fn preview_profile(engine: &Engine, input: &Value) -> Result<Value> {
    let profile = read_profile(engine, text(input, "id")?)?;
    let mut result = config::preview(engine, &json!({"content":profile.content,"format":"json"}))?;
    if !profile.provenance.is_null() {
        result["provenance"] = profile.provenance;
    }
    Ok(result)
}
fn provenance(input: &Value) -> Result<Value> {
    let Some(value) = input.get("provenance").filter(|v| !v.is_null()) else {
        return Ok(Value::Null);
    };
    if value["source"] == crate::pixel_archive::SOURCE {
        return crate::pixel_archive::provenance(
            value,
            &config::parse(text(input, "content")?, "auto")?,
        );
    }
    if value["source"] != "https://flash.android.com/"
        || !value["fetched"].is_u64()
        || value["patch_known"] != false
        || value.get("identity_bound").is_some_and(|v| !v.is_boolean())
    {
        bail!("invalid fingerprint provenance");
    }
    if value["identity_bound"] == true
        || ["fingerprint", "model", "first_api_level", "incremental"]
            .iter()
            .any(|key| value.get(key).is_some())
    {
        return crate::pixel::provenance(value, &config::parse(text(input, "content")?, "auto")?);
    }
    let device = field(value, "device", 80, true)?;
    let build = field(value, "build", 80, true)?;
    let entries = config::parse(text(input, "content")?, "auto")?;
    if entries.get("DEVICE") != Some(&device) || entries.get("ID") != Some(&build) {
        bail!("provenance does not match the configuration");
    }
    Ok(
        json!({"source":"https://flash.android.com/","device":device,"build":build,"fetched":value["fetched"],"patch_known":false,"identity_bound":false}),
    )
}
pub fn delete_profile(engine: &Engine, input: &Value) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    fs::remove_file(profile_path(engine, text(input, "id")?)?)?;
    fs::File::open(engine.data.join("profiles"))?.sync_all()?;
    profiles(engine)
}

#[derive(Serialize, Deserialize)]
struct TestRecord {
    id: String,
    created: u64,
    app: String,
    version: String,
    note: String,
    results: [String; 3],
    revision: String,
    model: String,
    fingerprint: String,
    enabled: bool,
    mode: String,
    #[serde(default)]
    environment: Value,
    #[serde(default)]
    report: Value,
    #[serde(default)]
    installation_source: String,
}
fn read_records(engine: &Engine) -> Result<Vec<TestRecord>> {
    let raw = files::read(&engine.data.join("test-records.json"))?;
    let records: Vec<TestRecord> = if raw.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&raw)?
    };
    if records.len() > 24 {
        bail!("test record limit exceeded");
    }
    Ok(records)
}
pub fn records(engine: &Engine) -> Result<Value> {
    Ok(json!({"records":read_records(engine)?,"user_reported":true,"verdict_verified":false}))
}
pub fn save_record(
    engine: &Engine,
    input: &Value,
    settings: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    save_observation(engine, input, settings, Value::Null)
}
pub fn import_record(
    engine: &Engine,
    input: &Value,
    settings: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    let value = crate::reports::observation(input)?;
    save_observation(engine, &value, settings, value["report"].clone())
}
fn save_observation(
    engine: &Engine,
    input: &Value,
    settings: impl FnOnce() -> Result<Value>,
    report: Value,
) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    let app = field(input, "app", 80, true)?;
    let version = field(input, "version", 40, false)?;
    let note = field(input, "note", 240, false)?;
    let installation_source = field(input, "installation_source", 80, false)?;
    let mut results = [String::new(), String::new(), String::new()];
    for (i, key) in ["basic", "device", "strong"].iter().enumerate() {
        let value = text(input, key)?;
        if !["unknown", "pass", "fail", "omitted"].contains(&value) {
            bail!("invalid reported result: {key}");
        }
        results[i] = value.into();
    }
    let current = config::current(engine)?;
    if input["revision"] != current["revision"] {
        bail!("configuration changed; refresh before recording a test");
    }
    let options = settings()?;
    let mut previous = read_records(engine)?;
    previous.insert(
        0,
        TestRecord {
            id: files::hash(
                format!(
                    "{}:{}",
                    std::process::id(),
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
                )
                .as_bytes(),
            ),
            created: now()?,
            app,
            version,
            note,
            results,
            revision: text(&current, "revision")?.into(),
            model: current["entries"]["MODEL"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(128)
                .collect(),
            fingerprint: current["entries"]["FINGERPRINT"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(1024)
                .collect(),
            enabled: options["enabled"] == true,
            mode: text(&options, "mode")?.into(),
            environment: crate::test_environment::capture(engine)?,
            report,
            installation_source,
        },
    );
    previous.truncate(24);
    while serde_json::to_vec_pretty(&previous)?.len() > 128 * 1024 {
        previous.pop();
    }
    files::write_json(
        &engine.data.join("test-records.json"),
        &serde_json::to_value(previous)?,
    )?;
    records(engine)
}
pub fn clear_records(engine: &Engine) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    files::write_json(&engine.data.join("test-records.json"), &json!([]))?;
    records(engine)
}
