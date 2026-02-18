use launch_code::state::StateStore;
use serde_json::json;

use crate::app::{
    api_cleanup_sessions, api_restart_session_with_options, api_stop_session_with_options,
};
use crate::http_utils::{
    http_json, http_json_body_error, http_json_error, http_read_json_object_body,
};

type HttpJsonResponse = tiny_http::Response<std::io::Cursor<Vec<u8>>>;
type CleanupPayload = (Vec<launch_code::model::SessionStatus>, bool);

pub(super) fn handle_stop(
    store: &StateStore,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> HttpJsonResponse {
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
) -> HttpJsonResponse {
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

pub(super) fn handle_cleanup(
    store: &StateStore,
    request: &mut tiny_http::Request,
) -> HttpJsonResponse {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => return http_json_body_error(err),
    };

    let (statuses, dry_run) = match parse_cleanup_payload(&payload) {
        Ok(value) => value,
        Err(response) => return response,
    };

    match api_cleanup_sessions(store, &statuses, dry_run) {
        Ok(result) => {
            let matched_count = result.matched_session_ids.len();
            let removed_count = result.removed_session_ids.len();
            http_json(
                tiny_http::StatusCode(200),
                json!({
                    "ok": true,
                    "dry_run": result.dry_run,
                    "matched_count": matched_count,
                    "removed_count": removed_count,
                    "kept_count": result.kept_count,
                    "matched_session_ids": result.matched_session_ids,
                    "removed_session_ids": result.removed_session_ids,
                }),
            )
        }
        Err(err) => http_json_error(&err),
    }
}

fn parse_force_and_grace(
    payload: &serde_json::Value,
    default_force: bool,
    default_grace_timeout_ms: u64,
) -> Result<(bool, u64), HttpJsonResponse> {
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

fn parse_cleanup_payload(payload: &serde_json::Value) -> Result<CleanupPayload, HttpJsonResponse> {
    let dry_run = match payload.get("dry_run") {
        None => false,
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => {
                return Err(http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "dry_run must be a boolean"}),
                ));
            }
        },
    };

    let statuses = match payload.get("statuses") {
        None => vec![
            launch_code::model::SessionStatus::Stopped,
            launch_code::model::SessionStatus::Unknown,
        ],
        Some(value) => {
            let Some(items) = value.as_array() else {
                return Err(http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "statuses must be an array"}),
                ));
            };

            if items.is_empty() {
                return Err(http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "statuses must not be empty"}),
                ));
            }

            let mut statuses = Vec::with_capacity(items.len());
            for item in items {
                let Some(status_raw) = item.as_str() else {
                    return Err(http_json(
                        tiny_http::StatusCode(400),
                        json!({"ok": false, "error": "bad_request", "message": "statuses entries must be strings"}),
                    ));
                };
                let normalized = status_raw.trim().to_ascii_lowercase();
                let status = match normalized.as_str() {
                    "stopped" => launch_code::model::SessionStatus::Stopped,
                    "unknown" => launch_code::model::SessionStatus::Unknown,
                    _ => {
                        return Err(http_json(
                            tiny_http::StatusCode(400),
                            json!({"ok": false, "error": "bad_request", "message": format!("unsupported cleanup status: {status_raw}")}),
                        ));
                    }
                };
                if !statuses.contains(&status) {
                    statuses.push(status);
                }
            }
            statuses
        }
    };

    Ok((statuses, dry_run))
}
