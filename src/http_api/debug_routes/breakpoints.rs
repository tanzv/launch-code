use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, send_request_with_retry};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_read_json_object_body,
};

pub(crate) fn handle_debug_breakpoints(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let path = match payload.get("path").and_then(|v| v.as_str()) {
        Some(value) if !value.trim().is_empty() => value.to_string(),
        _ => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "path is required"}),
            );
        }
    };
    let lines = match payload.get("lines").and_then(|v| v.as_array()) {
        Some(value) if !value.is_empty() => value,
        _ => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "lines is required"}),
            );
        }
    };

    let mut breakpoints = Vec::new();
    for item in lines {
        if let Some(line) = item.as_u64() {
            if line == 0 {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "line must be a positive integer"}),
                );
            }
            breakpoints.push(json!({ "line": line }));
            continue;
        }

        let Some(obj) = item.as_object() else {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "lines must be integers or objects"}),
            );
        };

        let Some(line) = obj.get("line").and_then(|v| v.as_u64()) else {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "breakpoint object requires numeric line"}),
            );
        };
        if line == 0 {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "line must be a positive integer"}),
            );
        }

        let mut breakpoint = serde_json::Map::new();
        breakpoint.insert("line".to_string(), json!(line));
        if let Some(condition) = obj.get("condition").and_then(|v| v.as_str()) {
            breakpoint.insert("condition".to_string(), json!(condition));
        }
        if let Some(hit_condition) = obj.get("hitCondition").and_then(|v| v.as_str()) {
            breakpoint.insert("hitCondition".to_string(), json!(hit_condition));
        }
        if let Some(log_message) = obj.get("logMessage").and_then(|v| v.as_str()) {
            breakpoint.insert("logMessage".to_string(), json!(log_message));
        }
        breakpoints.push(serde_json::Value::Object(breakpoint));
    }

    let args = json!({
        "source": { "path": path },
        "breakpoints": breakpoints
    });

    let timeout_ms = match payload.get("timeout_ms") {
        None => 1500,
        Some(value) => match value.as_u64() {
            Some(value) => value,
            None => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "timeout_ms must be a non-negative integer"}),
                );
            }
        },
    };
    let timeout = Duration::from_millis(timeout_ms.min(60_000));
    match send_request_with_retry(
        store,
        serve_state,
        session_id,
        "setBreakpoints",
        args,
        timeout,
    ) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(crate) fn handle_debug_exception_breakpoints(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let filters = match payload.get("filters") {
        Some(value) => match value.as_array() {
            Some(items) => items
                .iter()
                .map(|item| item.as_str().map(|text| text.to_string()))
                .collect::<Option<Vec<String>>>(),
            None => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "filters must be an array of strings"}),
                );
            }
        },
        None => Some(Vec::new()),
    };

    let Some(filters) = filters else {
        return http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": "filters must be an array of strings"}),
        );
    };

    let timeout_ms = match payload.get("timeout_ms") {
        None => 1500,
        Some(value) => match value.as_u64() {
            Some(value) => value,
            None => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "timeout_ms must be a non-negative integer"}),
                );
            }
        },
    };
    let timeout = Duration::from_millis(timeout_ms.min(60_000));
    match send_request_with_retry(
        store,
        serve_state,
        session_id,
        "setExceptionBreakpoints",
        json!({ "filters": filters }),
        timeout,
    ) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}
