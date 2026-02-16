use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, send_request_with_retry};
use crate::http_utils::{http_json, http_json_error, http_read_json_body};

pub(crate) fn handle_debug_evaluate(
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

    let expression = match payload.get("expression").and_then(|v| v.as_str()) {
        Some(value) if !value.trim().is_empty() => value.to_string(),
        _ => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "expression is required"}),
            );
        }
    };

    let mut args = serde_json::Map::new();
    args.insert("expression".to_string(), json!(expression));
    if let Some(frame_id) = payload.get("frameId").and_then(|v| v.as_u64()) {
        args.insert("frameId".to_string(), json!(frame_id));
    }
    if let Some(context) = payload.get("context").and_then(|v| v.as_str()) {
        args.insert("context".to_string(), json!(context));
    }

    let timeout = Duration::from_millis(
        payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .min(60_000),
    );

    match send_request_with_retry(
        store,
        serve_state,
        session_id,
        "evaluate",
        serde_json::Value::Object(args),
        timeout,
    ) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(crate) fn handle_debug_set_variable(
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

    let variables_reference = match payload.get("variablesReference").and_then(|v| v.as_u64()) {
        Some(value) => value,
        None => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "variablesReference is required"}),
            );
        }
    };
    let name = match payload.get("name").and_then(|v| v.as_str()) {
        Some(value) if !value.trim().is_empty() => value.to_string(),
        _ => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "name is required"}),
            );
        }
    };
    let value = match payload.get("value").and_then(|v| v.as_str()) {
        Some(value) => value.to_string(),
        None => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "value is required"}),
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
    match send_request_with_retry(
        store,
        serve_state,
        session_id,
        "setVariable",
        json!({
            "variablesReference": variables_reference,
            "name": name,
            "value": value
        }),
        timeout,
    ) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}
