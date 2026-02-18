use clap::{Args, Subcommand};

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[command(subcommand)]
    pub command: DoctorCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DoctorCommands {
    #[command(about = "Collect session status, threads, events, and recent logs.")]
    Debug(DoctorDebugArgs),
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
