mod codec;
mod python_adapter_proxy;
mod shared;
mod subprocess;
mod tcp_proxy;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::model::RuntimeKind;
use serde_json::json;

use launch_code::state::StateStore;

use crate::app::api_get_session;
use crate::error::AppError;

use python_adapter_proxy::PythonAdapterDapProxy;
use tcp_proxy::TcpDapProxy;

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

    if !matches!(session.spec.runtime, RuntimeKind::Python) {
        return Err(AppError::Dap(
            "auto-bootstrap currently supports python debug sessions only".to_string(),
        ));
    }

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

    Ok(vec![
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
    ])
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

    let proxy = DapProxy::connect(host, port)?;
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
    PythonAdapter(Arc<PythonAdapterDapProxy>),
}

impl DapProxy {
    pub(crate) fn connect(host: String, port: u16) -> Result<Arc<Self>, AppError> {
        let inner = TcpDapProxy::connect(host, port)?;
        Ok(Arc::new(Self::Tcp(inner)))
    }

    pub(crate) fn send_batch(
        &self,
        requests: Vec<(String, serde_json::Value)>,
        timeout: Duration,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        match self {
            Self::Tcp(inner) => inner.send_batch(requests, timeout),
            Self::PythonAdapter(inner) => inner.send_batch(requests, timeout),
        }
    }

    pub(crate) fn matches(&self, host: &str, port: u16) -> bool {
        match self {
            Self::Tcp(inner) => inner.matches(host, port),
            Self::PythonAdapter(inner) => inner.matches(host, port),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self {
            Self::Tcp(inner) => inner.is_closed(),
            Self::PythonAdapter(inner) => inner.is_closed(),
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
            Self::PythonAdapter(inner) => inner.send_request(command, arguments, timeout),
        }
    }

    pub(crate) fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
        match self {
            Self::Tcp(inner) => inner.pop_events(max, timeout),
            Self::PythonAdapter(inner) => inner.pop_events(max, timeout),
        }
    }
}
