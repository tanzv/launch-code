use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

mod config_args;
mod dap_args;

pub use config_args::{
    ConfigArgs, ConfigCommands, ConfigExportArgs, ConfigImportArgs, ConfigNameArgs, ConfigRunArgs,
    ConfigSaveArgs, ConfigValidateArgs,
};
pub use dap_args::{
    DapAdoptSubprocessArgs, DapArgs, DapBatchArgs, DapBreakpointsArgs, DapCommands,
    DapContinueArgs, DapDisconnectArgs, DapEvaluateArgs, DapEvaluateContextArg, DapEventsArgs,
    DapExceptionBreakpointsArgs, DapPauseArgs, DapRequestArgs, DapScopesArgs, DapSetVariableArgs,
    DapStackTraceArgs, DapStepArgs, DapTerminateArgs, DapThreadsArgs, DapVariablesArgs,
    DapVariablesFilterArg,
};

#[derive(Debug, Parser)]
#[command(
    name = "launch-code",
    version,
    about = "IDE-like launch manager CLI",
    long_about = "IDE-like launch manager CLI for launching, supervising, and debugging local development programs.",
    after_help = "Examples:\n  launch-code start --runtime python --entry app.py --cwd .\n  launch-code debug --runtime python --entry app.py --cwd .\n  launch-code dap evaluate --id <session_id> --expression \"counter + 1\"\n  launch-code serve --bind 127.0.0.1:8787 --token <token>"
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Emit structured JSON output for command results and errors."
    )]
    pub json: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(
        about = "Start a new run session.",
        long_about = "Start a run-mode session for a local program. Env merge order: --env-file values (in declaration order), then --env overrides."
    )]
    Start(StartArgs),
    #[command(
        about = "Run a session in debug mode.",
        long_about = "Start a debug-mode session for a local program. Env merge order: --env-file values (in declaration order), then --env overrides."
    )]
    Debug(DebugArgs),
    #[command(about = "Launch a session from launch.json-style configuration.")]
    Launch(LaunchArgs),
    #[command(about = "Print debugger attach metadata for a session.")]
    Attach(SessionIdArgs),
    #[command(about = "Inspect process status and recent log lines for a session.")]
    Inspect(InspectArgs),
    #[command(about = "Stream or tail logs for a session.")]
    Logs(LogsArgs),
    #[command(about = "Stop a running session.")]
    Stop(StopArgs),
    #[command(about = "Restart a session process.")]
    Restart(SessionIdArgs),
    #[command(about = "Suspend a running session.")]
    Suspend(SessionIdArgs),
    #[command(about = "Resume a suspended session.")]
    Resume(SessionIdArgs),
    #[command(about = "Show reconciled status for a session.")]
    Status(SessionIdArgs),
    #[command(about = "List known sessions with optional filters.")]
    List(ListArgs),
    #[command(about = "Manage saved run/debug profiles.")]
    Config(ConfigArgs),
    #[command(about = "Run reconciliation loop for managed sessions.")]
    Daemon(DaemonArgs),
    #[command(about = "Expose an HTTP control plane for lifecycle and debug APIs.")]
    Serve(ServeArgs),
    #[command(about = "Send DAP (Debug Adapter Protocol) commands to a debug session.")]
    Dap(DapArgs),
}

#[derive(Debug, Clone, Args)]
pub struct StartArgs {
    #[arg(long, help = "Optional session display name.")]
    pub name: Option<String>,
    #[arg(
        long,
        value_enum,
        help = "Runtime kind used to launch the entry program."
    )]
    pub runtime: RuntimeArg,
    #[arg(long, help = "Program entry path for the selected runtime.")]
    pub entry: String,
    #[arg(
        long,
        default_value = ".",
        help = "Working directory for the launched process."
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
        help = "Env file loaded for this run (KEY=VALUE per line). Repeatable; later files override earlier ones."
    )]
    pub env_file: Vec<PathBuf>,
    #[arg(
        long,
        default_value_t = false,
        help = "Enable managed restart on unexpected exit."
    )]
    pub managed: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DebugArgs {
    #[command(flatten)]
    pub start: StartArgs,
    #[arg(long, default_value = "127.0.0.1", help = "Debug adapter bind host.")]
    pub host: String,
    #[arg(long, default_value_t = 5678, help = "Requested debug adapter port.")]
    pub port: u16,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, help = "Wait for debugger attach before running user code.")]
    pub wait_for_client: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, help = "Enable debugpy subprocess debugging hooks for child Python processes.")]
    pub subprocess: bool,
}

