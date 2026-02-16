mod app;
mod cli;
mod dap;
mod error;
mod http_api;
mod http_utils;
mod output;

use std::env;
use std::path::PathBuf;

use clap::Parser;
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::Cli;
use crate::error::AppError;

fn main() {
    let cli = Cli::parse();
    output::set_json_mode(cli.json);

    if let Err(err) = run(cli) {
        if output::is_json_mode() {
            let payload = json!({
                "ok": false,
                "error": err.code(),
                "message": err.to_string(),
            });
            eprintln!(
                "{}",
                serde_json::to_string(&payload).expect("json error should serialize")
            );
        } else {
            eprintln!("{err}");
        }
        std::process::exit(err.exit_code());
    }
}

fn run(cli: Cli) -> Result<(), AppError> {
    let store = build_store()?;
    app::execute(&store, cli.command)
}

fn build_store() -> Result<StateStore, AppError> {
    let root = env::var("LAUNCH_CODE_HOME")
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    Ok(StateStore::new(root))
}
