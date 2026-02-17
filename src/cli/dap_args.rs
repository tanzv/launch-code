use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

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
    #[arg(
        long = "line",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Breakpoint line number. Repeatable."
    )]
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
        value_parser = clap::value_parser!(u64).range(1..),
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
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Optional stack frame id."
    )]
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
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "variablesReference value from a scope or variable."
    )]
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
        value_parser = clap::value_parser!(u64).range(1..),
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
        value_parser = clap::value_parser!(u64).range(1..),
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
        value_parser = clap::value_parser!(u64).range(1..),
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
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Stack frame id from stackTrace."
    )]
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
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "variablesReference value from scopes/variables."
    )]
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
