use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

mod config_args;
mod dap_args;
mod doctor_args;
mod lifecycle_args;
mod link_args;
mod project_args;

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
pub use doctor_args::{DoctorArgs, DoctorCommands, DoctorDebugArgs};
pub use lifecycle_args::{BatchSortArg, RestartArgs, ResumeArgs, StopArgs, SuspendArgs};
pub use link_args::{LinkAddArgs, LinkArgs, LinkCommands, LinkNameArgs, LinkPruneArgs};
pub use project_args::{
    ProjectArgs, ProjectClearArgs, ProjectCommands, ProjectListArgs, ProjectListFieldArg,
    ProjectSetArgs, ProjectUnsetArgs, ProjectUnsetFieldArg,
};

#[derive(Debug, Parser)]
#[command(
    name = "lcode",
    bin_name = "lcode",
    version,
    about = "IDE-like launch manager CLI",
    long_about = "IDE-like launch manager CLI for launching, supervising, and debugging local development programs.",
    after_help = "Examples:\n  lcode start --runtime python --entry app.py --cwd .\n  lcode debug --runtime python --entry app.py --cwd .\n  lcode dap evaluate --id <session_id> --expression \"counter + 1\"\n  lcode serve --bind 127.0.0.1:8787 --token <token>"
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Emit structured JSON output for command results and errors."
    )]
    pub json: bool,
    #[arg(
        long = "global",
        global = true,
        default_value_t = false,
        help = "Use global link scope (default behavior). Current workspace link is auto-created when missing."
    )]
    pub global: bool,
    #[arg(
        long = "local",
        global = true,
        default_value_t = false,
        conflicts_with_all = ["global", "link"],
        help = "Use workspace-local state scope (LAUNCH_CODE_HOME or current directory)."
    )]
    pub local: bool,
    #[arg(
        long,
        global = true,
        conflicts_with = "local",
        help = "Route runtime commands to a linked workspace by link name."
    )]
    pub link: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
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
    Restart(RestartArgs),
    #[command(about = "Suspend a running session.")]
    Suspend(SuspendArgs),
    #[command(about = "Resume a suspended session.")]
    Resume(ResumeArgs),
    #[command(about = "Show reconciled status for a session.")]
    Status(SessionIdArgs),
    #[command(
        about = "List known sessions with optional filters.",
        visible_alias = "ps"
    )]
    List(ListArgs),
    #[command(
        about = "List only running sessions across the current scope.",
        long_about = "List sessions with reconciled running status. Defaults to a compact table optimized for interactive use. Use --wide to show full columns."
    )]
    Running(RunningArgs),
    #[command(
        about = "Remove stale session records.",
        long_about = "Remove session records matching selected statuses. In global scope cleanup runs across all linked workspaces; use --local to limit to the current workspace. By default cleanup targets stopped and unknown sessions. Use --status to narrow scope and --dry-run to preview matched records."
    )]
    Cleanup(CleanupArgs),
    #[command(about = "Manage saved run/debug profiles.")]
    Config(ConfigArgs),
    #[command(about = "Manage workspace project metadata.")]
    Project(ProjectArgs),
    #[command(about = "Manage global workspace links.")]
    Link(LinkArgs),
    #[command(about = "Run reconciliation loop for managed sessions.")]
    Daemon(DaemonArgs),
    #[command(about = "Expose an HTTP control plane for lifecycle and debug APIs.")]
    Serve(ServeArgs),
    #[command(about = "Send DAP (Debug Adapter Protocol) commands to a debug session.")]
    Dap(DapArgs),
    #[command(about = "Run diagnostic checks for session lifecycle and debug channels.")]
    Doctor(DoctorArgs),
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
    #[arg(
        long,
        default_value_t = false,
        help = "Run process in foreground and stream output according to --log-mode."
    )]
    pub foreground: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "After background start, follow this session log stream immediately."
    )]
    pub tail: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = StartLogModeArg::File,
        help = "Startup log mode. file=background log file only, stdout=foreground terminal only, tee=foreground terminal and file."
    )]
    pub log_mode: StartLogModeArg,
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
    #[arg(
        long,
        required_unless_present = "session_id",
        conflicts_with = "session_id",
        help = "Target session id."
    )]
    pub id: Option<String>,
    #[arg(
        value_name = "ID",
        index = 1,
        required_unless_present = "id",
        conflicts_with = "id",
        help = "Target session id (positional shorthand)."
    )]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long, value_enum, help = "Filter sessions by reconciled status.")]
    pub status: Option<ListStatusArg>,
    #[arg(long, value_enum, help = "Filter sessions by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(long, help = "Case-insensitive substring filter on session name.")]
    pub name_contains: Option<String>,
    #[arg(
        long,
        value_enum,
        help = "Output format for list view. `table` and `wide` both mean full columns."
    )]
    pub format: Option<ListFormatArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Show compact columns optimized for interactive terminal usage."
    )]
    pub compact: bool,
    #[arg(
        short = 'q',
        long,
        default_value_t = false,
        help = "Print only session ids (one per line)."
    )]
    pub quiet: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Disable compact-column truncation."
    )]
    pub no_trunc: bool,
    #[arg(
        long,
        default_value_t = 12,
        value_parser = parse_short_id_len,
        help = "Short session id length in compact table view."
    )]
    pub short_id_len: usize,
    #[arg(
        long,
        default_value_t = false,
        help = "Do not print table headers for table/compact formats."
    )]
    pub no_headers: bool,
}

