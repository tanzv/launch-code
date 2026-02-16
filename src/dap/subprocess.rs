use launch_code::model::{
    AppState, DebugConfig, DebugSessionMeta, SessionRecord, SessionStatus, unix_timestamp_secs,
};
use launch_code::state::StateStore;
use serde_json::json;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub(super) struct DebugpyAttachTarget {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) process_id: Option<u32>,
    pub(super) attach_arguments: serde_json::Value,
    pub(super) source_event: serde_json::Value,
}

pub(super) fn latest_debugpy_attach_target(
    events: &[serde_json::Value],
) -> Result<DebugpyAttachTarget, AppError> {
    let mut malformed_attach_error: Option<String> = None;
    for event in events.iter().rev() {
        if event.get("event").and_then(|value| value.as_str()) != Some("debugpyAttach") {
            continue;
        }
        match parse_debugpy_attach_target(event) {
            Ok(value) => return Ok(value),
            Err(err) => malformed_attach_error = Some(err.to_string()),
        }
    }

    if let Some(message) = malformed_attach_error {
        return Err(AppError::Dap(format!(
            "invalid debugpyAttach event: {message}"
        )));
    }

    Err(AppError::Dap(
        "no debugpyAttach event available; poll events and retry".to_string(),
    ))
}

pub(super) fn register_subprocess_session(
    store: &StateStore,
    parent_session_id: &str,
    target: &DebugpyAttachTarget,
    requested_child_session_id: Option<&str>,
) -> Result<String, AppError> {
    let parent_session_id = parent_session_id.to_string();
    let requested_child_session_id = requested_child_session_id.map(str::to_string);
    let target = target.clone();

    store.update::<_, _, AppError>(|state| {
        let parent = state
            .sessions
            .get(&parent_session_id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(parent_session_id.clone()))?;

        let mut child_spec = parent.spec.clone();
        if let Some(debug) = child_spec.debug.as_mut() {
            debug.host = target.host.clone();
            debug.port = target.port;
            debug.wait_for_client = false;
        } else {
            child_spec.debug = Some(DebugConfig {
                host: target.host.clone(),
                port: target.port,
                wait_for_client: false,
                subprocess: true,
            });
        }

        let base = match &requested_child_session_id {
            Some(value) => value.clone(),
            None => match target.process_id {
                Some(pid) => format!("{parent_session_id}-subprocess-{pid}"),
                None => format!("{parent_session_id}-subprocess-{}", target.port),
            },
        };
        let child_session_id =
            unique_session_id(state, &base, requested_child_session_id.is_none())
                .ok_or_else(|| AppError::Dap(format!("session id already exists: {base}")))?;
        let now = unix_timestamp_secs();

        state.sessions.insert(
            child_session_id.clone(),
            SessionRecord {
                id: child_session_id.clone(),
                spec: child_spec,
                status: SessionStatus::Running,
                pid: target.process_id,
                supervisor_pid: parent.pid.or(parent.supervisor_pid),
                log_path: parent.log_path.clone(),
                debug_meta: Some(DebugSessionMeta {
                    host: target.host.clone(),
                    requested_port: target.port,
                    active_port: target.port,
                    fallback_applied: false,
                    reconnect_policy: "auto-retry".to_string(),
                }),
                created_at: now,
                updated_at: now,
                last_exit_code: None,
                restart_count: 0,
            },
        );

        Ok(child_session_id)
    })
}

fn unique_session_id(state: &AppState, base: &str, allow_suffix: bool) -> Option<String> {
    if !state.sessions.contains_key(base) {
        return Some(base.to_string());
    }
    if !allow_suffix {
        return None;
    }

    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !state.sessions.contains_key(&candidate) {
            return Some(candidate);
        }
        suffix += 1;
    }
}

fn parse_debugpy_attach_target(event: &serde_json::Value) -> Result<DebugpyAttachTarget, AppError> {
    if event.get("type").and_then(|value| value.as_str()) != Some("event") {
        return Err(AppError::Dap(
            "event payload type must be `event`".to_string(),
        ));
    }
    if event.get("event").and_then(|value| value.as_str()) != Some("debugpyAttach") {
        return Err(AppError::Dap(
            "event name must be `debugpyAttach`".to_string(),
        ));
    }

    let body = event
        .get("body")
        .and_then(|value| value.as_object())
        .ok_or_else(|| AppError::Dap("debugpyAttach event requires object body".to_string()))?;
    let (host, port) = parse_attach_host_port(body)?;
    let process_id = body
        .get("subProcessId")
        .or_else(|| body.get("processId"))
        .and_then(parse_optional_u32);

    let mut attach_arguments = body.clone();
    for key in [
        "name", "type", "request", "connect", "host", "port", "listen",
    ] {
        attach_arguments.remove(key);
    }
    if attach_arguments.is_empty() {
        attach_arguments.insert("justMyCode".to_string(), json!(false));
    }

    Ok(DebugpyAttachTarget {
        host,
        port,
        process_id,
        attach_arguments: serde_json::Value::Object(attach_arguments),
        source_event: event.clone(),
    })
}

fn parse_attach_host_port(
    body: &serde_json::Map<String, serde_json::Value>,
) -> Result<(String, u16), AppError> {
    if let Some(connect) = body.get("connect").and_then(|value| value.as_object()) {
        let host = connect
            .get("host")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AppError::Dap("connect.host is required".to_string()))?;
        let port = connect
            .get("port")
            .and_then(parse_optional_u16)
            .ok_or_else(|| AppError::Dap("connect.port is required".to_string()))?;
        return Ok((host.to_string(), port));
    }

    let host = body
        .get("host")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AppError::Dap("host is required when connect is missing".to_string()))?;
    let port = body
        .get("port")
        .and_then(parse_optional_u16)
        .ok_or_else(|| AppError::Dap("port is required when connect is missing".to_string()))?;
    Ok((host.to_string(), port))
}

fn parse_optional_u16(value: &serde_json::Value) -> Option<u16> {
    if let Some(num) = value.as_u64() {
        return u16::try_from(num).ok();
    }
    value.as_str().and_then(|text| text.parse::<u16>().ok())
}

fn parse_optional_u32(value: &serde_json::Value) -> Option<u32> {
    if let Some(num) = value.as_u64() {
        return u32::try_from(num).ok();
    }
    value.as_str().and_then(|text| text.parse::<u32>().ok())
}
