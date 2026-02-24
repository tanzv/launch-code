mod codec;
mod python_adapter_proxy;
mod shared;
mod subprocess;
mod tcp_proxy;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::debug_backend::DebugBackendKind;
use launch_code::model::RuntimeKind;
use serde_json::json;

use launch_code::state::StateStore;

use crate::app::api_get_session;
use crate::error::AppError;

use python_adapter_proxy::StdioAdapterDapProxy;
use tcp_proxy::TcpDapProxy;

const NODE_DAP_ADAPTER_CMD_ENV: &str = "LCODE_NODE_DAP_ADAPTER_CMD";
const NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV: &str = "LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeAdapterSource {
    Env,
    Path,
    VscodeExtension,
}

impl NodeAdapterSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Path => "path",
            Self::VscodeExtension => "vscode_extension",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeAdapterCommand {
    pub program: String,
    pub args: Vec<String>,
    pub source: NodeAdapterSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeAdapterResolution {
    Command(NodeAdapterCommand),
    InvalidEnv { message: String },
    AutoDiscoveryDisabled,
    NotFound,
}

#[derive(Debug, Default)]
pub(crate) struct DapRegistry {
    proxies: HashMap<String, Arc<DapProxy>>,
}

#[derive(Debug, Clone)]
pub(crate) struct AdoptSubprocessResult {
    pub child_session_id: String,
    pub host: String,
    pub port: u16,
    pub process_id: Option<u32>,
    pub source_event: serde_json::Value,
    pub bootstrap_responses: Vec<serde_json::Value>,
}

pub(crate) fn send_request_with_retry(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    command: &str,
    arguments: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, AppError> {
    let retry_arguments = arguments.clone();
    let bootstrap_timeout = effective_bootstrap_timeout(timeout);
    let response = match send_request_transport_with_retry(
        store, registry, session_id, command, arguments, timeout,
    ) {
        Ok(response) => response,
        Err(initial_err) => {
            let retried = if is_retryable_transport_error(&initial_err) {
                std::thread::sleep(Duration::from_millis(250));
                send_request_transport_with_retry(
                    store,
                    registry,
                    session_id,
                    command,
                    retry_arguments.clone(),
                    timeout,
                )
            } else {
                Err(initial_err)
            };

            match retried {
                Ok(response) => response,
                Err(err) => {
                    if should_bootstrap_after_error(command, &err) {
                        send_bootstrap_sequence(store, registry, session_id, bootstrap_timeout)?;
                        return send_request_transport_with_retry(
                            store,
                            registry,
                            session_id,
                            command,
                            retry_arguments,
                            timeout,
                        );
                    }
                    return Err(err);
                }
            }
        }
    };
    if should_bootstrap_after_response(command, &response) {
        send_bootstrap_sequence(store, registry, session_id, bootstrap_timeout)?;
        return send_request_transport_with_retry(
            store,
            registry,
            session_id,
            command,
            retry_arguments,
            timeout,
        );
    }
    Ok(response)
}

fn effective_bootstrap_timeout(timeout: Duration) -> Duration {
    timeout.max(Duration::from_millis(1500))
}

fn send_request_transport_with_retry(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    command: &str,
    arguments: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, AppError> {
    let proxy = proxy_for_session(store, registry, session_id)?;
    let retry_arguments = arguments.clone();
    match proxy.send_request(command, arguments, timeout) {
        Ok(response) => Ok(response),
        Err(err) => {
            let should_retry = proxy.is_closed() || is_retryable_transport_error(&err);
            if should_retry {
                drop_session(registry, session_id);
                let proxy = proxy_for_session(store, registry, session_id)?;
                match proxy.send_request(command, retry_arguments, timeout) {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        drop_session(registry, session_id);
                        Err(err)
                    }
                }
            } else {
                Err(err)
            }
        }
    }
}

fn should_bootstrap_after_response(command: &str, response: &serde_json::Value) -> bool {
    if is_bootstrap_command(command) {
        return false;
    }

    if !matches!(
        response.get("success").and_then(|v| v.as_bool()),
        Some(false)
    ) {
        return false;
    }

    response
        .get("message")
        .and_then(|v| v.as_str())
        .map(|value| {
            value
                .to_ascii_lowercase()
                .contains("server is not available")
        })
        .unwrap_or(false)
}

fn should_bootstrap_after_error(command: &str, err: &AppError) -> bool {
    if is_bootstrap_command(command) {
        return false;
    }

    match err {
        AppError::Dap(message) => message
            .to_ascii_lowercase()
            .contains("timeout waiting for response"),
        _ => false,
    }
}

fn is_retryable_transport_error(err: &AppError) -> bool {
    match err {
        AppError::Dap(message) => {
            let lower = message.to_ascii_lowercase();
            lower.contains("connection refused")
                || lower.contains("connection reset")
                || lower.contains("broken pipe")
                || lower.contains("not connected")
                || lower.contains("channel disconnected")
        }
        _ => false,
    }
}

fn is_bootstrap_command(command: &str) -> bool {
    matches!(command, "initialize" | "attach" | "configurationDone")
}

fn send_bootstrap_sequence(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    timeout: Duration,
) -> Result<(), AppError> {
    let requests = build_bootstrap_requests(store, session_id)?;
    let responses = send_batch_with_retry(store, registry, session_id, requests, timeout)?;
    validate_bootstrap_responses(&responses)
}

fn build_bootstrap_requests(
    store: &StateStore,
    session_id: &str,
) -> Result<Vec<(String, serde_json::Value)>, AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;
    let backend = require_dap_backend(&session.spec.runtime, true)?;

    let fallback_host = session
        .spec
        .debug
        .as_ref()
        .map(|debug| debug.host.as_str())
        .unwrap_or("127.0.0.1");
    let host = if meta.host.trim().is_empty() {
        fallback_host
    } else {
        meta.host.as_str()
    };
    let port = meta.active_port;

    match backend {
        DebugBackendKind::PythonDebugpy => Ok(vec![
            (
                "initialize".to_string(),
                json!({
                    "clientID": "launch-code",
                    "adapterID": "python",
                    "pathFormat": "path",
                    "linesStartAt1": true,
                    "columnsStartAt1": true
                }),
            ),
            (
                "attach".to_string(),
                json!({
                    "connect": {
                        "host": host,
                        "port": port
                    },
                    "justMyCode": false
                }),
            ),
            ("configurationDone".to_string(), json!({})),
        ]),
        DebugBackendKind::NodeInspector => Ok(vec![
            (
                "initialize".to_string(),
                json!({
                    "clientID": "launch-code",
                    "adapterID": "pwa-node",
                    "pathFormat": "path",
                    "linesStartAt1": true,
                    "columnsStartAt1": true
                }),
            ),
            (
                "attach".to_string(),
                json!({
                    "address": host,
                    "port": port,
                    "restart": true,
                    "localRoot": "${workspaceFolder}",
                    "remoteRoot": "."
                }),
            ),
            ("configurationDone".to_string(), json!({})),
        ]),
        DebugBackendKind::GoDelve => {
            let cwd = session.spec.cwd.clone();
            let mut program = session.spec.entry.clone();
            let entry_path = Path::new(&program);
            if !entry_path.is_absolute() {
                program = Path::new(&cwd).join(entry_path).to_string_lossy().to_string();
            }
            Ok(vec![
                (
                    "initialize".to_string(),
                    json!({
                        "clientID": "launch-code",
                        "adapterID": "go",
                        "pathFormat": "path",
                        "linesStartAt1": true,
                        "columnsStartAt1": true
                    }),
                ),
                (
                    "launch".to_string(),
                    json!({
                        "name": session.spec.name,
                        "type": "go",
                        "request": "launch",
                        "mode": "debug",
                        "program": program,
                        "cwd": cwd,
                        "args": session.spec.args,
                        "env": session.spec.env,
                        "stopOnEntry": session.spec.debug.as_ref().map(|cfg| cfg.wait_for_client).unwrap_or(true)
                    }),
                ),
                ("configurationDone".to_string(), json!({})),
            ])
        }
    }
}

fn validate_bootstrap_responses(responses: &[serde_json::Value]) -> Result<(), AppError> {
    for response in responses {
        if matches!(
            response.get("success").and_then(|v| v.as_bool()),
            Some(false)
        ) {
            let command = response
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let message = response
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("bootstrap request failed");
            if command == "attach" && is_attach_already_debugged_message(message) {
                continue;
            }
            return Err(AppError::Dap(message.to_string()));
        }
    }
    Ok(())
}

fn is_attach_already_debugged_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("already being debugged")
}