#[derive(Debug, Clone, Args)]
pub struct LaunchArgs {
    #[arg(long, help = "Configuration name from launch.json.")]
    pub name: String,
    #[arg(long, value_enum, default_value_t = LaunchModeArg::Debug, help = "Run mode to apply when launching configuration.")]
    pub mode: LaunchModeArg,
    #[arg(
        long,
        help = "Optional launch.json path. Defaults to .vscode/launch.json."
    )]
    pub launch_file: Option<PathBuf>,
    #[arg(long, help = "Force managed restart behavior for this launch request.")]
    pub managed: bool,
}

#[derive(Debug, Clone, Args)]
pub struct SessionIdArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long, value_enum, help = "Filter sessions by reconciled status.")]
    pub status: Option<ListStatusArg>,
    #[arg(long, value_enum, help = "Filter sessions by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(long, help = "Case-insensitive substring filter on session name.")]
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct StopArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = false,
        help = "Force kill if process is still alive after grace timeout."
    )]
    pub force: bool,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Graceful stop timeout in milliseconds before force kill."
    )]
    pub grace_timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct LogsArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 100,
        help = "Number of recent lines to show before follow."
    )]
    pub tail: usize,
    #[arg(
        long,
        default_value_t = false,
        help = "Keep streaming appended log output."
    )]
    pub follow: bool,
    #[arg(
        long,
        default_value_t = 200,
        help = "Polling interval in milliseconds when --follow is set."
    )]
    pub poll_ms: u64,
    #[arg(
        long = "contains",
        help = "Substring filter for log lines. Repeat to match any token."
    )]
    pub contains: Vec<String>,
    #[arg(
        long = "exclude",
        help = "Exclude filter for log lines. Repeat to remove any token match."
    )]
    pub exclude: Vec<String>,
    #[arg(long, help = "Regular expression include filter for log lines.")]
    pub regex: Option<String>,
    #[arg(long, help = "Exclude regular expression for log lines.")]
    pub exclude_regex: Option<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Case-insensitive matching for --contains/--exclude/--regex/--exclude-regex."
    )]
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DaemonArgs {
    #[arg(
        long,
        default_value_t = false,
        help = "Run one reconciliation pass then exit."
    )]
    pub once: bool,
    #[arg(
        long,
        default_value_t = 1000,
        help = "Reconciliation interval in milliseconds."
    )]
    pub interval_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct InspectArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum number of tail log lines to include."
    )]
    pub tail: usize,
}

#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    #[arg(
        long,
        default_value = "127.0.0.1:0",
        help = "HTTP bind address. Use :0 for random port."
    )]
    pub bind: String,
    #[arg(
        long,
        help = "Bearer token required by all HTTP API requests. Prefer --token-file or LAUNCH_CODE_HTTP_TOKEN in production."
    )]
    pub token: Option<String>,
    #[arg(
        long,
        help = "Path to file containing bearer token (first non-empty trimmed line)."
    )]
    pub token_file: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = default_serve_workers(),
        help = "Number of worker threads for processing HTTP requests."
    )]
    pub workers: usize,
    #[arg(
        long,
        default_value_t = 256,
        help = "Maximum queued HTTP requests waiting for workers."
    )]
    pub queue_capacity: usize,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum RuntimeArg {
    Python,
    Node,
    Rust,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum LaunchModeArg {
    Run,
    Debug,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum ListStatusArg {
    Running,
    Stopped,
    Suspended,
    Unknown,
}

fn default_serve_workers() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
}
