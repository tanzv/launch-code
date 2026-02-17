use std::sync::{Arc, Mutex};

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, send_request_with_retry};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_optional_timeout_ms,
    http_read_json_object_body,
};

pub(crate) fn handle_debug_evaluate(
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
    if let Some(frame_id_value) = payload.get("frameId") {
        match frame_id_value.as_u64() {
            Some(frame_id) if frame_id > 0 => {
                args.insert("frameId".to_string(), json!(frame_id));
            }
            _ => {
                return bad_request("frameId must be a positive integer");
            }
        }
    }
    if let Some(context_value) = payload.get("context") {
        match context_value.as_str() {
            Some(context) => {
                args.insert("context".to_string(), json!(context));
            }
            None => {
                return bad_request("context must be a string");
            }
        }
    }

    let timeout = match http_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };

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
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let variables_reference = match payload.get("variablesReference") {
        Some(value) => match value.as_u64() {
            Some(value) if value > 0 => value,
            _ => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "variablesReference must be a positive integer"}),
                );
            }
        },
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

    let timeout = match http_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };
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

type HttpResponse = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn bad_request(message: impl Into<String>) -> HttpResponse {
    http_json(
        tiny_http::StatusCode(400),
        json!({"ok": false, "error": "bad_request", "message": message.into()}),
    )
}
