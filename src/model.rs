use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const APP_STATE_SCHEMA_VERSION: u32 = 1;

fn default_app_state_schema_version() -> u32 {
    APP_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Python,
    Node,
    Rust,
    Go,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Run,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_true")]
    pub wait_for_client: bool,
    #[serde(default = "default_true")]
    pub subprocess: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 5678,
            wait_for_client: true,
            subprocess: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_debug_adapter_kind() -> String {
    "unknown".to_string()
}

fn default_debug_transport() -> String {
    "tcp".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub name: String,
    pub runtime: RuntimeKind,
    pub entry: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_remove: Vec<String>,
    pub managed: bool,
    pub mode: LaunchMode,
    pub debug: Option<DebugConfig>,
    #[serde(default)]
    pub prelaunch_task: Option<String>,
    #[serde(default)]
    pub poststop_task: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Stopped,
    Suspended,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugSessionMeta {
    #[serde(default)]
    pub host: String,
    pub requested_port: u16,
    pub active_port: u16,
    pub fallback_applied: bool,
    pub reconnect_policy: String,
    #[serde(default = "default_debug_adapter_kind")]
    pub adapter_kind: String,
    #[serde(default = "default_debug_transport")]
    pub transport: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub spec: LaunchSpec,
    pub status: SessionStatus,
    pub pid: Option<u32>,
    pub supervisor_pid: Option<u32>,
    pub log_path: Option<String>,
    #[serde(default)]
    pub debug_meta: Option<DebugSessionMeta>,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_exit_code: Option<i32>,
    #[serde(default)]
    pub restart_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub languages: Option<Vec<String>>,
    #[serde(default)]
    pub runtimes: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

impl ProjectInfo {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.repository.is_none()
            && self.languages.is_none()
            && self.runtimes.is_none()
            && self.tools.is_none()
            && self.tags.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default = "default_app_state_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub profiles: BTreeMap<String, LaunchSpec>,
    #[serde(default)]
    pub sessions: BTreeMap<String, SessionRecord>,
    #[serde(default)]
    pub project_info: Option<ProjectInfo>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            schema_version: APP_STATE_SCHEMA_VERSION,
            profiles: BTreeMap::new(),
            sessions: BTreeMap::new(),
            project_info: None,
        }
    }
}

pub fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
}
