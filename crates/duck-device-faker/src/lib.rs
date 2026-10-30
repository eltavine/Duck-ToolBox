//! Device Faker management through its public process protocol.
mod process;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use duck_core::{
    BoxFuture, ClapFeature, CommandFailure, CommandOutput, CommandResult, Context, FeatureInfo,
};
use serde_json::{Value, json};
use std::io::Read;

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "device-faker",
        summary: "Per-application device profiles and templates",
        contract: 2,
    },
    dispatch,
);
fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move { run(command, ctx) })
}
#[derive(Debug, Args)]
pub struct Input {
    #[arg(long)]
    stdin_json: bool,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    Status,
    Config,
    Preview(Input),
    ImportPreview,
    RestorePreview,
    Apply(Input),
    Restore(Input),
    Enable(Input),
    Disable,
}
fn run(command: Command, ctx: &Context) -> CommandResult {
    let (operation, input) = match command {
        Command::Status => ("status", None),
        Command::Config => ("config", None),
        Command::Preview(v) => ("preview", Some(v)),
        Command::ImportPreview => ("import-preview", None),
        Command::RestorePreview => ("restore-preview", None),
        Command::Apply(v) => ("apply", Some(v)),
        Command::Restore(v) => ("restore", Some(v)),
        Command::Enable(v) => ("enable", Some(v)),
        Command::Disable => ("disable", None),
    };
    let label = match operation {
        "status" => "device-faker.status",
        "config" => "device-faker.config",
        "preview" => "device-faker.preview",
        "import-preview" => "device-faker.import-preview",
        "apply" => "device-faker.apply",
        "restore" => "device-faker.restore",
        "restore-preview" => "device-faker.restore-preview",
        "enable" => "device-faker.enable",
        "disable" => "device-faker.disable",
        _ => unreachable!(),
    };
    let result = (|| -> Result<Value> {
        let input = if let Some(input) = input {
            if !input.stdin_json {
                bail!("JSON input requires --stdin-json");
            }
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(128 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 128 * 1024 {
                bail!("input exceeds 128 KiB");
            }
            serde_json::from_slice(&bytes)?
        } else {
            json!({})
        };
        process::request(ctx, operation, &input)
    })();
    match result {
        Ok(data) => CommandOutput::new(label, data),
        Err(error) => {
            let code = if ctx
                .paths
                .root
                .join("device-faker/device-faker-engine")
                .is_file()
            {
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
