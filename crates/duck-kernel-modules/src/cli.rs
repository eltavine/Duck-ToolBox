use std::io::{self, Read};

use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use duck_core::{BoxFuture, ClapFeature, CommandResult, Context, FeatureInfo, IntoCommandResult};
use serde::Deserialize;

use crate::{ModuleError, code_of, files, inventory, mutation::Request, worker};

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "kernel-modules",
        summary: "Linux kernel module inventory, .ko inspection and runtime loading/unloading",
        contract: 2,
    },
    dispatch,
);

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Kernel capabilities and modules visible through procfs and sysfs.
    Status,
    /// Runtime metadata, holders and readable parameters of one module.
    Details {
        #[arg(allow_hyphen_values = true)]
        name: String,
    },
    /// Find uncompressed .ko files in standard Android module directories.
    Files,
    /// Inspect an uncompressed ELF kernel module without executing it.
    Inspect { path: String },
    /// Load an inspected module into the running kernel (no force flags).
    Load(JsonArgs),
    /// Unload an unused module from the running kernel (no force flags).
    Unload(JsonArgs),
    #[command(hide = true)]
    Worker(JsonArgs),
}

#[derive(Debug, Args)]
pub struct JsonArgs {
    #[arg(long, required = true)]
    stdin_json: bool,
}

fn read_json<T: for<'a> Deserialize<'a>>(args: JsonArgs, limit: u64) -> Result<T> {
    ensure!(
        args.stdin_json,
        ModuleError::InvalidInput("JSON input required".into())
    );
    let mut bytes = Vec::new();
    io::stdin().take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        ModuleError::InvalidInput("JSON input exceeds command size limit".into())
    );
    serde_json::from_slice(&bytes).map_err(|e| ModuleError::InvalidInput(e.to_string()).into())
}

fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move {
        match command {
            Command::Status => {
                inventory::status(&ctx.sysroot).into_command("kernel-modules.status", code_of)
            }
            Command::Details { name } => inventory::details(&ctx.sysroot, &name)
                .into_command("kernel-modules.details", code_of),
            Command::Files => {
                files::catalog(&ctx.sysroot).into_command("kernel-modules.files", code_of)
            }
            Command::Inspect { path } => files::inspect(&ctx.sysroot, &path)
                .map(|(info, _)| info)
                .into_command("kernel-modules.inspect", code_of),
            Command::Load(args) => read_json(args, 16_384)
                .map(Request::Load)
                .and_then(|request| worker::execute(ctx, &request))
                .into_command("kernel-modules.load", code_of),
            Command::Unload(args) => read_json(args, 16_384)
                .map(Request::Unload)
                .and_then(|request| worker::execute(ctx, &request))
                .into_command("kernel-modules.unload", code_of),
            Command::Worker(args) => read_json(args, 32_768)
                .and_then(|request| crate::mutation::perform(ctx, request))
                .into_command("kernel-modules.worker", code_of),
        }
    })
}
