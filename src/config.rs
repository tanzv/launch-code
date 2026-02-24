use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::envfile::{EnvFileError, parse_env_file_map};
use crate::model::{DebugConfig, LaunchMode, LaunchSpec, RuntimeKind};

const VSCODE_LAUNCH_FILE: &str = ".vscode/launch.json";
const LOCAL_LAUNCH_FILE: &str = ".launch-code/launch.json";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("launch file not found")]
    LaunchFileNotFound,
    #[error("launch configuration not found: {0}")]
    ConfigNotFound(String),
    #[error("unsupported runtime type: {0}")]
    UnsupportedRuntimeType(String),
    #[error("program is required in launch configuration: {0}")]
    MissingProgram(String),
    #[error("invalid env file line: {0}")]
    InvalidEnvFileLine(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct LaunchFile {
    configurations: Vec<LaunchConfiguration>,
}

#[derive(Debug, Deserialize)]
struct LaunchConfiguration {
    name: String,
    #[serde(rename = "type")]
    runtime_type: String,
    request: Option<String>,
    program: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    env: Option<BTreeMap<String, LaunchEnvValue>>,
    #[serde(rename = "envFile")]
    env_file: Option<String>,
    #[serde(rename = "python")]
    python: Option<String>,
    #[serde(rename = "pythonPath")]
    python_path: Option<String>,
    managed: Option<bool>,
    #[serde(rename = "debugHost")]
    debug_host: Option<String>,
    #[serde(rename = "debugPort")]
    debug_port: Option<u16>,
    #[serde(rename = "waitForClient")]
    wait_for_client: Option<bool>,
    #[serde(rename = "subProcess", alias = "subprocess")]
    sub_process: Option<bool>,
    #[serde(rename = "preLaunchTask")]
    prelaunch_task: Option<String>,
    #[serde(rename = "postDebugTask")]
    post_debug_task: Option<String>,
    #[serde(rename = "postStopTask")]
    post_stop_task: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LaunchEnvValue {
    String(String),
    Number(serde_json::Number),
    Bool(bool),
    Null(()),
}

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub name: String,
    pub mode: LaunchMode,
    pub managed_override: Option<bool>,
    pub launch_file: Option<PathBuf>,
}

pub fn load_launch_spec(
    workspace_root: &Path,
    request: &LaunchRequest,
) -> Result<LaunchSpec, ConfigError> {
    let launch_path = resolve_launch_file(workspace_root, request.launch_file.clone())?;
    let payload = fs::read_to_string(&launch_path)?;
    let launch_file: LaunchFile = serde_json::from_str(&payload)?;

    let config = launch_file
        .configurations
        .into_iter()
        .find(|item| item.name == request.name)
        .ok_or_else(|| ConfigError::ConfigNotFound(request.name.clone()))?;

    let runtime = map_runtime(&config.runtime_type)?;
    let workspace = infer_workspace_root(&launch_path);
    let cwd = config
        .cwd
        .as_deref()
        .map(|value| resolve_path_string(&workspace, value))
        .unwrap_or_else(|| workspace.to_string_lossy().to_string());

    let entry = config
        .program
        .as_deref()
        .ok_or_else(|| ConfigError::MissingProgram(config.name.clone()))
        .map(|value| resolve_path_string(&workspace, value))?;

    let mut env_map = BTreeMap::new();
    let mut env_remove = BTreeSet::new();
    if let Some(env_file) = config.env_file.as_deref() {
        let env_path = resolve_path_string(&workspace, env_file);
        env_map.extend(parse_env_file(Path::new(&env_path))?);
    }

    if let Some(map) = config.env {
        for (key, value) in map {
            match value {
                LaunchEnvValue::Null(()) => {
                    env_map.remove(&key);
                    env_remove.insert(key);
                }
                other => {
                    env_map.insert(key.clone(), env_value_to_string(other));
                    env_remove.remove(&key);
                }
            }
        }
    }

    if let Some(python) = config.python.or(config.python_path) {
        let resolved = resolve_path_string(&workspace, &python);
        env_map.insert("PYTHON_BIN".to_string(), resolved);
        env_remove.remove("PYTHON_BIN");
    }

    let mut mode = request.mode.clone();
    if mode == LaunchMode::Run {
        if let Some(req) = config.request.as_deref() {
            if req.eq_ignore_ascii_case("debug") {
                mode = LaunchMode::Debug;
            }
        }
    }

    let managed = request
        .managed_override
        .unwrap_or_else(|| config.managed.unwrap_or(false));

    let debug = if mode == LaunchMode::Debug {
        Some(DebugConfig {
            host: config.debug_host.unwrap_or_else(|| "127.0.0.1".to_string()),
            port: config.debug_port.unwrap_or(5678),
            wait_for_client: config.wait_for_client.unwrap_or(true),
            subprocess: config.sub_process.unwrap_or(true),
        })
    } else {
        None
    };

    Ok(LaunchSpec {
        name: config.name,
        runtime,
        entry,
        args: config.args.unwrap_or_default(),
        cwd,
        env: env_map,
        env_remove: env_remove.into_iter().collect(),
        managed,
        mode,
        debug,
        prelaunch_task: config.prelaunch_task,
        poststop_task: config.post_stop_task.or(config.post_debug_task),
    })
}

fn resolve_launch_file(
    workspace_root: &Path,
    explicit: Option<PathBuf>,
) -> Result<PathBuf, ConfigError> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path);
        }
        return Err(ConfigError::LaunchFileNotFound);
    }

    let vscode_path = workspace_root.join(VSCODE_LAUNCH_FILE);
    if vscode_path.exists() {
        return Ok(vscode_path);
    }

    let local_path = workspace_root.join(LOCAL_LAUNCH_FILE);
    if local_path.exists() {
        return Ok(local_path);
    }

    Err(ConfigError::LaunchFileNotFound)
}

