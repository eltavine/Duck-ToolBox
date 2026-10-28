use crate::{
    code_of, fail, inventory, mutation,
    store::{BootResult, Store},
    worker,
};
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use duck_core::{BoxFuture, ClapFeature, CommandResult, Context, FeatureInfo, IntoCommandResult};
use serde::Deserialize;
use std::io::{self, Read};

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "zram",
        summary: "ZRAM and swap inventory, runtime configuration and opt-in boot profiles",
        contract: 2,
    },
    dispatch,
);
#[derive(Debug, Subcommand)]
pub enum Command {
    Status,
    Apply(JsonArgs),
    Restore(JsonArgs),
    Save(JsonArgs),
    Forget {
        device: String,
    },
    /// Apply explicitly enabled saved plans, at most once per boot.
    BootApply,
    #[command(hide = true)]
    Worker(JsonArgs),
}
#[derive(Debug, Args)]
pub struct JsonArgs {
    #[arg(long, required = true)]
    stdin_json: bool,
}
fn read_json<T: for<'a> Deserialize<'a>>(args: JsonArgs) -> Result<T> {
    ensure!(
        args.stdin_json,
        fail("invalid_input", "JSON input required")
    );
    let mut bytes = Vec::new();
    io::stdin().take(16_385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16_384,
        fail("invalid_input", "JSON input exceeds limit")
    );
    serde_json::from_slice(&bytes).map_err(|e| fail("invalid_input", e.to_string()))
}
fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move {
        match command {
            Command::Status => (|| -> Result<_> {
                let status = inventory::status(&ctx.sysroot)?;
                let (saved, busy) = Store::snapshot(ctx)?;
                let mut value = serde_json::to_value(status)?;
                value["saved"] = serde_json::to_value(saved)?;
                value["operation_busy"] = busy.into();
                Ok(value)
            })()
            .into_command("zram.status", code_of),
            Command::Apply(args) => {
                run_operation(ctx, args, "apply").into_command("zram.apply", code_of)
            }
            Command::Restore(args) => {
                run_operation(ctx, args, "restore").into_command("zram.restore", code_of)
            }
            Command::Worker(args) => read_json(args)
                .and_then(|request| mutation::perform(ctx, request))
                .into_command("zram.worker", code_of),
            Command::Save(args) => read_json(args)
                .and_then(|request| mutation::save_profile(ctx, request))
                .into_command("zram.save", code_of),
            Command::Forget { device } => {
                mutation::forget_profile(ctx, &device).into_command("zram.forget", code_of)
            }
            Command::BootApply => boot_apply(ctx).into_command("zram.boot-apply", code_of),
        }
    })
}
fn boot_apply(ctx: &Context) -> Result<Vec<BootResult>> {
    ensure!(
        crate::capabilities::supported(&ctx.sysroot),
        fail(
            "unsupported_environment",
            "boot application requires Android ARM64 root"
        )
    );
    let status = inventory::status(&ctx.sysroot)?;
    let store = Store::open(ctx)?;
    let mut state = store.load()?;
    if state.attempted_boot.as_ref() == Some(&status.boot_id) {
        return Ok(state.boot_results);
    }
    let profiles: Vec<_> = state
        .profiles
        .iter()
        .filter(|p| p.apply_at_boot)
        .map(|p| (p.clone(), state.profile_version(&p.device)))
        .collect();
    if profiles.is_empty() {
        return Ok(vec![]);
    }
    state.attempted_boot = Some(status.boot_id.clone());
    state.boot_claimed.clear();
    state.boot_results = profiles
        .iter()
        .map(|(p, _)| BootResult {
            device: p.device.clone(),
            boot_id: status.boot_id.clone(),
            code: "operation_unknown".into(),
            message: "Boot application was scheduled without a reliable completion result".into(),
        })
        .collect();
    // Persist before starting so duplicate boot hooks and crashes cannot repeat a reset.
    store.save(&state)?;
    drop(store);
    for (profile, version) in profiles {
        let result = (|| -> Result<_> {
            let current = inventory::status(&ctx.sysroot)?;
            let d = current
                .devices
                .iter()
                .find(|d| d.name == profile.device)
                .ok_or_else(|| fail("state_changed", "saved device unavailable"))?;
            let mut config = profile.config.clone();
            config.memory_limit = d.stats.memory_limit.unwrap_or(0);
            worker::execute(
                ctx,
                &mutation::Request {
                    device: profile.device.clone(),
                    revision: d.revision.clone(),
                    confirmed: true,
                    boot_profile: Some(profile.clone()),
                    boot_version: Some(version),
                    operation: mutation::Operation::Apply { config },
                },
            )
        })();
        let (code, message) = match result {
            Ok(_) => ("applied".into(), String::new()),
            Err(e) => (code_of(&e).into(), e.to_string()),
        };
        let store = Store::open(ctx)?;
        let mut state = store.load()?;
        if let Some(result) = state
            .boot_results
            .iter_mut()
            .find(|r| r.device == profile.device)
        {
            result.code = code;
            result.message = message;
        }
        store.save(&state)?;
    }
    Store::open(ctx)?.load().map(|s| s.boot_results)
}

fn run_operation(
    ctx: &Context,
    args: JsonArgs,
    expected: &str,
) -> Result<mutation::OperationResult> {
    let request: mutation::Request = read_json(args)?;
    ensure!(
        request.action() == expected,
        fail("invalid_input", "action does not match command")
    );
    worker::execute(ctx, &request)
}
