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
    #[error("session id is ambiguous: {0}")]
    SessionIdAmbiguous(String),
    #[error("session has no active pid: {0}")]
    SessionMissingPid(String),
    #[error("session has no debug metadata: {0}")]
    SessionMissingDebugMeta(String),
    #[error("session has no log path: {0}")]
    SessionMissingLogPath(String),
    #[error("session state changed during operation: {0}")]
    SessionStateChanged(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("unsupported profile bundle version: {0}; expected 1")]
    ProfileBundleVersionUnsupported(u32),
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
    #[error("debug mode currently supports python and node runtimes only: {0}")]
    UnsupportedDebugRuntime(String),
    #[error("dap operations are unavailable for this runtime/backend: {0}")]
    UnsupportedDapRuntime(String),
    #[error("http server error: {0}")]
    Http(String),
    #[error("dap error: {0}")]
    Dap(String),
    #[error("link not found: {0}")]
    LinkNotFound(String),
    #[error("invalid link path: {0}")]
    InvalidLinkPath(String),
    #[error("invalid start options: {0}")]
    InvalidStartOptions(String),
    #[error("runtime readiness failed: {0}")]
    RuntimeReadinessFailed(String),
    #[error("confirmation required: {0}")]
    ConfirmationRequired(String),
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
            Self::Process(ProcessError::StopTimeout { .. }) => "stop_timeout",
            Self::Process(_) => "process_error",
            Self::SessionNotFound(_) => "session_not_found",
            Self::SessionIdAmbiguous(_) => "session_id_ambiguous",
            Self::SessionMissingPid(_) => "session_missing_pid",
            Self::SessionMissingDebugMeta(_) => "session_missing_debug_meta",
            Self::SessionMissingLogPath(_) => "session_missing_log_path",
            Self::SessionStateChanged(_) => "session_state_changed",
            Self::ProfileNotFound(_) => "profile_not_found",
            Self::ProfileBundleVersionUnsupported(_) => "profile_bundle_version_unsupported",
            Self::ProfileValidationFailed(_) => "profile_validation_failed",
            Self::InvalidEnvPair(_) => "invalid_env_pair",
            Self::InvalidEnvFileLine(_) => "invalid_env_file_line",
            Self::InvalidLogRegex(_) => "invalid_log_regex",
            Self::PythonDebugpyUnavailable => "python_debugpy_unavailable",
            Self::UnsupportedDebugRuntime(_) => "unsupported_debug_runtime",
            Self::UnsupportedDapRuntime(_) => "unsupported_dap_runtime",
            Self::Http(_) => "http_error",
            Self::Dap(_) => "dap_error",
            Self::LinkNotFound(_) => "link_not_found",
            Self::InvalidLinkPath(_) => "invalid_link_path",
            Self::InvalidStartOptions(_) => "invalid_start_options",
            Self::RuntimeReadinessFailed(_) => "runtime_readiness_failed",
            Self::ConfirmationRequired(_) => "confirmation_required",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidEnvPair(_)
            | Self::InvalidEnvFileLine(_)
            | Self::InvalidLogRegex(_)
            | Self::ProfileBundleVersionUnsupported(_)
            | Self::ProfileValidationFailed(_)
            | Self::UnsupportedDebugRuntime(_)
            | Self::UnsupportedDapRuntime(_) => 2,
            Self::InvalidLinkPath(_) => 2,
            Self::InvalidStartOptions(_) => 2,
            Self::RuntimeReadinessFailed(_) => 2,
            Self::ConfirmationRequired(_) => 2,
            Self::SessionNotFound(_)
            | Self::SessionIdAmbiguous(_)
            | Self::SessionMissingPid(_)
            | Self::SessionMissingDebugMeta(_)
            | Self::SessionMissingLogPath(_)
            | Self::SessionStateChanged(_)
            | Self::ProfileNotFound(_)
            | Self::LinkNotFound(_) => 3,
            Self::PythonDebugpyUnavailable => 4,
            Self::Dap(_) => 5,
            Self::Http(_) => 6,
            _ => 1,
        }
    }
}
