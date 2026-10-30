// SPDX-License-Identifier: GPL-3.0-only
//! Community Pixel archive transport and conversion into bounded PI candidates.
use crate::{Engine, config, files, network, rules, text};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SOURCE: &str = "https://github.com/VictorGabriel7Dev/build.prop";
const RAW: &str = "https://raw.githubusercontent.com/VictorGabriel7Dev/build.prop";
const CACHE: &str = "pixel-archive.json";
const CHANNELS: &[&str] = &["release", "beta", "canary"];

#[derive(Deserialize, Serialize)]
struct Build {
    latest: String,
    build: String,
    incremental: String,
    security_patch: String,
    android: String,
    vbmeta_digest: String,
}
#[derive(Deserialize, Serialize)]
struct Device {
    model: String,
    first_api_level: u32,
    channels: BTreeMap<String, Build>,
}
#[derive(Deserialize, Serialize)]
struct Index {
    generated: String,
    devices: BTreeMap<String, Device>,
}
#[derive(Deserialize, Serialize)]
struct Catalog {
    source: String,
    source_revision: String,
    fetched: u64,
    #[serde(flatten)]
    index: Index,
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}
fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn boot_digest(value: &str) -> bool {
    hex(value, 64) || hex(value, 128)
}
fn generated_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && value.is_ascii()
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && rules::valid_patch_date(&value[..10])
        && [(11, 13, 23), (14, 16, 59), (17, 19, 59)]
            .into_iter()
            .all(|(start, end, max)| {
                value[start..end].bytes().all(|b| b.is_ascii_digit())
                    && value[start..end]
                        .parse::<u32>()
                        .is_ok_and(|number| number <= max)
            })
}
fn validate_index(index: &Index) -> Result<()> {
    if index.devices.is_empty() || index.devices.len() > 100 || !generated_date(&index.generated) {
        bail!("unsupported Pixel archive index");
    }
    for (id, device) in &index.devices {
        if !token(id)
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || device.model.is_empty()
            || device.model.len() > 128
            || device.model.chars().any(char::is_control)
            || !(1..=100).contains(&device.first_api_level)
            || device.channels.is_empty()
            || device.channels.len() > 3
        {
            bail!("invalid Pixel archive device");
        }
        for (channel, build) in &device.channels {
            let suffix = match channel.as_str() {
                "release" => String::new(),
                "beta" | "canary" => format!("_{channel}"),
                _ => bail!("unsupported Pixel archive channel"),
            };
            if !token(&build.build)
                || build.incremental.is_empty()
                || !build.incremental.bytes().all(|b| b.is_ascii_digit())
                || build.incremental.parse::<u64>().is_err()
                || build.latest != format!("{id}/{id}{suffix}_{}.prop", build.incremental)
                || !token(&build.android)
                || !rules::valid_patch_date(&build.security_patch)
                || !boot_digest(&build.vbmeta_digest)
            {
                bail!("invalid Pixel archive build");
            }
        }
    }
    Ok(())
}

fn feed_revision(raw: &str) -> Result<&str> {
    let revision = raw
        .split_once("<entry>")
        .and_then(|(_, entry)| entry.split_once("</entry>"))
        .and_then(|(entry, _)| entry.split_once("<id>"))
        .and_then(|(_, id)| id.split_once("</id>"))
        .and_then(|(id, _)| id.trim().strip_prefix("tag:github.com,2008:Grit::Commit/"))
        .filter(|id| hex(id, 40))
        .ok_or_else(|| anyhow::anyhow!("unsupported Pixel archive commit feed"))?;
    Ok(revision)
}

pub async fn catalog(engine: &Engine) -> Result<Value> {
    let client = network::client(Duration::from_secs(5))?;
    // Pin both the index and subsequent property downloads to the same commit.
    // The public commit feed does not require a token or consume the REST API quota.
    let reference = network::body(
        client
            .get(format!("{SOURCE}/commits/main.atom"))
            .header("User-Agent", "Duck-ToolBox")
            .send()
            .await?,
        256 * 1024,
    )
    .await?;
    let revision = feed_revision(&reference)?;
    let raw = network::body(
        client
            .get(format!("{RAW}/{revision}/index.json"))
            .send()
            .await?,
        64 * 1024,
    )
    .await?;
    let index: Index = serde_json::from_str(&raw)?;
    validate_index(&index)?;
    let catalog = Catalog {
        source: SOURCE.into(),
        source_revision: revision.into(),
        fetched: now()?,
        index,
    };
    let value = serde_json::to_value(catalog)?;
    let _lock = files::Lock::acquire(&engine.data)?;
    files::write_json(&engine.data.join(CACHE), &value)?;
    Ok(value)
}

