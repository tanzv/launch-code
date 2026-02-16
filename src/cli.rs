use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

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
    #[command(about = "Start a new run session.")]
    Start(StartArgs),
    #[command(about = "Run a session in debug mode.")]
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
    #[command(about = "List all known sessions.")]
    List,
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
    #[command(about = "Run a saved profile.")]
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
        help = "Optional env file loaded for this run (KEY=VALUE per line)."
    )]
    pub env_file: Option<PathBuf>,
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

#[derive(Debug, Clone, Args)]
pub struct SessionIdArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
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
    #[arg(long, help = "Bearer token required by all HTTP API requests.")]
    pub token: String,
}

#[derive(Debug, Clone, Args)]
pub struct DapArgs {
    #[command(subcommand)]
    pub command: DapCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DapCommands {
    #[command(about = "Send one raw DAP request.")]
    Request(DapRequestArgs),
    #[command(about = "Send a sequence of DAP requests from a JSON file.")]
    Batch(DapBatchArgs),
    #[command(about = "Set source breakpoints by file path and line numbers.")]
    Breakpoints(DapBreakpointsArgs),
    #[command(about = "Configure exception breakpoints.")]
    ExceptionBreakpoints(DapExceptionBreakpointsArgs),
    #[command(about = "Evaluate an expression.")]
    Evaluate(DapEvaluateArgs),
    #[command(about = "Set a variable value.")]
    SetVariable(DapSetVariableArgs),
    #[command(about = "Continue execution on a thread.")]
    Continue(DapContinueArgs),
    #[command(about = "Pause execution on a thread.")]
    Pause(DapPauseArgs),
    #[command(about = "Step over on a thread.")]
    Next(DapStepArgs),
    #[command(about = "Step into on a thread.")]
    StepIn(DapStepArgs),
    #[command(about = "Step out on a thread.")]
    StepOut(DapStepArgs),
    #[command(about = "Disconnect the debug adapter session.")]
    Disconnect(DapDisconnectArgs),
    #[command(about = "Terminate the debuggee via DAP.")]
    Terminate(DapTerminateArgs),
    #[command(about = "Adopt a debugpy child-process attach event into a new session id.")]
    AdoptSubprocess(DapAdoptSubprocessArgs),
    #[command(about = "Poll queued DAP events.")]
    Events(DapEventsArgs),
    #[command(about = "Request active threads.")]
    Threads(DapThreadsArgs),
    #[command(about = "Request stack frames for a thread.")]
    StackTrace(DapStackTraceArgs),
    #[command(about = "Request scopes for a stack frame.")]
    Scopes(DapScopesArgs),
    #[command(about = "Request variables for a variablesReference.")]
    Variables(DapVariablesArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DapRequestArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "DAP command name to send.")]
    pub command: String,
    #[arg(long, help = "JSON object string for command arguments.")]
    pub arguments: Option<String>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapBatchArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "Path to a JSON array file of DAP requests.")]
    pub file: PathBuf,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapBreakpointsArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "Source file path.")]
    pub path: String,
    #[arg(long = "line", help = "Breakpoint line number. Repeatable.")]
    pub lines: Vec<u64>,
    #[arg(long, help = "Optional breakpoint condition expression.")]
    pub condition: Option<String>,
    #[arg(long, help = "Optional hit condition expression.")]
    pub hit_condition: Option<String>,
    #[arg(long, help = "Optional log message for logpoint behavior.")]
    pub log_message: Option<String>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapContinueArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        help = "Thread id. If omitted, the first reported thread is used."
    )]
    pub thread_id: Option<u64>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapExceptionBreakpointsArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long = "filter",
        help = "Exception breakpoint filter value. Repeatable."
    )]
    pub filters: Vec<String>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapEvaluateArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "Expression to evaluate.")]
    pub expression: String,
    #[arg(long, help = "Optional stack frame id.")]
    pub frame_id: Option<u64>,
    #[arg(long, value_enum, help = "Evaluation context.")]
    pub context: Option<DapEvaluateContextArg>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapSetVariableArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "variablesReference value from a scope or variable.")]
    pub variables_reference: u64,
    #[arg(long, help = "Variable name to update.")]
    pub name: String,
    #[arg(long, help = "New variable value expression.")]
    pub value: String,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapPauseArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        help = "Thread id. If omitted, the first reported thread is used."
    )]
    pub thread_id: Option<u64>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapStepArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        help = "Thread id. If omitted, the first reported thread is used."
    )]
    pub thread_id: Option<u64>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapDisconnectArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = false,
        help = "Request adapter to terminate the debuggee."
    )]
    pub terminate_debuggee: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Request adapter to suspend the debuggee."
    )]
    pub suspend_debuggee: bool,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapTerminateArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = false,
        help = "Request restart after terminate."
    )]
    pub restart: bool,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapAdoptSubprocessArgs {
    #[arg(long, help = "Parent session id that receives debugpyAttach events.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Event wait timeout in milliseconds."
    )]
    pub timeout_ms: u64,
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum queued events to inspect for debugpyAttach."
    )]
    pub max_events: usize,
    #[arg(
        long,
        default_value_t = 5000,
        help = "Bootstrap timeout in milliseconds for initialize/attach/configurationDone."
    )]
    pub bootstrap_timeout_ms: u64,
    #[arg(long, help = "Optional explicit child session id.")]
    pub child_session_id: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct DapEventsArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum number of events to return."
    )]
    pub max: usize,
    #[arg(
        long,
        default_value_t = 0,
        help = "Event wait timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapThreadsArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapStackTraceArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        help = "Thread id. If omitted, the first reported thread is used."
    )]
    pub thread_id: Option<u64>,
    #[arg(long, help = "Optional start frame index.")]
    pub start_frame: Option<u64>,
    #[arg(long, help = "Optional frame count limit.")]
    pub levels: Option<u64>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapScopesArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "Stack frame id from stackTrace.")]
    pub frame_id: u64,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DapVariablesArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(long, help = "variablesReference value from scopes/variables.")]
    pub variables_reference: u64,
    #[arg(long, value_enum, help = "Variable filter mode.")]
    pub filter: Option<DapVariablesFilterArg>,
    #[arg(long, help = "Optional start index for pagination.")]
    pub start: Option<u64>,
    #[arg(long, help = "Optional item count for pagination.")]
    pub count: Option<u64>,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Request timeout in milliseconds."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum DapVariablesFilterArg {
    Named,
    Indexed,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum DapEvaluateContextArg {
    Watch,
    Repl,
    Hover,
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
