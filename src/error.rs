use launch_code::{
    config::ConfigError, debug::DebugError, process::ProcessError, runtime::RuntimeError,
    state::StateError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Debug(#[from] DebugError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("session has no active pid: {0}")]
    SessionMissingPid(String),
    #[error("session has no debug metadata: {0}")]
    SessionMissingDebugMeta(String),
    #[error("session has no log path: {0}")]
    SessionMissingLogPath(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("profile validation failed: {0}")]
    ProfileValidationFailed(String),
    #[error("invalid env pair: {0}; expected KEY=VALUE")]
    InvalidEnvPair(String),
    #[error("invalid env file line: {0}; expected KEY=VALUE")]
    InvalidEnvFileLine(String),
    #[error("invalid log regex: {0}")]
    InvalidLogRegex(String),
    #[error("python debug requires debugpy; install with `python -m pip install debugpy`")]
    PythonDebugpyUnavailable,
    #[error("http server error: {0}")]
    Http(String),
    #[error("dap error: {0}")]
    Dap(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io_error",
            Self::Json(_) => "json_error",
            Self::Runtime(_) => "runtime_error",
            Self::Debug(_) => "debug_error",
            Self::Config(_) => "config_error",
            Self::State(_) => "state_error",
            Self::Process(_) => "process_error",
            Self::SessionNotFound(_) => "session_not_found",
            Self::SessionMissingPid(_) => "session_missing_pid",
            Self::SessionMissingDebugMeta(_) => "session_missing_debug_meta",
            Self::SessionMissingLogPath(_) => "session_missing_log_path",
            Self::ProfileNotFound(_) => "profile_not_found",
            Self::ProfileValidationFailed(_) => "profile_validation_failed",
            Self::InvalidEnvPair(_) => "invalid_env_pair",
            Self::InvalidEnvFileLine(_) => "invalid_env_file_line",
            Self::InvalidLogRegex(_) => "invalid_log_regex",
            Self::PythonDebugpyUnavailable => "python_debugpy_unavailable",
            Self::Http(_) => "http_error",
            Self::Dap(_) => "dap_error",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidEnvPair(_)
            | Self::InvalidEnvFileLine(_)
            | Self::InvalidLogRegex(_)
            | Self::ProfileValidationFailed(_) => 2,
            Self::SessionNotFound(_)
            | Self::SessionMissingPid(_)
            | Self::SessionMissingDebugMeta(_)
            | Self::SessionMissingLogPath(_)
            | Self::ProfileNotFound(_) => 3,
            Self::PythonDebugpyUnavailable => 4,
            Self::Dap(_) => 5,
            Self::Http(_) => 6,
            _ => 1,
        }
    }
}