fn cached(engine: &Engine, input: &Value) -> Result<Catalog> {
    let catalog: Catalog = serde_json::from_str(&files::read(&engine.data.join(CACHE))?)?;
    let time = now()?;
    if catalog.source != SOURCE
        || !hex(&catalog.source_revision, 40)
        || input["source_revision"] != catalog.source_revision
        || catalog.fetched > time
        || time - catalog.fetched > 86400
    {
        bail!("Pixel archive catalog changed or expired; fetch it again");
    }
    validate_index(&catalog.index)?;
    Ok(catalog)
}

pub async fn preview(engine: &Engine, input: &Value) -> Result<Value> {
    let catalog = cached(engine, input)?;
    let device_id = text(input, "device")?;
    let channel = text(input, "channel")?;
    let device = catalog
        .index
        .devices
        .get(device_id)
        .ok_or_else(|| anyhow::anyhow!("select a fetched Pixel device"))?;
    let build = device
        .channels
        .get(channel)
        .ok_or_else(|| anyhow::anyhow!("select an available Pixel channel"))?;
    if input["build"] != build.build || input["incremental"] != build.incremental {
        bail!("Pixel archive selection changed; fetch it again");
    }
    let client = network::client(Duration::from_secs(5))?;
    let raw = network::body(
        client
            .get(format!(
                "{RAW}/{}/{}",
                catalog.source_revision, build.latest
            ))
            .send()
            .await?,
        64 * 1024,
    )
    .await?;
    candidate(engine, &catalog, device_id, channel, &raw)
}

fn candidate(
    engine: &Engine,
    catalog: &Catalog,
    device_id: &str,
    channel: &str,
    raw: &str,
) -> Result<Value> {
    let device = &catalog.index.devices[device_id];
    let build = &device.channels[channel];
    let mut seen = std::collections::BTreeSet::new();
    for line in raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let key = line
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid archived property"))?
            .0
            .trim();
        if !seen.insert(key) {
            bail!("duplicate archived property: {key}");
        }
    }
    let props = config::parse(raw, "prop")?;
    let get = |key: &str| {
        props
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing archived property: {key}"))
    };
    let fingerprint = get("ro.product.build.fingerprint")?;
    let parts: Vec<_> = fingerprint.split(['/', ':']).collect();
    let product = get("ro.product.product.name")?;
    if parts.len() != 8
        || parts.iter().any(|p| !token(p))
        || parts[0] != "google"
        || parts[1] != product
        || parts[2] != device_id
        || ![device_id.to_owned(), format!("{device_id}_beta")].contains(&product.to_owned())
        || parts[3]
            != get("ro.product.build.version.release_or_codename")
                .or_else(|_| get("ro.product.build.version.release"))?
        || parts[4] != build.build
        || parts[5] != build.incremental
        || parts[6] != "user"
        || parts[7] != "release-keys"
    {
        bail!("archived fingerprint does not match the selected build");
    }
    for (key, expected) in [
        ("ro.product.product.brand", "google"),
        ("ro.product.product.manufacturer", "Google"),
        ("ro.product.product.device", device_id),
        ("ro.product.product.model", device.model.as_str()),
        ("ro.build.id", build.build.as_str()),
        ("ro.product.build.id", build.build.as_str()),
        ("ro.build.version.incremental", build.incremental.as_str()),
        (
            "ro.product.build.version.incremental",
            build.incremental.as_str(),
        ),
        ("ro.build.version.release", build.android.as_str()),
        ("ro.product.build.version.release", build.android.as_str()),
        (
            "ro.build.version.security_patch",
            build.security_patch.as_str(),
        ),
        ("ro.boot.vbmeta.digest", build.vbmeta_digest.as_str()),
    ] {
        if get(key)? != expected {
            bail!("archive index/property mismatch: {key}");
        }
    }
    if get("ro.product.first_api_level")? != device.first_api_level.to_string() {
        bail!("archive launch API mismatch");
    }
    // Import identity and patch fields only; hardware and boot properties stay in the archive.
    let mut entries = config::Entries::from([
        ("MANUFACTURER".into(), "Google".into()),
        ("MODEL".into(), device.model.clone()),
        ("FINGERPRINT".into(), fingerprint.into()),
        ("SECURITY_PATCH".into(), build.security_patch.clone()),
        (
            "DEVICE_INITIAL_SDK_INT".into(),
            device.first_api_level.to_string(),
        ),
    ]);
    for key in [
        "ro.vendor.build.security_patch",
        "ro.vendor.boot_security_patch",
    ] {
        if let Some(patch) = props.get(key) {
            if !rules::valid_patch_date(patch) {
                bail!("invalid archived partition patch date: {key}");
            }
            entries.insert(key.into(), patch.clone());
        }
    }
    config::migrate(&mut entries, true, false);
    let mut result = config::identity_preview(engine, entries)?;
    result["provenance"] = json!({"source":SOURCE,"device":device_id,"build":build.build,
        "fetched":catalog.fetched,"patch_known":true,"source_revision":catalog.source_revision,
        "channel":channel,"incremental":build.incremental,"security_patch":build.security_patch,
        "vbmeta_digest":build.vbmeta_digest,"file_sha256":files::hash(raw.as_bytes()),
        "fingerprint":fingerprint,"model":device.model,"first_api_level":device.first_api_level});
    Ok(result)
}