fn map_runtime(kind: &str) -> Result<RuntimeKind, ConfigError> {
    match kind.to_ascii_lowercase().as_str() {
        "python" => Ok(RuntimeKind::Python),
        "node" | "pwa-node" | "node-terminal" => Ok(RuntimeKind::Node),
        "rust" | "lldb" | "codelldb" => Ok(RuntimeKind::Rust),
        "go" => Ok(RuntimeKind::Go),
        other => Err(ConfigError::UnsupportedRuntimeType(other.to_string())),
    }
}

fn infer_workspace_root(launch_file: &Path) -> PathBuf {
    let launch_dir = launch_file.parent().unwrap_or_else(|| Path::new("."));
    if launch_dir
        .file_name()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v == ".vscode")
    {
        return launch_dir.parent().unwrap_or(launch_dir).to_path_buf();
    }

    launch_dir.to_path_buf()
}

fn resolve_path_string(base: &Path, value: &str) -> String {
    let expanded = expand_template_string(base, value);
    let candidate = Path::new(&expanded);
    if candidate.is_absolute() {
        return candidate.to_string_lossy().to_string();
    }

    base.join(candidate).to_string_lossy().to_string()
}

fn expand_template_string(base: &Path, raw: &str) -> String {
    let workspace = base.to_string_lossy().to_string();
    let workspace_basename = base
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_string();
    let mut output = raw
        .replace("${workspaceFolder}", &workspace)
        .replace("${workspaceFolderBasename}", &workspace_basename);

    output = expand_env_tokens(&output);
    output
}

fn expand_env_tokens(raw: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0usize;

    while let Some(start_offset) = raw[cursor..].find("${env:") {
        let start = cursor + start_offset;
        output.push_str(&raw[cursor..start]);
        let name_start = start + 6;
        if let Some(end_offset) = raw[name_start..].find('}') {
            let end = name_start + end_offset;
            let key = &raw[name_start..end];
            let value = std::env::var(key).unwrap_or_default();
            output.push_str(&value);
            cursor = end + 1;
        } else {
            output.push_str(&raw[start..]);
            cursor = raw.len();
        }
    }

    if cursor < raw.len() {
        output.push_str(&raw[cursor..]);
    }

    output
}

fn parse_env_file(path: &Path) -> Result<BTreeMap<String, String>, ConfigError> {
    parse_env_file_map(path).map_err(|err| match err {
        EnvFileError::InvalidLine(line) => ConfigError::InvalidEnvFileLine(line),
        EnvFileError::Io(io) => ConfigError::Io(io),
    })
}

fn env_value_to_string(value: LaunchEnvValue) -> String {
    match value {
        LaunchEnvValue::String(raw) => raw,
        LaunchEnvValue::Number(raw) => raw.to_string(),
        LaunchEnvValue::Bool(raw) => raw.to_string(),
        LaunchEnvValue::Null(()) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_launch_spec_supports_env_null_unset_and_scalar_env_values() {
        let tmp = tempdir().expect("temp dir should exist");
        let workspace = tmp.path();
        let vscode_dir = workspace.join(".vscode");
        fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

        let env_file = workspace.join(".env");
        fs::write(&env_file, "FROM_FILE=1\nREMOVE_ME=from-file\n")
            .expect("env file should be written");

        let launch_path = vscode_dir.join("launch.json");
        fs::write(
            &launch_path,
            "{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {\n      \"name\": \"Env Null Config\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"${workspaceFolder}/app.py\",\n      \"cwd\": \"${workspaceFolder}\",\n      \"envFile\": \"${workspaceFolder}/.env\",\n      \"env\": {\n        \"REMOVE_ME\": null,\n        \"NUMERIC\": 7,\n        \"FLAG\": true\n      }\n    }\n  ]\n}\n",
        )
        .expect("launch file should be written");

        let request = LaunchRequest {
            name: "Env Null Config".to_string(),
            mode: LaunchMode::Run,
            managed_override: None,
            launch_file: Some(launch_path),
        };
        let spec = load_launch_spec(workspace, &request).expect("launch spec should load");

        assert_eq!(spec.env.get("FROM_FILE"), Some(&"1".to_string()));
        assert_eq!(spec.env.get("NUMERIC"), Some(&"7".to_string()));
        assert_eq!(spec.env.get("FLAG"), Some(&"true".to_string()));
        assert!(!spec.env.contains_key("REMOVE_ME"));
        assert!(
            spec.env_remove.iter().any(|item| item == "REMOVE_ME"),
            "env_remove should contain env-null key"
        );
    }
}
