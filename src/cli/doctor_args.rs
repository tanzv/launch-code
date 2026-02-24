use clap::{Args, Subcommand};

use super::RuntimeArg;

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[command(subcommand)]
    pub command: DoctorCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DoctorCommands {
    #[command(about = "Collect session status, threads, events, and recent logs.")]
    Debug(DoctorDebugArgs),
    #[command(about = "Inspect runtime toolchain and debugger prerequisites.")]
    Runtime(DoctorRuntimeArgs),
    #[command(about = "Run runtime checks plus optional session debug diagnostics in one report.")]
    All(DoctorAllArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DoctorDebugArgs {
    #[arg(long, help = "Target session id.")]
    pub id: String,
    #[arg(
        long,
        default_value_t = 80,
        help = "Maximum number of tail log lines to include in inspect output."
    )]
    pub tail: usize,
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum number of queued debug adapter events to include."
    )]
    pub max_events: usize,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Timeout in milliseconds for debug adapter requests and event polling."
    )]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorRuntimeArgs {
    #[arg(long, value_enum, help = "Filter runtime checks by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Exit with non-zero status if selected runtimes do not satisfy strict readiness checks."
    )]
    pub strict: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorAllArgs {
    #[arg(long, value_enum, help = "Filter runtime checks by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(
        long,
        default_value_t = false,
        help = "Exit with non-zero status if selected runtimes do not satisfy strict readiness checks."
    )]
    pub strict: bool,
    #[arg(
        long,
        help = "Optional debug session id. When provided, include doctor debug diagnostics for this session."
    )]
    pub id: Option<String>,
    #[arg(
        long,
        default_value_t = 80,
        help = "Maximum number of tail log lines to include when --id is set."
    )]
    pub tail: usize,
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum number of queued debug adapter events to include when --id is set."
    )]
    pub max_events: usize,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Timeout in milliseconds for debug adapter requests when --id is set."
    )]
    pub timeout_ms: u64,
}