#[derive(Debug, Clone, Args)]
pub struct RunningArgs {
    #[arg(long, value_enum, help = "Filter running sessions by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(
        long,
        help = "Case-insensitive substring filter on running session name."
    )]
    pub name_contains: Option<String>,
    #[arg(
        long,
        value_enum,
        help = "Output format for running view. `table` and `wide` both mean full columns."
    )]
    pub format: Option<ListFormatArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Show full list columns instead of compact running view."
    )]
    pub wide: bool,
    #[arg(
        short = 'q',
        long,
        default_value_t = false,
        help = "Print only session ids (one per line)."
    )]
    pub quiet: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Disable compact-column truncation."
    )]
    pub no_trunc: bool,
    #[arg(
        long,
        default_value_t = 12,
        value_parser = parse_short_id_len,
        help = "Short session id length in compact table view."
    )]
    pub short_id_len: usize,
    #[arg(
        long,
        default_value_t = false,
        help = "Do not print table headers for table/compact formats."
    )]
    pub no_headers: bool,
}

#[derive(Debug, Clone, Args)]
pub struct CleanupArgs {
    #[arg(
        long = "status",
        value_enum,
        help = "Session status to clean. Repeat for multiple statuses. Defaults to stopped and unknown."
    )]
    pub status: Vec<CleanupStatusArg>,
    #[arg(
        long = "older-than",
        value_name = "DURATION",
        value_parser = parse_cleanup_older_than_secs,
        help = "Only match sessions with updated_at older than this duration (examples: 30m, 12h, 7d)."
    )]
    pub older_than_secs: Option<u64>,
    #[arg(
        long,
        default_value_t = false,
        help = "Preview matched sessions without removing them."
    )]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CleanupStatusArg {
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListFormatArg {
    #[value(alias = "default")]
    Table,
    #[value(alias = "short")]
    Compact,
    #[value(alias = "debug")]
    Wide,
    Id,
}

#[derive(Debug, Clone, Args)]
pub struct LogsArgs {
    #[arg(
        long,
        required_unless_present = "session_id",
        conflicts_with = "session_id",
        help = "Target session id."
    )]
    pub id: Option<String>,
    #[arg(
        value_name = "ID",
        index = 1,
        required_unless_present = "id",
        conflicts_with = "id",
        help = "Target session id (positional shorthand)."
    )]
    pub session_id: Option<String>,
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
    #[arg(
        long,
        required_unless_present = "session_id",
        conflicts_with = "session_id",
        help = "Target session id."
    )]
    pub id: Option<String>,
    #[arg(
        value_name = "ID",
        index = 1,
        required_unless_present = "id",
        conflicts_with = "id",
        help = "Target session id (positional shorthand)."
    )]
    pub session_id: Option<String>,
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
        help = "Maximum queued HTTP requests waiting for workers. Set to 0 for direct handoff (no queue); overload returns HTTP 503 with Retry-After."
    )]
    pub queue_capacity: usize,
    #[arg(
        long,
        default_value_t = 1_048_576,
        help = "Maximum JSON request body size in bytes for HTTP debug APIs."
    )]
    pub max_body_bytes: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StartLogModeArg {
    File,
    Stdout,
    Tee,
}

fn default_serve_workers() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
}

fn parse_short_id_len(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| "short-id-len must be an integer".to_string())?;
    if !(4..=32).contains(&parsed) {
        return Err("short-id-len must be between 4 and 32".to_string());
    }
    Ok(parsed)
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| "value must be a positive integer".to_string())?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(parsed)
}

fn parse_cleanup_older_than_secs(value: &str) -> Result<u64, String> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err("older-than must not be empty".to_string());
    }

    let split_index = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    if split_index == 0 {
        return Err("older-than must start with a positive integer".to_string());
    }

    let amount = raw[..split_index]
        .parse::<u64>()
        .map_err(|_| "older-than must start with a positive integer".to_string())?;
    if amount == 0 {
        return Err("older-than must be greater than 0".to_string());
    }

    let unit = raw[split_index..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "s" => 1u64,
        "m" => 60u64,
        "h" => 60u64 * 60u64,
        "d" => 60u64 * 60u64 * 24u64,
        "w" => 60u64 * 60u64 * 24u64 * 7u64,
        _ => {
            return Err("older-than unit must be one of: s, m, h, d, w (example: 7d)".to_string());
        }
    };

    amount
        .checked_mul(multiplier)
        .ok_or_else(|| "older-than value is too large".to_string())
}

impl SessionIdArgs {
    pub fn resolved_id(&self) -> Option<&str> {
        self.id.as_deref().or(self.session_id.as_deref())
    }
}

impl LogsArgs {
    pub fn resolved_id(&self) -> Option<&str> {
        self.id.as_deref().or(self.session_id.as_deref())
    }
}

impl InspectArgs {
    pub fn resolved_id(&self) -> Option<&str> {
        self.id.as_deref().or(self.session_id.as_deref())
    }
}
