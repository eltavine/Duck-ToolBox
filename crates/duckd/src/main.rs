//! `duckd`: every feature crate registered in [`FEATURES`] becomes `duckd <id> ...`, and every
//! invocation prints exactly one JSON envelope on stdout.

mod builtin;

use clap::{Arg, ArgAction, error::ErrorKind};
use duck_core::{
    Context, Feature, Sysroot,
    command::{CommandFailure, CommandResult},
    envelope,
};

static FEATURES: &[&dyn Feature] = &[
    #[cfg(feature = "rkp")]
    &duck_rkp::FEATURE,
    #[cfg(feature = "rkp-device")]
    &duck_rkp_device::FEATURE,
    #[cfg(feature = "tee-attestation")]
    &duck_tee_attestation::FEATURE,
    #[cfg(feature = "device-ids")]
    &duck_device_ids::FEATURE,
    #[cfg(feature = "tricky-store")]
    &duck_tricky_store::FEATURE,
    #[cfg(feature = "system")]
    &duck_system::FEATURE,
];

fn cli() -> clap::Command {
    clap::Command::new("duckd")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Duck ToolBox backend. Every subcommand prints exactly one JSON envelope on stdout.")
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Accepted for compatibility; output is always JSON"),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommands(builtin::subcommands())
        .subcommands(FEATURES.iter().map(|feature| feature.command()))
}

#[tokio::main]
async fn main() {
    let matches = match cli().try_get_matches() {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error.exit()
        }
        Err(error) => {
            let failure = CommandFailure::new(
                "bootstrap",
                "usage_error",
                &anyhow::anyhow!(error.render().to_string()),
                None,
            );
            envelope::emit(&envelope::failure(&failure));
            std::process::exit(2);
        }
    };

    let paths = match duck_core::AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            let failure = CommandFailure::new("bootstrap", "bootstrap_error", &error, None);
            envelope::emit(&envelope::failure(&failure));
            std::process::exit(1);
        }
    };
    let ctx = Context {
        paths,
        sysroot: Sysroot::from_env(),
    };

    let result = match matches.subcommand() {
        Some((builtin::MANIFEST, _)) => builtin::manifest(FEATURES),
        Some((builtin::DESCRIBE, _)) => builtin::describe(FEATURES, &ctx),
        Some((id, sub)) => match FEATURES.iter().find(|feature| feature.info().id == id) {
            Some(feature) => feature.run(sub, &ctx).await,
            None => unreachable!("clap only accepts registered subcommands"),
        },
        None => unreachable!("a subcommand is required"),
    };
    emit(&ctx, result);
}

fn emit(ctx: &Context, result: CommandResult) {
    let (value, failed) = match result {
        Ok(output) => (envelope::success(&output), false),
        Err(failure) => (envelope::failure(&failure), true),
    };

    duck_core::log::append(&ctx.paths, &value);
    envelope::emit(&value);

    if failed {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{FEATURES, builtin, cli};

    #[test]
    fn cli_is_consistent() {
        cli().debug_assert();
    }

    #[test]
    fn feature_ids_are_unique_and_do_not_shadow_builtins() {
        let mut ids: Vec<_> = FEATURES.iter().map(|feature| feature.info().id).collect();
        ids.extend([builtin::MANIFEST, builtin::DESCRIBE]);
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }
}
