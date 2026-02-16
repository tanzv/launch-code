use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, adopt_debugpy_subprocess, send_request_with_retry};
use crate::http_utils::{http_json, http_json_body_error, http_json_error, http_read_json_body};

pub(crate) fn handle_debug_disconnect(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let timeout = match parse_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let terminate_debuggee = match parse_optional_bool(&payload, "terminateDebuggee", false) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let suspend_debuggee = match parse_optional_bool(&payload, "suspendDebuggee", false) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let args = json!({
        "terminateDebuggee": terminate_debuggee,
        "suspendDebuggee": suspend_debuggee
    });

    match send_request_with_retry(store, serve_state, session_id, "disconnect", args, timeout) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(crate) fn handle_debug_terminate(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let timeout = match parse_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let restart = match parse_optional_bool(&payload, "restart", false) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let args = json!({
        "restart": restart
    });

    match send_request_with_retry(store, serve_state, session_id, "terminate", args, timeout) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(crate) fn handle_debug_adopt_subprocess(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let timeout = match parse_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let bootstrap_timeout = match parse_optional_timeout_ms(&payload, "bootstrap_timeout_ms", 5000)
    {
        Ok(value) => value,
        Err(response) => return response,
    };
    let max_events = match parse_optional_usize(&payload, "max_events", 50, usize::MAX) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let child_session_id = match parse_optional_non_empty_string(&payload, "childSessionId") {
        Ok(value) => value,
        Err(response) => return response,
    };

    match adopt_debugpy_subprocess(
        store,
        serve_state,
        session_id,
        timeout,
        max_events,
        bootstrap_timeout,
        child_session_id.as_deref(),
    ) {
        Ok(adopted) => http_json(
            tiny_http::StatusCode(200),
            json!({
                "ok": true,
                "parent_session_id": session_id,
                "child_session_id": adopted.child_session_id,
                "endpoint": format!("{}:{}", adopted.host, adopted.port),
                "process_id": adopted.process_id,
                "source_event": adopted.source_event,
                "bootstrap": {
                    "responses": adopted.bootstrap_responses
                }
            }),
        ),
        Err(err) => http_json_error(&err),
    }
}

type HttpResponse = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn bad_request(message: impl Into<String>) -> HttpResponse {
    http_json(
        tiny_http::StatusCode(400),
        json!({"ok": false, "error": "bad_request", "message": message.into()}),
    )
}

fn parse_optional_timeout_ms(
    payload: &serde_json::Value,
    key: &str,
    default: u64,
) -> Result<Duration, HttpResponse> {
    let value = match payload.get(key) {
        None => default,
        Some(value) => match value.as_u64() {
            Some(value) => value,
            None => return Err(bad_request(format!("{key} must be a non-negative integer"))),
        },
    };
    Ok(Duration::from_millis(value.min(60_000)))
}

fn parse_optional_bool(
    payload: &serde_json::Value,
    key: &str,
    default: bool,
) -> Result<bool, HttpResponse> {
    match payload.get(key) {
        None => Ok(default),
        Some(value) => match value.as_bool() {
            Some(value) => Ok(value),
            None => Err(bad_request(format!("{key} must be a boolean"))),
        },
    }
}

fn parse_optional_usize(
    payload: &serde_json::Value,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, HttpResponse> {
    let value = match payload.get(key) {
        None => default as u64,
        Some(value) => match value.as_u64() {
            Some(value) => value,
            None => return Err(bad_request(format!("{key} must be a non-negative integer"))),
        },
    };

    let capped = value.min(max as u64);
    usize::try_from(capped).map_err(|_| bad_request(format!("{key} is out of range")))
}

fn parse_optional_non_empty_string(
    payload: &serde_json::Value,
    key: &str,
) -> Result<Option<String>, HttpResponse> {
    match payload.get(key) {
        None => Ok(None),
        Some(value) => match value.as_str() {
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            None => Err(bad_request(format!("{key} must be a string"))),
        },
    }
}
