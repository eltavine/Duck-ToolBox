use crate::{
    archive, code_of, fail, jobs, lifecycle,
    model::{Target, revision},
    mounts, native, store, supervisor,
};
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use duck_core::{
    BoxFuture, ClapFeature, CommandResult, Context, Feature, FeatureInfo, IntoCommandResult,
};
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::Path,
};

pub struct LinuxFeature;
pub static FEATURE: LinuxFeature = LinuxFeature;
static CLI: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "linux-env",
        summary: "Trusted ARM64 Linux environments, foreground services and boot plans",
        contract: 1,
    },
    dispatch,
);
#[derive(Subcommand)]
enum Command {
    Status,
    Inspect {
        path: String,
    },
    Import(Json),
    Cancel {
        #[arg(long)]
        token: String,
    },
    Start(Json),
    Stop(Json),
    Save(Json),
    Remove(Json),
    Logs {
        #[arg(long)]
        id: String,
    },
    BootApply,
    StopAll,
    /// Enter a running environment with inherited terminal input and output.
    Shell {
        id: String,
    },
    #[command(hide = true)]
    ImportWorker {
        #[arg(long)]
        token: String,
    },
    #[command(hide = true)]
    Supervise {
        #[arg(long)]
        id: String,
        #[arg(long)]
        instance: String,
    },
}
#[derive(Args)]
struct Json {
    #[arg(long, required = true)]
    stdin_json: bool,
}
fn input<T: for<'a> Deserialize<'a>>(args: Json) -> Result<T> {
    ensure!(
        args.stdin_json,
        fail("invalid_input", "stdin JSON required")
    );
    let mut bytes = Vec::new();
    io::stdin().take(524289).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 524288,
        fail("invalid_input", "request exceeds 512 KiB")
    );
    serde_json::from_slice(&bytes).map_err(|e| fail("invalid_input", e.to_string()))
}
impl Feature for LinuxFeature {
    fn info(&self) -> FeatureInfo {
        CLI.info()
    }
    fn command(&self) -> clap::Command {
        CLI.command()
    }
    fn run<'a>(
        &'a self,
        matches: &'a clap::ArgMatches,
        ctx: &'a Context,
    ) -> BoxFuture<'a, CommandResult> {
        CLI.run(matches, ctx)
    }
    fn foreground(&self, matches: &clap::ArgMatches, ctx: &Context) -> Option<i32> {
        let result = match matches.subcommand()? {
            ("shell", args) => lifecycle::shell(ctx, args.get_one::<String>("id")?).map(Some),
            ("supervise", args) => supervisor::run(
                ctx,
                args.get_one::<String>("id")?,
                args.get_one::<String>("instance")?,
            )
            .map(|_| None),
            ("import-worker", args) => {
                jobs::worker(ctx, args.get_one::<String>("token")?).map(|_| None)
            }
            _ => return None,
        };
        Some(match result {
            Ok(code) => code.unwrap_or(0),
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        })
    }
}
fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move {
        match command {
            Command::Status => status(ctx).into_command("linux-env.status", code_of),
            Command::Inspect { path } => {
                archive::inspect(Path::new(&path)).into_command("linux-env.inspect", code_of)
            }
            Command::Import(args) => input(args)
                .and_then(|request| jobs::start(ctx, request))
                .into_command("linux-env.import", code_of),
            Command::Cancel { token } => {
                jobs::cancel(ctx, &token).into_command("linux-env.cancel", code_of)
            }
            Command::Start(args) => input(args)
                .and_then(|request| lifecycle::start(ctx, request))
                .into_command("linux-env.start", code_of),
            Command::Stop(args) => input(args)
                .and_then(|request| lifecycle::stop(ctx, request))
                .into_command("linux-env.stop", code_of),
            Command::Save(args) => input(args)
                .and_then(|request| lifecycle::save(ctx, request))
                .into_command("linux-env.save", code_of),
            Command::Remove(args) => input(args)
                .and_then(|request| lifecycle::delete(ctx, request))
                .into_command("linux-env.remove", code_of),
            Command::Logs { id } => {
                lifecycle::logs(ctx, &id).into_command("linux-env.logs", code_of)
            }
            Command::BootApply => boot(ctx).into_command("linux-env.boot-apply", code_of),
            Command::StopAll => {
                lifecycle::stop_all(ctx).into_command("linux-env.stop-all", code_of)
            }
            _ => Err::<(), _>(fail(
                "invalid_input",
                "foreground command must run before the async runtime",
            ))
            .into_command("linux-env.foreground", code_of),
        }
    })
}
fn status(ctx: &Context) -> Result<serde_json::Value> {
    let base = store::base(ctx);
    let views = store::list(&base)?
        .into_iter()
        .map(|env| lifecycle::view(&base, env))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        serde_json::json!({"supported":native::supported(ctx),"runtime_ready":mounts::helper(&ctx.paths.root.join("bin/tini-arm64")).is_ok(),"busy":store::busy(&base)?,"environments":views,"job":jobs::snapshot(&base)?}),
    )
}
fn boot(ctx: &Context) -> Result<Vec<serde_json::Value>> {
    native::require(ctx)?;
    let mut results = Vec::new();
    for env in store::list(&store::base(ctx))?
        .into_iter()
        .filter(|e| e.config.auto_start)
    {
        let result = lifecycle::start_boot(
            ctx,
            Target {
                id: env.id.clone(),
                revision: revision(&env)?,
                confirmed: true,
            },
        );
        results.push(match result {
            Ok(_) => serde_json::json!({"id":env.id,"code":"starting"}),
            Err(e) => serde_json::json!({"id":env.id,"code":code_of(&e),"message":e.to_string()}),
        });
    }
    Ok(results)
}
