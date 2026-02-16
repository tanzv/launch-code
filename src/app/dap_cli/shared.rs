use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, send_request_with_retry};
use crate::error::AppError;

pub(super) fn fresh_registry() -> Arc<Mutex<DapRegistry>> {
    Arc::new(Mutex::new(DapRegistry::default()))
}

pub(super) fn clamp_timeout(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms.min(60_000))
}

pub(super) fn print_json_doc(doc: &serde_json::Value) -> Result<(), AppError> {
    println!("{}", serde_json::to_string_pretty(doc)?);
    Ok(())
}

pub(super) fn parse_dap_arguments(input: Option<&str>) -> Result<serde_json::Value, AppError> {
    match input {
        Some(raw) if !raw.trim().is_empty() => {
            let value: serde_json::Value = serde_json::from_str(raw)?;
            if value.is_null() {
                Ok(json!({}))
            } else {
                Ok(value)
            }
        }
        _ => Ok(json!({})),
    }
}

pub(super) fn send_thread_command(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    thread_id: Option<u64>,
    command: &str,
    timeout: Duration,
) -> Result<(u64, serde_json::Value), AppError> {
    let thread_id = resolve_thread_id(store, serve_state, session_id, thread_id, timeout)?;
    let response = send_request_with_retry(
        store,
        serve_state,
        session_id,
        command,
        json!({ "threadId": thread_id }),
        timeout,
    )?;
    Ok((thread_id, response))
}

pub(super) fn extract_first_thread_id(
    threads_response: &serde_json::Value,
) -> Result<u64, AppError> {
    threads_response
        .get("body")
        .and_then(|body| body.get("threads"))
        .and_then(|threads| threads.as_array())
        .and_then(|threads| threads.first())
        .and_then(|thread| thread.get("id"))
        .and_then(|id| id.as_u64())
        .ok_or_else(|| AppError::Dap("no thread returned by debug adapter".to_string()))
}

fn resolve_thread_id(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    thread_id: Option<u64>,
    timeout: Duration,
) -> Result<u64, AppError> {
    match thread_id {
        Some(value) => Ok(value),
        None => {
            let threads_response = send_request_with_retry(
                store,
                serve_state,
                session_id,
                "threads",
                json!({}),
                timeout,
            )?;
            extract_first_thread_id(&threads_response)
        }
    }
}
