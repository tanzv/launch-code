use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::{LaunchModeArg, RuntimeArg};

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommands {
    #[command(about = "List saved profiles.")]
    List,
    #[command(about = "Show one saved profile.")]
    Show(ConfigNameArgs),
    #[command(about = "Save or update a profile.")]
    Save(ConfigSaveArgs),
    #[command(about = "Delete a saved profile.")]
    Delete(ConfigNameArgs),
    #[command(
        about = "Run a saved profile.",
        long_about = "Run a saved profile with optional one-off overrides. Env merge order: saved profile env, then --env-file values (in declaration order), then --env overrides.",
        after_help = "Examples:\n  lcode config run --name \"Python Profile\"\n  lcode config run --name \"Python Profile\" --clear-args --clear-env --env-file ./.env\n  lcode config run --name \"Python Profile\" --env-file ./.env.base --env-file ./.env.local --env API_URL=http://127.0.0.1:9000"
    )]
    Run(ConfigRunArgs),
    #[command(about = "Validate one profile or all saved profiles.")]
    Validate(ConfigValidateArgs),
    #[command(about = "Export profiles to a JSON file.")]
    Export(ConfigExportArgs),
    #[command(about = "Import profiles from a JSON file.")]
    Import(ConfigImportArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ConfigNameArgs {
    #[arg(long, help = "Profile name.")]
    pub name: String,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigValidateArgs {
    #[arg(
        long,
        help = "Profile name to validate.",
        required_unless_present = "all",
        conflicts_with = "all"
    )]
    pub name: Option<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Validate all saved profiles.",
        conflicts_with = "name"
    )]
    pub all: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigRunArgs {
    #[arg(long, help = "Profile name.")]
    pub name: String,
    #[arg(long, value_enum, help = "Optional mode override for this run.")]
    pub mode: Option<LaunchModeArg>,
    #[arg(long, help = "Force managed restart behavior for this run.")]
    pub managed: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Ignore saved profile arguments for this run."
    )]
    pub clear_args: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Ignore saved profile environment variables for this run."
    )]
    pub clear_env: bool,
    #[arg(
        long,
        help = "Env file loaded for this run (KEY=VALUE per line). Repeatable; later files override earlier ones."
    )]
    pub env_file: Vec<PathBuf>,
    #[arg(
        long = "arg",
        help = "Additional runtime argument for this run. Repeatable."
    )]
    pub args: Vec<String>,
    #[arg(
        long = "env",
        help = "Runtime environment override in KEY=VALUE format. Repeatable."
    )]
    pub env: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigExportArgs {
    #[arg(long, help = "Export target JSON file path.")]
    pub file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigImportArgs {
    #[arg(long, help = "Import source JSON file path.")]
    pub file: PathBuf,
    #[arg(
        long,
        default_value_t = false,
        help = "Replace all existing profiles instead of merging."
    )]
    pub replace: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigSaveArgs {
    #[arg(long, help = "Profile name.")]
    pub name: String,
    #[arg(long, value_enum, help = "Runtime kind used by this profile.")]
    pub runtime: RuntimeArg,
    #[arg(long, help = "Program entry path for this profile.")]
    pub entry: String,
    #[arg(
        long,
        default_value = ".",
        help = "Working directory for this profile."
    )]
    pub cwd: String,
    #[arg(
        long = "arg",
        help = "Program argument. Repeat for multiple arguments."
    )]
    pub args: Vec<String>,
    #[arg(
        long = "env",
        help = "Environment variable pair in KEY=VALUE format. Repeatable."
    )]
    pub env: Vec<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Enable managed restart on unexpected exit."
    )]
    pub managed: bool,
    #[arg(long, value_enum, default_value_t = LaunchModeArg::Run, help = "Default run mode for this profile.")]
    pub mode: LaunchModeArg,
    #[arg(
        long,
        default_value = "127.0.0.1",
        help = "Debug adapter bind host for debug mode."
    )]
    pub host: String,
    #[arg(
        long,
        default_value_t = 5678,
        help = "Requested debug adapter port for debug mode."
    )]
    pub port: u16,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, help = "Wait for debugger attach before running user code in debug mode.")]
    pub wait_for_client: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, help = "Enable debugpy subprocess debugging hooks in debug mode.")]
    pub subprocess: bool,
    #[arg(long, help = "Optional prelaunch shell task.")]
    pub prelaunch_task: Option<String>,
    #[arg(long, help = "Optional poststop shell task.")]
    pub poststop_task: Option<String>,
}
