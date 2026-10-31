//! Miscellaneous tools, each with its own configuration and lifecycle.

mod language;

use clap::Subcommand;
use duck_core::{BoxFuture, ClapFeature, CommandResult, Context, FeatureInfo, IntoCommandResult};

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "misc",
        summary: "Miscellaneous tools: per-app language spoofing",
        contract: 1,
    },
    dispatch,
);

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Process-local language overrides; never changes the device language.
    Language {
        #[command(subcommand)]
        command: LanguageCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum LanguageCommand {
    Status,
    /// Save rules from stdin, guarded by the status revision. Does not enable injection.
    Save {
        #[arg(long, required = true)]
        stdin_json: bool,
    },
    /// Enable saved rules from stdin: {"revision":"..."}.
    Enable {
        #[arg(long, required = true)]
        stdin_json: bool,
    },
    /// Stop injection in future app processes, including with a corrupt configuration.
    Disable,
}

fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(std::future::ready(match command {
        Command::Language { command } => match command {
            LanguageCommand::Status => {
                language::status(ctx).into_command("misc.language.status", language::code_of)
            }
            LanguageCommand::Save { .. } => {
                language::save(ctx).into_command("misc.language.save", language::code_of)
            }
            LanguageCommand::Enable { .. } => {
                language::enable(ctx).into_command("misc.language.enable", language::code_of)
            }
            LanguageCommand::Disable => {
                language::disable(ctx).into_command("misc.language.disable", language::code_of)
            }
        },
    }))
}
