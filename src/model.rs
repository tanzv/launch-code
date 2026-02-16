use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Python,
    Node,
    Rust,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub name: String,
    pub runtime: RuntimeKind,
    pub entry: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
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
pub struct AppState {
    #[serde(default)]
    pub profiles: BTreeMap<String, LaunchSpec>,
    #[serde(default)]
    pub sessions: BTreeMap<String, SessionRecord>,
}

pub fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
}
