mod app;
mod cli;
mod dap;
mod error;
mod http_api;
mod http_utils;
mod link_registry;
mod output;
mod session_lookup;

use std::env;
use std::path::PathBuf;

use clap::Parser;
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{Cli, Commands, DapCommands, DoctorCommands, ProjectCommands};
use crate::error::AppError;

fn main() {
    let cli = Cli::parse();
    output::set_json_mode(cli.json);
    output::set_trace_time_mode(cli.trace_time);

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
        let workspace_root = resolve_workspace_root()?;
        match &cli.command {
            Commands::List(args) => return app::execute_global_list(args, &workspace_root),
            Commands::Running(args) => return app::execute_global_running(&workspace_root, args),
            _ => {}
        }
    }
    if should_use_global_cleanup(&cli) {
        if let Commands::Cleanup(args) = &cli.command {
            let workspace_root = resolve_workspace_root()?;
            return app::execute_global_cleanup(args, &workspace_root);
        }
    }
    if should_use_global_batch_session_control(&cli) {
        let workspace_root = resolve_workspace_root()?;
        match &cli.command {
            Commands::Stop(args) if args.targets_all() => {
                return app::execute_global_stop(args, &workspace_root);
            }
            Commands::Restart(args) if args.targets_all() => {
                return app::execute_global_restart(args, &workspace_root);
            }
            Commands::Suspend(args) if args.targets_all() => {
                return app::execute_global_suspend(args, &workspace_root);
            }
            Commands::Resume(args) if args.targets_all() => {
                return app::execute_global_resume(args, &workspace_root);
            }
            _ => {}
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

    let global_session_fallback_enabled = is_global_session_fallback_enabled(&cli);
    let store = build_store(&cli)?;
    let command = cli.command;
    match app::execute(&store, command.clone()) {
        Ok(()) => Ok(()),
        Err(AppError::SessionNotFound(missing_id))
            if should_use_global_session_fallback(
                global_session_fallback_enabled,
                &command,
                &missing_id,
            ) =>
        {
            if let Some(routed_store) =
                app::resolve_global_store_for_session_id(&missing_id, &store)?
            {
                return app::execute(&routed_store, command);
            }
            Err(AppError::SessionNotFound(missing_id))
        }
        Err(err) => Err(err),
    }
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
    matches!(&cli.command, Commands::List(_) | Commands::Running(_))
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

fn should_use_global_cleanup(cli: &Cli) -> bool {
    if cli.local || cli.link.is_some() {
        return false;
    }
    if env::var_os("LAUNCH_CODE_HOME").is_some() && !cli.global {
        return false;
    }
    matches!(&cli.command, Commands::Cleanup(_))
}

fn should_use_global_batch_session_control(cli: &Cli) -> bool {
    if cli.local || cli.link.is_some() {
        return false;
    }
    if env::var_os("LAUNCH_CODE_HOME").is_some() && !cli.global {
        return false;
    }
    match &cli.command {
        Commands::Stop(args) => args.targets_all(),
        Commands::Restart(args) => args.targets_all(),
        Commands::Suspend(args) => args.targets_all(),
        Commands::Resume(args) => args.targets_all(),
        _ => false,
    }
}

fn is_global_session_fallback_enabled(cli: &Cli) -> bool {
    if cli.local || cli.link.is_some() {
        return false;
    }
    if env::var_os("LAUNCH_CODE_HOME").is_some() && !cli.global {
        return false;
    }
    true
}

fn should_use_global_session_fallback(
    fallback_enabled: bool,
    command: &Commands,
    missing_id: &str,
) -> bool {
    if !fallback_enabled {
        return false;
    }
    matches!(command_session_id(command), Some(id) if id == missing_id)
}

fn command_session_id(command: &Commands) -> Option<&str> {
    match command {
        Commands::Attach(args) => args.resolved_id(),
        Commands::Inspect(args) => args.resolved_id(),
        Commands::Logs(args) => args.resolved_id(),
        Commands::Stop(args) => args.single_target_id(),
        Commands::Restart(args) => args.single_target_id(),
        Commands::Suspend(args) => args.single_target_id(),
        Commands::Resume(args) => args.single_target_id(),
        Commands::Status(args) => args.resolved_id(),
        Commands::Dap(args) => dap_command_session_id(&args.command),
        Commands::Doctor(args) => doctor_command_session_id(&args.command),
        _ => None,
    }
}

fn dap_command_session_id(command: &DapCommands) -> Option<&str> {
    match command {
        DapCommands::Request(args) => Some(&args.id),
        DapCommands::Batch(args) => Some(&args.id),
        DapCommands::Breakpoints(args) => Some(&args.id),
        DapCommands::ExceptionBreakpoints(args) => Some(&args.id),
        DapCommands::Evaluate(args) => Some(&args.id),
        DapCommands::SetVariable(args) => Some(&args.id),
        DapCommands::Continue(args) => Some(&args.id),
        DapCommands::Pause(args) => Some(&args.id),
        DapCommands::Next(args) => Some(&args.id),
        DapCommands::StepIn(args) => Some(&args.id),
        DapCommands::StepOut(args) => Some(&args.id),
        DapCommands::Disconnect(args) => Some(&args.id),
        DapCommands::Terminate(args) => Some(&args.id),
        DapCommands::AdoptSubprocess(args) => Some(&args.id),
        DapCommands::Events(args) => Some(&args.id),
        DapCommands::Threads(args) => Some(&args.id),
        DapCommands::StackTrace(args) => Some(&args.id),
        DapCommands::Scopes(args) => Some(&args.id),
        DapCommands::Variables(args) => Some(&args.id),
    }
}

fn doctor_command_session_id(command: &DoctorCommands) -> Option<&str> {
    match command {
        DoctorCommands::Debug(args) => Some(&args.id),
        DoctorCommands::Runtime(_) => None,
        DoctorCommands::All(args) => args.id.as_deref(),
    }
}
