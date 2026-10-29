//! The contract every tool implements to plug into the `duckd` binary.
//!
//! A feature owns one top-level subcommand tree (`duckd <id> ...`). The binary only keeps a
//! list of features, builds the CLI from them and dispatches by id, so adding a tool never
//! touches another tool's code.

use std::{future::Future, marker::PhantomData, pin::Pin};

use clap::{ArgMatches, Subcommand};
use serde::Serialize;

use crate::command::{CommandFailure, CommandResult, Context};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Static description of a feature, reported by `duckd features`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FeatureInfo {
    /// Stable id: the CLI subcommand, the envelope `command` prefix and the WebUI route.
    pub id: &'static str,
    pub summary: &'static str,
    /// Version of this feature's JSON data contract. Bump it on a breaking change so the
    /// WebUI can hide a tool it no longer understands instead of misreading it.
    pub contract: u32,
}

pub trait Feature: Sync {
    fn info(&self) -> FeatureInfo;

    /// The clap command for `duckd <id>`, including every subcommand.
    fn command(&self) -> clap::Command;

    /// Foreground terminals and namespace supervisors must run before the async
    /// runtime creates threads. This path owns its output and returns an exit code.
    fn foreground(&self, _matches: &ArgMatches, _ctx: &Context) -> Option<i32> {
        None
    }

    fn run<'a>(&'a self, matches: &'a ArgMatches, ctx: &'a Context)
    -> BoxFuture<'a, CommandResult>;

    /// A short, cheap-to-compute state summary for the root manager's module list, or
    /// `None` when the feature has nothing worth showing there.
    fn status_line(&self, _ctx: &Context) -> Option<String> {
        None
    }
}

pub type Handler<C> = for<'a> fn(C, &'a Context) -> BoxFuture<'a, CommandResult>;
pub type StatusLine = fn(&Context) -> Option<String>;

/// Adapts a clap-derived [`Subcommand`] enum and its handler into a [`Feature`].
pub struct ClapFeature<C> {
    info: FeatureInfo,
    handler: Handler<C>,
    status: Option<StatusLine>,
    _command: PhantomData<fn() -> C>,
}

impl<C> ClapFeature<C> {
    pub const fn new(info: FeatureInfo, handler: Handler<C>) -> Self {
        Self {
            info,
            handler,
            status: None,
            _command: PhantomData,
        }
    }

    pub const fn with_status(mut self, status: StatusLine) -> Self {
        self.status = Some(status);
        self
    }
}

impl<C: Subcommand> Feature for ClapFeature<C> {
    fn info(&self) -> FeatureInfo {
        self.info
    }

    fn status_line(&self, ctx: &Context) -> Option<String> {
        self.status.and_then(|status| status(ctx))
    }

    fn command(&self) -> clap::Command {
        C::augment_subcommands(
            clap::Command::new(self.info.id)
                .about(self.info.summary)
                .subcommand_required(true)
                .arg_required_else_help(true),
        )
    }

    fn run<'a>(
        &'a self,
        matches: &'a ArgMatches,
        ctx: &'a Context,
    ) -> BoxFuture<'a, CommandResult> {
        match C::from_arg_matches(matches) {
            Ok(command) => (self.handler)(command, ctx),
            Err(error) => {
                let failure = CommandFailure::new(
                    self.info.id,
                    "usage_error",
                    &anyhow::Error::new(error),
                    None,
                );
                Box::pin(std::future::ready(Err(failure)))
            }
        }
    }
}

/// Wraps a synchronous handler result for [`Handler`].
pub fn ready<'a>(result: CommandResult) -> BoxFuture<'a, CommandResult> {
    Box::pin(std::future::ready(result))
}

#[cfg(test)]
mod tests {
    use clap::Subcommand;
    use serde_json::json;

    use super::{BoxFuture, ClapFeature, Feature, FeatureInfo, ready};
    use crate::{AppPaths, CommandOutput, CommandResult, Context, Sysroot};

    #[derive(Debug, Subcommand)]
    enum Demo {
        Echo { value: String },
    }

    fn handle<'a>(command: Demo, _ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
        let Demo::Echo { value } = command;
        ready(CommandOutput::new("demo.echo", json!({ "value": value })))
    }

    static DEMO: ClapFeature<Demo> = ClapFeature::new(
        FeatureInfo {
            id: "demo",
            summary: "Demo feature",
            contract: 1,
        },
        handle,
    );

    #[test]
    fn clap_feature_parses_and_dispatches() {
        let root = std::env::temp_dir();
        let ctx = Context {
            paths: AppPaths::for_root(&root, &root),
            sysroot: Sysroot::default(),
        };
        let matches = DEMO
            .command()
            .try_get_matches_from(["demo", "echo", "quack"])
            .unwrap();
        let output = block_on_ready(DEMO.run(&matches, &ctx)).unwrap();

        assert_eq!(output.command, "demo.echo");
        assert_eq!(output.data["value"], "quack");
    }

    fn block_on_ready<T>(future: BoxFuture<'_, T>) -> T {
        use std::task::{Context as TaskContext, Poll, Waker};
        let mut future = future;
        let mut cx = TaskContext::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("demo handler must complete synchronously"),
        }
    }
}
