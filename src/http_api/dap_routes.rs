use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, proxy_for_session, send_batch_with_retry, send_request_with_retry};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_optional_timeout_ms, http_query_u64,
    http_query_usize, http_read_json_object_body,
};

const MAX_DAP_BATCH_REQUESTS: usize = 128;
const MAX_DAP_EVENTS_QUERY_MAX: usize = 1000;

pub(super) fn handle_dap_request(
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

    let timeout = match http_optional_timeout_ms(&payload, "timeout_ms", 1500) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };

    if let Some(batch_value) = payload.get("batch") {
        if payload.get("command").is_some() {
            return bad_request("command cannot be combined with batch");
        }

        let Some(batch) = batch_value.as_array() else {
            return bad_request("batch must be an array");
        };

        if batch.is_empty() {
            return bad_request("batch must not be empty");
        }
        if batch.len() > MAX_DAP_BATCH_REQUESTS {
            return bad_request(format!(
                "batch must contain at most {MAX_DAP_BATCH_REQUESTS} requests"
            ));
        }

        let mut requests = Vec::with_capacity(batch.len());
        for item in batch {
            let Some(_) = item.as_object() else {
                return bad_request("batch items must be objects");
            };

            let command = match item.get("command").and_then(|v| v.as_str()) {
                Some(value) if !value.trim().is_empty() => value.trim().to_string(),
                _ => return bad_request("batch items require command"),
            };

            let arguments = match parse_optional_arguments(item, "arguments") {
                Ok(value) => value,
                Err(response) => return response,
            };

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
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => return bad_request("command is required"),
        };

        let arguments = match parse_optional_arguments(&payload, "arguments") {
            Ok(value) => value,
            Err(response) => return response,
        };

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

type HttpResponse = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn bad_request(message: impl Into<String>) -> HttpResponse {
    http_json(
        tiny_http::StatusCode(400),
        json!({"ok": false, "error": "bad_request", "message": message.into()}),
    )
}

fn parse_optional_arguments(
    payload: &serde_json::Value,
    key: &str,
) -> Result<serde_json::Value, HttpResponse> {
    let value = match payload.get(key) {
        Some(value) => value,
        None => return Ok(json!({})),
    };

    if value.is_null() {
        return Ok(json!({}));
    }
    if value.is_object() {
        return Ok(value.clone());
    }

    Err(bad_request(format!("{key} must be an object")))
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
    if max == 0 {
        return http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": "invalid query parameter: max should be >= 1"}),
        );
    }
    if max > MAX_DAP_EVENTS_QUERY_MAX {
        return http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": format!("invalid query parameter: max should be <= {MAX_DAP_EVENTS_QUERY_MAX}")}),
        );
    }
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
