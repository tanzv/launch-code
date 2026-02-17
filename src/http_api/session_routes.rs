use launch_code::state::StateStore;
use serde_json::json;

use crate::app::{api_restart_session_with_options, api_stop_session_with_options};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_read_json_object_body,
};

pub(super) fn handle_stop(
    store: &StateStore,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => return http_json_body_error(err),
    };
    let (force, grace_timeout_ms) = match parse_force_and_grace(&payload, false, 1500) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match api_stop_session_with_options(store, session_id, force, grace_timeout_ms) {
        Ok(session) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "session": session}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(super) fn handle_restart(
    store: &StateStore,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => return http_json_body_error(err),
    };
    let (force, grace_timeout_ms) = match parse_force_and_grace(&payload, true, 150) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match api_restart_session_with_options(store, session_id, force, grace_timeout_ms) {
        Ok(session) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "session": session}),
        ),
        Err(err) => http_json_error(&err),
    }
}

fn parse_force_and_grace(
    payload: &serde_json::Value,
    default_force: bool,
    default_grace_timeout_ms: u64,
) -> Result<(bool, u64), tiny_http::Response<std::io::Cursor<Vec<u8>>>> {
    let force = match payload.get("force") {
        None => default_force,
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => {
                return Err(http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "force must be a boolean"}),
                ));
            }
        },
    };
    let grace_timeout_ms = match payload.get("grace_timeout_ms") {
        None => default_grace_timeout_ms,
        Some(value) => match value.as_u64() {
            Some(value) => value.min(60_000),
            None => {
                return Err(http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "grace_timeout_ms must be a non-negative integer"}),
                ));
            }
        },
    };
    Ok((force, grace_timeout_ms))
}
