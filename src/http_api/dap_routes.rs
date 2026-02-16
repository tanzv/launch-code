use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, proxy_for_session, send_batch_with_retry, send_request_with_retry};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_query_u64, http_query_usize,
    http_read_json_body,
};

pub(super) fn handle_dap_request(
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

    let timeout = Duration::from_millis(
        payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .min(60_000),
    );

    if let Some(batch) = payload.get("batch").and_then(|v| v.as_array()) {
        if batch.is_empty() {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": "batch must not be empty"}),
            );
        }

        let mut requests = Vec::with_capacity(batch.len());
        for item in batch {
            let command = match item.get("command").and_then(|v| v.as_str()) {
                Some(value) if !value.trim().is_empty() => value.to_string(),
                _ => {
                    return http_json(
                        tiny_http::StatusCode(400),
                        json!({"ok": false, "error": "bad_request", "message": "batch items require command"}),
                    );
                }
            };

            let mut arguments = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if arguments.is_null() {
                arguments = json!({});
            }

            requests.push((command, arguments));
        }

        match send_batch_with_retry(store, serve_state, session_id, requests, timeout) {
            Ok(responses) => http_json(
                tiny_http::StatusCode(200),
                json!({"ok": true, "responses": responses}),
            ),
            Err(err) => http_json_error(&err),
        }
    } else {
        let command = match payload.get("command").and_then(|v| v.as_str()) {
            Some(value) if !value.trim().is_empty() => value.to_string(),
            _ => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "command is required"}),
                );
            }
        };

        let mut arguments = payload
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if arguments.is_null() {
            arguments = json!({});
        }

        match send_request_with_retry(store, serve_state, session_id, &command, arguments, timeout)
        {
            Ok(response) => http_json(
                tiny_http::StatusCode(200),
                json!({"ok": true, "response": response}),
            ),
            Err(err) => http_json_error(&err),
        }
    }
}

pub(super) fn handle_dap_events(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    query: Option<&str>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let max = match http_query_usize(query, "max") {
        Ok(value) => value.unwrap_or(50),
        Err(msg) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": msg}),
            );
        }
    };
    let timeout_ms = match http_query_u64(query, "timeout_ms") {
        Ok(value) => value.unwrap_or(0),
        Err(msg) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": msg}),
            );
        }
    };
    let timeout = Duration::from_millis(timeout_ms.min(60_000));

    let proxy = match proxy_for_session(store, serve_state, session_id) {
        Ok(value) => value,
        Err(err) => return http_json_error(&err),
    };

    let events = proxy.pop_events(max, timeout);
    http_json(
        tiny_http::StatusCode(200),
        json!({"ok": true, "events": events}),
    )
}
