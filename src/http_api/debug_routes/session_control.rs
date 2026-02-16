use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, adopt_debugpy_subprocess, send_request_with_retry};
use crate::http_utils::{http_json, http_json_error, http_read_json_body};

pub(crate) fn handle_debug_disconnect(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_body(request) {
        Ok(value) => value,
        Err(msg) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": msg}),
            );
        }
    };

    let timeout = Duration::from_millis(
        payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .min(60_000),
    );
    let args = json!({
        "terminateDebuggee": payload.get("terminateDebuggee").and_then(|v| v.as_bool()).unwrap_or(false),
        "suspendDebuggee": payload.get("suspendDebuggee").and_then(|v| v.as_bool()).unwrap_or(false)
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
        Err(msg) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": msg}),
            );
        }
    };

    let timeout = Duration::from_millis(
        payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .min(60_000),
    );
    let args = json!({
        "restart": payload.get("restart").and_then(|v| v.as_bool()).unwrap_or(false)
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
        Err(msg) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": msg}),
            );
        }
    };

    let timeout = Duration::from_millis(
        payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .min(60_000),
    );
    let bootstrap_timeout = Duration::from_millis(
        payload
            .get("bootstrap_timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000)
            .min(60_000),
    );
    let max_events = payload
        .get("max_events")
        .and_then(|v| v.as_u64())
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(50);
    let child_session_id = payload
        .get("childSessionId")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty());

    match adopt_debugpy_subprocess(
        store,
        serve_state,
        session_id,
        timeout,
        max_events,
        bootstrap_timeout,
        child_session_id,
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
