// SPDX-License-Identifier: GPL-3.0-only
// Play Integrity Fork derivative; see the engine LICENSE and UPSTREAM.json.
// Modified 2026-10-24: independent CLI and versioned process interface.
mod activation;
mod command;
mod config;
mod denylist;
mod files;
mod inspection;
mod library;
mod network;
mod observation;
mod operations;
mod paths;
mod pixel;
mod pixel_archive;
mod power;
mod reports;
mod rules;
mod selinux;
mod shutdown;
mod stages;
mod tasks;
mod test_environment;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    version,
    about = "Play Integrity engine: standalone configuration and runtime tools (GPLv3)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Exit successfully when this engine owns the current sensitive-property handler.
    Active,
    /// Operations: status, config, preview, apply, restore, options, task-start,
    /// task-status, task-cancel, restart, diagnose. Optional JSON input on stdin.
    Request { operation: String },
    /// Run an installation, early-boot, late-boot or uninstall lifecycle stage.
    Stage { phase: String },
    /// Safely migrate a file without evaluating configuration as shell code.
    MigrateFile {
        source: PathBuf,
        output: PathBuf,
        #[arg(long, default_value = "prop")]
        format: String,
        #[arg(long)]
        advanced: bool,
        #[arg(long)]
        force: bool,
    },
    /// HTTPS fetch adapter for the bundled fingerprint generator.
    Fetch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(hide = true)]
    Worker { id: String },
}

#[derive(Clone)]
pub struct Engine {
    pub data: PathBuf,
    pub module: PathBuf,
    pub sysroot: PathBuf,
}

impl Engine {
    fn discover() -> Result<Self> {
        let sysroot = std::env::var_os("PIF_ENGINE_SYSROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let module = std::env::var_os("PIF_ENGINE_MODULE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/data/adb/modules/duck-toolbox/play-integrity"));
        let data = std::env::var_os("PIF_ENGINE_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| sysroot.join("data/adb/play-integrity"));
        if !data.is_absolute() || !module.is_absolute() || !sysroot.is_absolute() {
            bail!("engine paths must be absolute");
        }
        Ok(Self {
            data,
            module,
            sysroot,
        })
    }
    pub fn shell(&self) -> std::process::Command {
        for path in [
            "/data/adb/ksu/bin/busybox",
            "/data/adb/ap/bin/busybox",
            "/data/adb/magisk/busybox",
        ] {
            let path = self.device(path);
            if path.is_file() {
                let mut shell = std::process::Command::new(path);
                shell.arg("sh").env("ASH_STANDALONE", "1");
                return shell;
            }
        }
        std::process::Command::new("sh")
    }
    pub fn device(&self, path: &str) -> PathBuf {
        self.sysroot.join(path.trim_start_matches('/'))
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Active) {
        let active = Engine::discover().and_then(|engine| operations::active(&engine));
        std::process::exit(if active.unwrap_or(false) { 0 } else { 1 });
    }
    if let Command::Fetch { args } = cli.command {
        if let Err(error) = network::fetch(&args).await {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return;
    }
    let result = async {
        let engine = Engine::discover()?;
        match cli.command {
            Command::Worker { id } => tasks::work(&engine, &id),
            Command::Stage { phase } => operations::stage(&engine, &phase),
            Command::MigrateFile {
                source,
                output,
                format,
                advanced,
                force,
            } => {
                let raw = files::read(&source)?;
                if raw.trim().is_empty() {
                    bail!("migration source is empty or missing: {}", source.display());
                }
                let mut entries = config::parse(&raw, "auto")?;
                config::migrate(&mut entries, advanced, force);
                config::validate(&entries)?;
                if !["prop", "json"].contains(&format.as_str()) {
                    bail!("unsupported output format");
                }
                if output.exists() {
                    files::atomic(
                        &output.with_extension(format!("{format}.bak")),
                        files::read(&output)?.as_bytes(),
                    )?;
                }
                files::atomic(&output, config::serialize(&entries, &format)?.as_bytes())?;
                Ok(json!({"output":output,"warnings":rules::inspect(&entries)}))
            }
            Command::Request { operation } => {
                let mut input = String::new();
                io::stdin()
                    .take(128 * 1024 + 1)
                    .read_to_string(&mut input)?;
                if input.len() > 128 * 1024 {
                    bail!("input exceeds 128 KiB");
                }
                let request = if input.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&input)?
                };
                match operation.as_str() {
                    "pixel-devices" => Ok(pixel::devices()),
                    "pixel-builds" => pixel::builds(&engine, &request).await,
                    "pixel-preview" => pixel::preview(&engine, &request),
                    "pixel-archive-catalog" => pixel_archive::catalog(&engine).await,
                    "pixel-archive-preview" => pixel_archive::preview(&engine, &request).await,
                    _ => operations::run(&engine, &operation, request),
                }
            }
            Command::Fetch { .. } => unreachable!(),
            Command::Active => unreachable!(),
        }
    }
    .await;
    match result {
        Ok(data) => println!("{}", json!({"ok":true,"api":1,"data":data})),
        Err(error) => {
            let mut failure = json!({"code":"engine_error","message":format!("{error:#}")});
            if let Some(details) = error.downcast_ref::<files::RecoveryError>() {
                failure["details"] = details.0.clone();
            }
            println!("{}", json!({"ok":false,"api":1,"error":failure}));
            std::process::exit(1);
        }
    }
}

pub fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string: {key}"))
}