pub(crate) fn send_batch_with_retry(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    requests: Vec<(String, serde_json::Value)>,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, AppError> {
    let proxy = proxy_for_session(store, registry, session_id)?;
    let retry_requests = requests.clone();
    match proxy.send_batch(requests, timeout) {
        Ok(responses) => Ok(responses),
        Err(err) => {
            let should_retry = proxy.is_closed() || is_retryable_transport_error(&err);
            if should_retry {
                drop_session(registry, session_id);
                let proxy = proxy_for_session(store, registry, session_id)?;
                match proxy.send_batch(retry_requests, timeout) {
                    Ok(responses) => Ok(responses),
                    Err(err) => {
                        drop_session(registry, session_id);
                        Err(err)
                    }
                }
            } else {
                Err(err)
            }
        }
    }
}

pub(crate) fn adopt_debugpy_subprocess(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    parent_session_id: &str,
    event_timeout: Duration,
    max_events: usize,
    bootstrap_timeout: Duration,
    requested_child_session_id: Option<&str>,
) -> Result<AdoptSubprocessResult, AppError> {
    let proxy = proxy_for_session(store, registry, parent_session_id)?;
    let events = proxy.pop_events(max_events.max(1), event_timeout);

    let target = subprocess::latest_debugpy_attach_target(&events)?;

    let child_session_id = subprocess::register_subprocess_session(
        store,
        parent_session_id,
        &target,
        requested_child_session_id,
    )?;

    let bootstrap_requests = vec![
        (
            "initialize".to_string(),
            json!({
                "clientID": "launch-code",
                "adapterID": "python",
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true
            }),
        ),
        ("attach".to_string(), target.attach_arguments.clone()),
        ("configurationDone".to_string(), json!({})),
    ];

    let bootstrap_responses = match send_batch_with_retry(
        store,
        registry,
        &child_session_id,
        bootstrap_requests,
        bootstrap_timeout,
    ) {
        Ok(responses) => responses,
        Err(err) => {
            let child_id = child_session_id.clone();
            let _ = store.update::<_, _, AppError>(|state| {
                state.sessions.remove(&child_id);
                Ok(())
            });
            return Err(err);
        }
    };

    Ok(AdoptSubprocessResult {
        child_session_id,
        host: target.host,
        port: target.port,
        process_id: target.process_id,
        source_event: target.source_event,
        bootstrap_responses,
    })
}

pub(crate) fn proxy_for_session(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
) -> Result<Arc<DapProxy>, AppError> {
    let state = store.load()?;
    let base_session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?
        .clone();
    // Managed sessions may restart and rotate debug ports; refresh before dialing.
    let session = if base_session.spec.managed {
        api_get_session(store, session_id)?
    } else {
        base_session
    };
    let backend = require_dap_backend(&session.spec.runtime, false)?;
    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;
    let host = meta.host.clone();
    let port = meta.active_port;

    {
        let guard = registry
            .lock()
            .map_err(|_| AppError::Http("dap registry lock poisoned".to_string()))?;
        if let Some(proxy) = guard.proxies.get(session_id) {
            if proxy.matches(&host, port) {
                return Ok(Arc::clone(proxy));
            }
        }
    }

    let proxy = match backend {
        DebugBackendKind::PythonDebugpy => DapProxy::connect_tcp(host, port)?,
        DebugBackendKind::GoDelve => DapProxy::connect_tcp(host, port)?,
        DebugBackendKind::NodeInspector => {
            let (program, args) = resolve_node_adapter_command()?;
            DapProxy::connect_stdio_adapter(program, args, host, port)?
        }
    };
    let mut guard = registry
        .lock()
        .map_err(|_| AppError::Http("dap registry lock poisoned".to_string()))?;
    guard
        .proxies
        .insert(session_id.to_string(), Arc::clone(&proxy));
    Ok(proxy)
}

fn drop_session(registry: &Arc<Mutex<DapRegistry>>, session_id: &str) {
    if let Ok(mut guard) = registry.lock() {
        guard.proxies.remove(session_id);
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum DapProxy {
    Tcp(Arc<TcpDapProxy>),
    StdioAdapter(Arc<StdioAdapterDapProxy>),
}

impl DapProxy {
    pub(crate) fn connect_tcp(host: String, port: u16) -> Result<Arc<Self>, AppError> {
        let inner = TcpDapProxy::connect(host, port)?;
        Ok(Arc::new(Self::Tcp(inner)))
    }

    pub(crate) fn connect_stdio_adapter(
        program: String,
        args: Vec<String>,
        host: String,
        port: u16,
    ) -> Result<Arc<Self>, AppError> {
        let inner = StdioAdapterDapProxy::spawn(program, args, host, port)?;
        Ok(Arc::new(Self::StdioAdapter(inner)))
    }

    pub(crate) fn send_batch(
        &self,
        requests: Vec<(String, serde_json::Value)>,
        timeout: Duration,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        match self {
            Self::Tcp(inner) => inner.send_batch(requests, timeout),
            Self::StdioAdapter(inner) => inner.send_batch(requests, timeout),
        }
    }

    pub(crate) fn matches(&self, host: &str, port: u16) -> bool {
        match self {
            Self::Tcp(inner) => inner.matches(host, port),
            Self::StdioAdapter(inner) => inner.matches(host, port),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self {
            Self::Tcp(inner) => inner.is_closed(),
            Self::StdioAdapter(inner) => inner.is_closed(),
        }
    }

    pub(crate) fn send_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        match self {
            Self::Tcp(inner) => inner.send_request(command, arguments, timeout),
            Self::StdioAdapter(inner) => inner.send_request(command, arguments, timeout),
        }
    }

    pub(crate) fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
        match self {
            Self::Tcp(inner) => inner.pop_events(max, timeout),
            Self::StdioAdapter(inner) => inner.pop_events(max, timeout),
        }
    }
}

fn runtime_label(runtime: &RuntimeKind) -> String {
    match runtime {
        RuntimeKind::Python => "python".to_string(),
        RuntimeKind::Node => "node".to_string(),
        RuntimeKind::Rust => "rust".to_string(),
        RuntimeKind::Go => "go".to_string(),
    }
}

fn resolve_node_adapter_command() -> Result<(String, Vec<String>), AppError> {
    match inspect_node_adapter_resolution() {
        NodeAdapterResolution::Command(command) => Ok((command.program, command.args)),
        NodeAdapterResolution::InvalidEnv { message } => Err(AppError::UnsupportedDapRuntime(
            format!("node (invalid {NODE_DAP_ADAPTER_CMD_ENV}: {message})"),
        )),
        NodeAdapterResolution::AutoDiscoveryDisabled => {
            Err(AppError::UnsupportedDapRuntime(format!(
                "node (set {NODE_DAP_ADAPTER_CMD_ENV} to a JSON array command, for example [\"node\",\"/path/to/js-debug/src/dapDebugServer.js\"])"
            )))
        }
        NodeAdapterResolution::NotFound => Err(AppError::UnsupportedDapRuntime(format!(
            "node (set {NODE_DAP_ADAPTER_CMD_ENV} or install js-debug adapter in PATH/VSCode extensions)"
        ))),
    }
}

pub(crate) fn inspect_node_adapter_resolution() -> NodeAdapterResolution {
    if let Ok(raw) = std::env::var(NODE_DAP_ADAPTER_CMD_ENV) {
        return match parse_adapter_command_value(&raw) {
            Ok((program, args)) => NodeAdapterResolution::Command(NodeAdapterCommand {
                program,
                args,
                source: NodeAdapterSource::Env,
            }),
            Err(message) => NodeAdapterResolution::InvalidEnv { message },
        };
    }

    if env_flag_enabled(NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV) {
        return NodeAdapterResolution::AutoDiscoveryDisabled;
    }

    discover_node_adapter_command()
        .map(NodeAdapterResolution::Command)
        .unwrap_or(NodeAdapterResolution::NotFound)
}

fn parse_adapter_command_value(raw: &str) -> Result<(String, Vec<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("value must not be empty".to_string());
    }

    if trimmed.starts_with('[') {
        let values: Vec<String> =
            serde_json::from_str(trimmed).map_err(|err| format!("invalid JSON array: {err}"))?;
        let Some((program, args)) = values.split_first() else {
            return Err("JSON array must include at least one command token".to_string());
        };
        if program.trim().is_empty() {
            return Err("command token must not be empty".to_string());
        }
        return Ok((program.clone(), args.to_vec()));
    }

    let mut tokens = trimmed
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<String>>();
    if tokens.is_empty() {
        return Err("value must include a command token".to_string());
    }
    let program = tokens.remove(0);
    Ok((program, tokens))
}

fn discover_node_adapter_command() -> Option<NodeAdapterCommand> {
    if command_exists("js-debug-adapter") {
        return Some(NodeAdapterCommand {
            program: "js-debug-adapter".to_string(),
            args: Vec::new(),
            source: NodeAdapterSource::Path,
        });
    }

    if command_exists("node") {
        if let Some(script_path) = discover_vscode_js_debug_server_script() {
            return Some(NodeAdapterCommand {
                program: "node".to_string(),
                args: vec![script_path.to_string_lossy().to_string()],
                source: NodeAdapterSource::VscodeExtension,
            });
        }
    }

    None
}

fn discover_vscode_js_debug_server_script() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    discover_vscode_js_debug_server_script_in_home(Path::new(&home))
}

