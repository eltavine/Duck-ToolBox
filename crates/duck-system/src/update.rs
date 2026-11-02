//! In-app updates, ported from Tricky Addon's update manager.
//!
//! The stable channel reads the `updateJson` document from `module.prop`, in the format the
//! KernelSU module guide defines (`versionCode`, `version`, `zipUrl`, `changelog`). The canary
//! channel lists the newest CI artifacts through nightly.link, whose archives are named
//! `<module id>-<version>-<versionCode>-canary.zip`.

use std::io::{Cursor, Read};

use anyhow::{Context as _, Result};
use duck_core::{AppPaths, Context};
use duck_platform::{modules::parse_module_prop, net, root};
use serde::{Deserialize, Serialize};

use crate::{SystemError, info};

const DOCUMENT_LIMIT: usize = 1024 * 1024;
const CHANGELOG_LIMIT: usize = 16 * 1024;
const CANARY_WORKFLOW: &str = "ci";
const CANARY_BRANCH: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Stable,
    Canary,
}

impl Channel {
    fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Canary => "canary",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub channel: Channel,
    pub current_version: String,
    pub current_version_code: u64,
    pub available: bool,
    pub version: Option<String>,
    pub version_code: Option<u64>,
    #[serde(skip)]
    pub zip_url: Option<String>,
    /// Markdown release notes for `version`, when the channel publishes them.
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallData {
    pub version: String,
    pub version_code: u64,
    pub output: String,
    pub reboot_required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateJson {
    version_code: u64,
    version: String,
    zip_url: String,
    #[serde(default)]
    changelog: Option<String>,
}

pub async fn check(paths: &AppPaths, channel: Channel) -> Result<UpdateInfo> {
    let identity = info::module_identity(paths)?;
    let mut info = UpdateInfo {
        channel,
        current_version: identity.version.clone(),
        current_version_code: identity.version_code,
        available: false,
        version: None,
        version_code: None,
        zip_url: None,
        changelog: None,
    };

    match channel {
        Channel::Stable => {
            let url = identity
                .update_json
                .clone()
                .ok_or_else(|| SystemError::Download("module.prop has no updateJson".into()))?;
            let body = net::fetch_text(&url, DOCUMENT_LIMIT)
                .await
                .map_err(|error| SystemError::Download(format!("{error:#}")))?;
            let document: UpdateJson =
                serde_json::from_str(&body).context("parse updateJson document")?;
            info.available = document.version_code > identity.version_code;
            if info.available
                && let Some(changelog_url) = &document.changelog
            {
                info.changelog = fetch_changelog(changelog_url, &document.version).await;
            }
            info.version = Some(document.version);
            info.version_code = Some(document.version_code);
            info.zip_url = Some(document.zip_url);
        }
        Channel::Canary => {
            let listing = format!(
                "https://nightly.link/{}/workflows/{CANARY_WORKFLOW}/{CANARY_BRANCH}?preview",
                repository_slug()
            );
            let html = net::fetch_text(&listing, DOCUMENT_LIMIT)
                .await
                .map_err(|error| SystemError::Download(format!("{error:#}")))?;
            if let Some(build) = newest_canary(&html, &identity.id) {
                info.available = build.version_code > identity.version_code;
                info.version = Some(build.version);
                info.version_code = Some(build.version_code);
                info.zip_url = Some(build.url);
            }
        }
    }

    Ok(info)
}

/// Downloads the update the channel currently advertises and hands it to the root manager.
/// The URL always comes from the channel, never from the caller.
pub async fn install(ctx: &Context, channel: Channel) -> Result<InstallData> {
    let manager = root::detect(&ctx.sysroot).ok_or(SystemError::NoRootManager)?;
    let identity = info::module_identity(&ctx.paths)?;
    let info = check(&ctx.paths, channel).await?;
    let (Some(url), Some(version), Some(version_code), true) = (
        info.zip_url,
        info.version,
        info.version_code,
        info.available,
    ) else {
        return Err(SystemError::NoUpdate(channel.name()).into());
    };

    let archive = net::fetch_bytes(&url, net::MAX_DOWNLOAD_BYTES)
        .await
        .map_err(|error| SystemError::Download(format!("{error:#}")))?;
    check_package(&archive, &identity.id)?;

    ctx.paths.ensure_runtime_dirs()?;
    let zip_path = ctx.paths.tmp_dir.join(format!("update-{version_code}.zip"));
    duck_core::fs::write_bytes_atomic(&zip_path, &archive)?;
    let result = manager.install_module(&zip_path.display().to_string());
    let _ = std::fs::remove_file(&zip_path);

    Ok(InstallData {
        version,
        version_code,
        output: result?,
        reboot_required: true,
    })
}

/// Rejects archives that are not a module zip for this module id.
fn check_package(archive: &[u8], module_id: &str) -> Result<()> {
    let invalid = |reason: String| anyhow::Error::from(SystemError::InvalidPackage(reason));
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|error| invalid(format!("not a zip archive: {error}")))?;
    let mut prop = String::new();
    zip.by_name("module.prop")
        .map_err(|_| invalid("module.prop is missing".into()))?
        .take(64 * 1024)
        .read_to_string(&mut prop)
        .map_err(|error| invalid(format!("module.prop is unreadable: {error}")))?;
    let id = parse_module_prop(&prop).remove("id").unwrap_or_default();
    if id != module_id {
        return Err(invalid(format!(
            "archive is for module `{id}`, not `{module_id}`"
        )));
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct CanaryBuild {
    url: String,
    version: String,
    version_code: u64,
}

/// Picks the newest `<id>-<version>-<code>-canary.zip` link on a nightly.link page.
fn newest_canary(html: &str, module_id: &str) -> Option<CanaryBuild> {
    let prefix = format!("{module_id}-");
    html.split("href=\"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next())
        .filter_map(|url| {
            let file = url.rsplit('/').next()?;
            let middle = file
                .strip_suffix("-canary.zip")?
                .strip_prefix(prefix.as_str())?;
            let (version, code) = middle.rsplit_once('-')?;
            Some(CanaryBuild {
                url: url.to_owned(),
                version: version.to_owned(),
                version_code: code.parse().ok()?,
            })
        })
        .max_by_key(|build| build.version_code)
}

async fn fetch_changelog(url: &str, version: &str) -> Option<String> {
    let body = net::fetch_text(url, DOCUMENT_LIMIT).await.ok()?;
    let section = changelog_section(&body, version).unwrap_or(body);
    Some(section.chars().take(CHANGELOG_LIMIT).collect())
}

/// Returns the Markdown section whose heading names `version` (`## v1.2.0`, `### [1.2.0]`,
/// `## v1.2.0 (2026-01-01)`), up to the next heading of the same or a higher level.
fn changelog_section(markdown: &str, version: &str) -> Option<String> {
    let wanted = version.trim().trim_start_matches('v');
    let heading = |line: &str| -> Option<(usize, String)> {
        let level = line.chars().take_while(|ch| *ch == '#').count();
        if level == 0 || level > 6 {
            return None;
        }
        let title = line[level..]
            .trim()
            .trim_start_matches('[')
            .trim_start_matches('v');
        Some((level, title.to_owned()))
    };

    let mut lines = markdown.lines();
    let level = lines.by_ref().find_map(|line| {
        let (level, title) = heading(line)?;
        let rest = title.strip_prefix(wanted)?;
        rest.chars()
            .next()
            .is_none_or(|next| !next.is_ascii_alphanumeric() && next != '.' && next != '-')
            .then_some(level)
    })?;
    let body: Vec<&str> = lines
        .take_while(|line| heading(line).is_none_or(|(next, _)| next > level))
        .collect();
    Some(body.join("\n").trim().to_owned())
}

fn repository_slug() -> &'static str {
    env!("CARGO_PKG_REPOSITORY")
        .trim_start_matches("https://github.com/")
        .trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::{CanaryBuild, changelog_section, check_package, newest_canary, repository_slug};

    #[test]
    fn picks_highest_canary_build_for_this_module() {
        let html = r#"
            <a href="https://nightly.link/o/r/workflows/ci/main/duck-toolbox-v0.2.0-41-canary.zip">a</a>
            <a href="https://nightly.link/o/r/workflows/ci/main/duck-toolbox-v0.2.0-57-canary.zip">b</a>
            <a href="https://nightly.link/o/r/workflows/ci/main/duck-toolbox-v0.2.0-99-canary-verified.zip">verified bundle</a>
            <a href="https://nightly.link/o/r/workflows/ci/main/duckd.zip">c</a>
            <a href="https://nightly.link/o/r/workflows/ci/main/other-v1-99-canary.zip">d</a>
        "#;
        assert_eq!(
            newest_canary(html, "duck-toolbox"),
            Some(CanaryBuild {
                url: "https://nightly.link/o/r/workflows/ci/main/duck-toolbox-v0.2.0-57-canary.zip"
                    .into(),
                version: "v0.2.0".into(),
                version_code: 57,
            })
        );
        assert_eq!(newest_canary("<p>no builds</p>", "duck-toolbox"), None);
    }

    #[test]
    fn extracts_the_matching_changelog_section() {
        let markdown = "# Changelog\n\n## v1.1.0\n- old\n\n## v1.2.0 (2026-09-28)\n- new\n### Details\n- nested\n\n## v1.10.0\n- later\n";
        assert_eq!(
            changelog_section(markdown, "v1.2.0").as_deref(),
            Some("- new\n### Details\n- nested")
        );
        assert_eq!(
            changelog_section(markdown, "1.1.0").as_deref(),
            Some("- old")
        );
        assert_eq!(changelog_section(markdown, "v1.3.0"), None);
    }

    #[test]
    fn package_check_requires_matching_module_id() {
        let zip_with = |prop: &str| {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            writer
                .start_file("module.prop", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(prop.as_bytes()).unwrap();
            writer.finish().unwrap().into_inner()
        };

        check_package(&zip_with("id=duck-toolbox\n"), "duck-toolbox").unwrap();
        assert!(check_package(&zip_with("id=evil\n"), "duck-toolbox").is_err());
        assert!(check_package(b"not a zip", "duck-toolbox").is_err());
    }

    #[test]
    fn repository_slug_comes_from_the_manifest() {
        assert_eq!(repository_slug(), "eltavine/Duck-ToolBox");
    }
}
