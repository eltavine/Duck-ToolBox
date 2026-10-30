//! MIT management adapter. The separately built GPL engine owns its implementation and state.
//! This crate communicates only over the engine's documented command-line interface.
mod process;
pub use process::request;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use duck_core::{
    BoxFuture, ClapFeature, CommandFailure, CommandOutput, CommandResult, Context, FeatureInfo,
};
use serde_json::{Value, json};
use std::io::{self, Read};

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "play-integrity",
        summary: "Play Integrity engine configuration, fingerprint generation and diagnostics",
        contract: 1,
    },
    dispatch,
);

fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move { run(command, ctx) })
}

#[derive(Debug, Args)]
pub struct Input {
    #[arg(long = "stdin-json")]
    stdin_json: bool,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    Power,
    Disable,
    Status,
    Config,
    Inspect,
    PixelDevices,
    PixelBuilds(Input),
    PixelPreview(Input),
    PixelArchiveCatalog,
    PixelArchivePreview(Input),
    ProfileList,
    ProfileSave(Input),
    ProfilePreview(Input),
    ProfileDelete(Input),
    RecordList,
    RecordSave(Input),
    RecordImport(Input),
    RecordClear,
    Preview(Input),
    ImportPreview(Input),
    Apply(Input),
    Restore(Input),
    Options(Input),
    TaskStart(Input),
    TaskStatus(Input),
    TaskCancel(Input),
    Restart,
    Diagnose,
}

fn run(command: Command, ctx: &Context) -> CommandResult {
    let (operation, label, input) = match command {
        Command::Power => ("power", "play-integrity.power", None),
        Command::Disable => ("disable", "play-integrity.disable", None),
        Command::Status => ("status", "play-integrity.status", None),
        Command::Config => ("config", "play-integrity.config", None),
        Command::Inspect => ("inspect", "play-integrity.inspect", None),
        Command::PixelDevices => ("pixel-devices", "play-integrity.pixel-devices", None),
        Command::PixelBuilds(v) => ("pixel-builds", "play-integrity.pixel-builds", Some(v)),
        Command::PixelPreview(v) => ("pixel-preview", "play-integrity.pixel-preview", Some(v)),
        Command::PixelArchiveCatalog => (
            "pixel-archive-catalog",
            "play-integrity.pixel-archive-catalog",
            None,
        ),
        Command::PixelArchivePreview(v) => (
            "pixel-archive-preview",
            "play-integrity.pixel-archive-preview",
            Some(v),
        ),
        Command::ProfileList => ("profile-list", "play-integrity.profile-list", None),
        Command::ProfileSave(v) => ("profile-save", "play-integrity.profile-save", Some(v)),
        Command::ProfilePreview(v) => {
            ("profile-preview", "play-integrity.profile-preview", Some(v))
        }
        Command::ProfileDelete(v) => ("profile-delete", "play-integrity.profile-delete", Some(v)),
        Command::RecordList => ("record-list", "play-integrity.record-list", None),
        Command::RecordSave(v) => ("record-save", "play-integrity.record-save", Some(v)),
        Command::RecordImport(v) => ("record-import", "play-integrity.record-import", Some(v)),
        Command::RecordClear => ("record-clear", "play-integrity.record-clear", None),
        Command::Preview(v) => ("preview", "play-integrity.preview", Some(v)),
        Command::ImportPreview(v) => ("import-preview", "play-integrity.import-preview", Some(v)),
        Command::Apply(v) => ("apply", "play-integrity.apply", Some(v)),
        Command::Restore(v) => ("restore", "play-integrity.restore", Some(v)),
        Command::Options(v) => ("options", "play-integrity.options", Some(v)),
        Command::TaskStart(v) => ("task-start", "play-integrity.task-start", Some(v)),
        Command::TaskStatus(v) => ("task-status", "play-integrity.task-status", Some(v)),
        Command::TaskCancel(v) => ("task-cancel", "play-integrity.task-cancel", Some(v)),
        Command::Restart => ("restart", "play-integrity.restart", None),
        Command::Diagnose => ("diagnose", "play-integrity.diagnose", None),
    };
    let result = (|| -> Result<Value> {
        let input = if let Some(input) = input {
            read_input(&input)?
        } else {
            json!({})
        };
        process::request(ctx, operation, &input)
    })();
    match result {
        Ok(data) => CommandOutput::new(label, data),
        Err(error) => {
            let code = if ctx.paths.root.join("play-integrity/pif-engine").is_file() {
                "engine_error"
            } else {
                "engine_missing"
            };
            let details = error
                .downcast_ref::<process::EngineError>()
                .map(|e| e.0.clone());
            Err(CommandFailure::new(label, code, &error, details))
        }
    }
}
fn read_input(input: &Input) -> Result<Value> {
    if !input.stdin_json {
        bail!("JSON input requires --stdin-json");
    }
    let mut bytes = Vec::new();
    io::stdin().take(128 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 128 * 1024 {
        bail!("input exceeds 128 KiB");
    }
    Ok(serde_json::from_slice(&bytes)?)
}