fn discover_vscode_js_debug_server_script_in_home(home: &Path) -> Option<PathBuf> {
    let roots = [
        home.join(".vscode").join("extensions"),
        home.join(".vscode-insiders").join("extensions"),
        home.join(".cursor").join("extensions"),
    ];

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for root in roots {
        let Ok(read_dir) = fs::read_dir(&root) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("ms-vscode.js-debug") {
                continue;
            }

            let script_path = entry
                .path()
                .join("dist")
                .join("src")
                .join("dapDebugServer.js");
            if !script_path.is_file() {
                continue;
            }

            let modified = entry
                .metadata()
                .and_then(|value| value.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            let should_replace = newest
                .as_ref()
                .map(|(current, _)| modified > *current)
                .unwrap_or(true);
            if should_replace {
                newest = Some((modified, script_path));
            }
        }
    }

    newest.map(|(_, path)| path)
}

fn command_exists(command: &str) -> bool {
    let candidate = Path::new(command);
    if candidate.is_absolute() || command.contains(std::path::MAIN_SEPARATOR) {
        return candidate.is_file();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.join(command).is_file() {
            return true;
        }
    }
    false
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn require_dap_backend(
    runtime: &RuntimeKind,
    require_bootstrap: bool,
) -> Result<DebugBackendKind, AppError> {
    let Some(backend) = DebugBackendKind::for_runtime(runtime) else {
        return Err(AppError::UnsupportedDapRuntime(runtime_label(runtime)));
    };

    if matches!(backend, DebugBackendKind::NodeInspector) {
        let _ = resolve_node_adapter_command()?;
        return Ok(backend);
    }

    if !backend.supports_dap() {
        return Err(AppError::UnsupportedDapRuntime(runtime_label(runtime)));
    }
    if require_bootstrap && !backend.supports_dap_bootstrap() {
        return Err(AppError::UnsupportedDapRuntime(runtime_label(runtime)));
    }
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{discover_vscode_js_debug_server_script_in_home, parse_adapter_command_value};

    #[test]
    fn parse_adapter_command_supports_json_array() {
        let (program, args) =
            parse_adapter_command_value("[\"node\", \"adapter.js\", \"--stdio\"]")
                .expect("json array command should parse");
        assert_eq!(program, "node");
        assert_eq!(args, vec!["adapter.js", "--stdio"]);
    }

    #[test]
    fn parse_adapter_command_supports_plain_command_string() {
        let (program, args) =
            parse_adapter_command_value("node adapter.js --stdio").expect("plain command parse");
        assert_eq!(program, "node");
        assert_eq!(args, vec!["adapter.js", "--stdio"]);
    }

    #[test]
    fn discover_vscode_js_debug_server_script_prefers_existing_extension_layout() {
        let tmp = tempdir().expect("temp dir should exist");
        let script_path = tmp
            .path()
            .join(".vscode")
            .join("extensions")
            .join("ms-vscode.js-debug-1.100.0")
            .join("dist")
            .join("src")
            .join("dapDebugServer.js");
        fs::create_dir_all(script_path.parent().expect("parent should exist"))
            .expect("extension dir should be created");
        fs::write(&script_path, "/* mock */").expect("script should be written");

        let discovered = discover_vscode_js_debug_server_script_in_home(tmp.path())
            .expect("script should be discovered");
        assert_eq!(discovered, script_path);
    }
}
