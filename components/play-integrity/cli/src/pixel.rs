// SPDX-License-Identifier: GPL-3.0-only
//! Pixel release metadata; candidates are constructed locally and never applied here.
use crate::{Engine, config, files, network};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOURCE: &str = "https://flash.android.com/";
const DEVICES: &[(&str, &str, u32)] = &[
    ("bluejay", "Pixel 6a", 32),
    ("oriole", "Pixel 6", 31),
    ("raven", "Pixel 6 Pro", 31),
    ("panther", "Pixel 7", 33),
    ("cheetah", "Pixel 7 Pro", 33),
    ("lynx", "Pixel 7a", 33),
    ("shiba", "Pixel 8", 34),
    ("husky", "Pixel 8 Pro", 34),
    ("akita", "Pixel 8a", 34),
    ("tokay", "Pixel 9", 34),
    ("caiman", "Pixel 9 Pro", 34),
    ("komodo", "Pixel 9 Pro XL", 34),
    ("comet", "Pixel 9 Pro Fold", 34),
    ("tegu", "Pixel 9a", 35),
];
fn device(input: &Value) -> Result<(&'static str, &'static str, u32)> {
    let id = match input.get("device") {
        None => "bluejay",
        Some(value) => value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("device must be text"))?,
    };
    DEVICES
        .iter()
        .copied()
        .find(|d| d.0 == id)
        .ok_or_else(|| anyhow::anyhow!("unsupported Pixel device"))
}
pub fn devices() -> Value {
    json!({"default":"bluejay","devices":DEVICES.iter().map(|(id,name,_)|json!({"id":id,"name":name})).collect::<Vec<_>>()})
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
fn fingerprint(product: &str, id: &str, incremental: &str, release: &str) -> Option<String> {
    (token(id)
        && incremental.parse::<u64>().is_ok()
        && incremental.bytes().all(|c| c.is_ascii_digit())
        && token(release)
        && release.parse::<u32>().is_ok_and(|v| v > 0))
    .then(|| format!("google/{product}/{product}:{release}/{id}/{incremental}:user/release-keys"))
}
fn parse_builds(raw: &str, product: &str) -> Result<Vec<Value>> {
    let data: Value = serde_json::from_str(raw)?;
    let mut builds = Vec::new();
    for build in data["flashstationBuild"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing Pixel builds"))?
    {
        if build["product"] != product
            || build["target"] != format!("{product}-user")
            || !build["releaseBuildMetadata"].is_object()
        {
            continue;
        }
        let id = build["releaseCandidateName"].as_str().unwrap_or("");
        let incremental = build["buildId"].as_str().unwrap_or("");
        let version = build["version"].as_str().unwrap_or("");
        let release = version.strip_suffix(".0.0").unwrap_or(version);
        let Some(fingerprint) = fingerprint(product, id, incremental, release) else {
            continue;
        };
        let notes = build["releaseBuildMetadata"]["notes"]
            .as_str()
            .unwrap_or("");
        if notes.len() > 160 || notes.chars().any(char::is_control) {
            continue;
        }
        builds.push(
            json!({"id":id,"incremental":incremental,"release":release,"notes":notes,
            "fingerprint":fingerprint}),
        );
    }
    builds.sort_by_key(|b| {
        std::cmp::Reverse(
            b["incremental"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
        )
    });
    builds.dedup_by(|a, b| a["fingerprint"] == b["fingerprint"]);
    builds.truncate(40);
    if builds.is_empty() {
        bail!("no supported Pixel release builds were returned");
    }
    Ok(builds)
}
pub async fn builds(engine: &Engine, input: &Value) -> Result<Value> {
    let (product, model, _) = device(input)?;
    let client = network::client(Duration::from_secs(5))?;
    let html = network::body(client.get(SOURCE).send().await?, 2 * 1024 * 1024).await?;
    // This is the public web client's key, obtained at request time like the bundled generator.
    let start = html
        .find("AIza")
        .ok_or_else(|| anyhow::anyhow!("Pixel metadata client configuration unavailable"))?;
    let key: String = html[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(80)
        .collect();
    if key.len() != 39 {
        bail!("unsupported Pixel metadata client configuration");
    }
    let mut url = reqwest::Url::parse("https://content-flashstation-pa.googleapis.com/v1/builds")?;
    url.query_pairs_mut()
        .extend_pairs([("product", product), ("key", &key)]);
    let response = client
        .get(url)
        .header("Referer", SOURCE)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Pixel build metadata request failed"))?;
    let builds = parse_builds(&network::body(response, 2 * 1024 * 1024).await?, product)?;
    let fetched = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let result =
        json!({"device":product,"model":model,"source":SOURCE,"fetched":fetched,"builds":builds});
    let _lock = files::Lock::acquire(&engine.data)?;
    files::write_json(&engine.data.join(format!("pixel-{product}.json")), &result)?;
    Ok(result)
}
pub fn preview(engine: &Engine, input: &Value) -> Result<Value> {
    let (product, model, first_api) = device(input)?;
    let cached = files::json_file(&engine.data.join(format!("pixel-{product}.json")))?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if cached["device"] != product
        || cached["source"] != SOURCE
        || !cached["fetched"]
            .as_u64()
            .is_some_and(|t| t <= now && now - t <= 86400)
    {
        bail!("Pixel catalog expired; fetch builds again");
    }
    let build = cached["builds"]
        .as_array()
        .and_then(|list| list.iter().find(|b| b["id"] == input["build"]))
        .ok_or_else(|| anyhow::anyhow!("select a fetched Pixel build"))?;
    let expected = fingerprint(
        product,
        crate::text(build, "id")?,
        crate::text(build, "incremental")?,
        crate::text(build, "release")?,
    )
    .ok_or_else(|| anyhow::anyhow!("invalid cached Pixel build; fetch again"))?;
    if build["fingerprint"] != expected {
        bail!("cached Pixel fingerprint does not match its build; fetch again");
    }
    let mut entries = config::parse(
        &format!(
            "MANUFACTURER=Google\nMODEL={model}\nFINGERPRINT={}\nDEVICE_INITIAL_SDK_INT={first_api}\n",
            crate::text(build, "fingerprint")?
        ),
        "prop",
    )?;
    config::migrate(&mut entries, true, false);
    let mut result = config::identity_preview(engine, entries)?;
    result["provenance"] = json!({"source":SOURCE,"device":product,"build":build["id"],"fetched":cached["fetched"],"patch_known":false});
    result["warnings"]
        .as_array_mut()
        .unwrap()
        .push(json!({"code":"pixel_patch_unknown","fields":[]}));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_release_builds_for_selected_product_are_accepted() {
        let raw = json!({"flashstationBuild":[
            {"product":"bluejay","target":"bluejay-user","releaseCandidateName":"ID.1","buildId":"10","version":"16.0.0","releaseBuildMetadata":{"notes":""}},
            {"product":"bluejay","target":"bluejay-userdebug","releaseCandidateName":"BAD","buildId":"99","version":"16.0.0","releaseBuildMetadata":{}},
            {"product":"lynx","target":"lynx-user","releaseCandidateName":"BAD","buildId":"99","version":"16.0.0","releaseBuildMetadata":{}}
        ]});
        let result = parse_builds(&raw.to_string(), "bluejay").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0]["fingerprint"],
            "google/bluejay/bluejay:16/ID.1/10:user/release-keys"
        );
        assert_eq!(device(&json!({})).unwrap().0, "bluejay");
        assert!(device(&json!({"device":"../x"})).is_err());
        assert!(device(&json!({"device":false})).is_err());
    }
}
