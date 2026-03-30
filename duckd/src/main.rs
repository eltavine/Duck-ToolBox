mod cli;

use std::io;

use clap::Parser;
use tracing_subscriber::FmtSubscriber;

use cli::{args::Cli, dispatch};
use duckd::runtime::{json_api, paths::AppPaths};

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder().with_writer(io::stderr).finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let cli = Cli::parse();
    let paths = match AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            json_api::emit(&json_api::failure("bootstrap", &error, None));
            return;
        }
    };

    match dispatch::dispatch(cli.command, &paths).await {
        Ok(payload) => emit_payload(&paths, payload, false),
        Err((command, error, details)) => {
            let payload = json_api::failure(command, &error, details);
            emit_payload(&paths, payload, true);
        }
    }
}

fn emit_payload(paths: &AppPaths, payload: serde_json::Value, exit_with_error: bool) {
    json_api::append_log(paths, &payload);
    json_api::emit(&payload);

    if exit_with_error {
        std::process::exit(1);
    }
}
