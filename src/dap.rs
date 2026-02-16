mod codec;
mod python_adapter_proxy;
mod shared;
mod subprocess;
mod tcp_proxy;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use launch_code::state::StateStore;

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
    let proxy = proxy_for_session(store, registry, session_id)?;
    let retry_arguments = arguments.clone();
    match proxy.send_request(command, arguments, timeout) {
        Ok(response) => Ok(response),
        Err(err) => {
            let should_retry = proxy.is_closed();
            drop_session(registry, session_id);
            if should_retry {
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
            let should_retry = proxy.is_closed();
            drop_session(registry, session_id);
            if should_retry {
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
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
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
