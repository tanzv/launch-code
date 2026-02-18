mod app;
mod cli;
mod dap;
mod error;
mod http_api;
mod http_utils;
mod link_registry;
mod output;

use std::env;
use std::path::PathBuf;

use clap::Parser;
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{Cli, Commands, ProjectCommands};
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
    if should_use_global_link_list(&cli) {
        if let Commands::List(args) = &cli.command {
            return app::execute_global_list(args);
        }
    }
    if should_use_global_project_show(&cli)
        && matches!(
            &cli.command,
            Commands::Project(project_args)
                if matches!(&project_args.command, ProjectCommands::Show)
        )
    {
        return app::execute_global_project_show();
    }

    let store = build_store(&cli)?;
    app::execute(&store, cli.command)
}

fn build_store(cli: &Cli) -> Result<StateStore, AppError> {
    let root = if matches!(&cli.command, Commands::Link(_)) {
        resolve_workspace_root()?
    } else if let Some(link) = cli.link.as_ref() {
        link_registry::resolve_link_path(link)?
    } else if cli.local {
        resolve_workspace_root()?
    } else {
        let workspace_root = resolve_workspace_root()?;
        let link = link_registry::ensure_link_for_workspace(&workspace_root)?;
        PathBuf::from(link.path)
    };
    Ok(StateStore::new(root))
}

fn resolve_workspace_root() -> Result<PathBuf, AppError> {
    Ok(env::var("LAUNCH_CODE_HOME")
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?))
}

fn should_use_global_link_list(cli: &Cli) -> bool {
    if cli.local || cli.link.is_some() {
        return false;
    }
    if env::var_os("LAUNCH_CODE_HOME").is_some() && !cli.global {
        return false;
    }
    matches!(&cli.command, Commands::List(_))
}

fn should_use_global_project_show(cli: &Cli) -> bool {
    if cli.local || cli.link.is_some() {
        return false;
    }
    if env::var_os("LAUNCH_CODE_HOME").is_some() && !cli.global {
        return false;
    }
    matches!(
        &cli.command,
        Commands::Project(project_args)
            if matches!(&project_args.command, ProjectCommands::Show)
    )
}