pub fn provenance(value: &Value, entries: &config::Entries) -> Result<Value> {
    if value["source"] != SOURCE
        || value["patch_known"] != true
        || !value["fetched"].is_u64()
        || !hex(text(value, "source_revision")?, 40)
        || !hex(text(value, "file_sha256")?, 64)
        || !boot_digest(text(value, "vbmeta_digest")?)
        || !CHANNELS.contains(&text(value, "channel")?)
    {
        bail!("invalid Pixel archive provenance");
    }
    let mut normalized = json!({"source":SOURCE,"patch_known":true,"fetched":value["fetched"]});
    for key in [
        "source_revision",
        "file_sha256",
        "vbmeta_digest",
        "channel",
        "device",
        "build",
        "incremental",
        "security_patch",
    ] {
        let field = text(value, key)?;
        if field.len() > if key == "vbmeta_digest" { 128 } else { 80 } {
            bail!("invalid archive provenance field");
        }
        normalized[key] = json!(field);
    }
    for (field, key) in [
        ("DEVICE", "device"),
        ("ID", "build"),
        ("INCREMENTAL", "incremental"),
        ("SECURITY_PATCH", "security_patch"),
        ("FINGERPRINT", "fingerprint"),
        ("MODEL", "model"),
    ] {
        if entries.get(field).map(String::as_str) != Some(text(value, key)?) {
            bail!("archive provenance does not match the configuration");
        }
    }
    let first_api = value["first_api_level"]
        .as_u64()
        .filter(|v| (1..=100).contains(v))
        .ok_or_else(|| anyhow::anyhow!("invalid archive launch API"))?;
    if entries.get("DEVICE_INITIAL_SDK_INT") != Some(&first_api.to_string()) {
        bail!("archive provenance does not match the launch API");
    }
    for (field, property) in rules::BUILD_PROPERTIES {
        if let Some(prop) = entries.get(*property)
            && entries.get(*field) != Some(prop)
        {
            bail!("archive snapshot has inconsistent paired fields: {field}");
        }
    }
    normalized["fingerprint"] = json!(text(value, "fingerprint")?);
    normalized["model"] = json!(text(value, "model")?);
    normalized["first_api_level"] = json!(first_api);
    if !rules::valid_patch_date(text(value, "security_patch")?) {
        bail!("invalid archive patch date");
    }
    Ok(normalized)
}

#[cfg(test)]
#[path = "pixel_archive_tests.rs"]
mod tests;
