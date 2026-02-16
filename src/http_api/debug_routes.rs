use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, adopt_debugpy_subprocess, send_request_with_retry};
use crate::http_utils::{http_json, http_json_error, http_read_json_body};

pub(super) fn handle_debug_threads(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let timeout = Duration::from_millis(1500);
    match send_request_with_retry(
        store,
        serve_state,
        session_id,
        "threads",
        json!({}),
        timeout,
    ) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(super) fn handle_debug_breakpoints(
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

    let timeout = Duration::from_millis(1500);
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

pub(super) fn handle_debug_exception_breakpoints(
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

    let filters = match payload.get("filters").and_then(|v| v.as_array()) {
        Some(value) => value
            .iter()
            .map(|item| item.as_str().map(|text| text.to_string()))
            .collect::<Option<Vec<String>>>(),
        None => Some(Vec::new()),
    };

    let Some(filters) = filters else {
        return http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": "filters must be an array of strings"}),
        );
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

pub(super) fn handle_debug_evaluate(
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

pub(super) fn handle_debug_set_variable(
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

pub(super) fn handle_debug_continue(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "continue")
}

pub(super) fn handle_debug_pause(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "pause")
}

pub(super) fn handle_debug_next(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "next")
}

pub(super) fn handle_debug_step_in(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "stepIn")
}

pub(super) fn handle_debug_step_out(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "stepOut")
}

fn handle_debug_thread_control(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
    command: &str,
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

    let timeout = Duration::from_millis(1500);
    let thread_id = match payload.get("threadId").and_then(|v| v.as_u64()) {
        Some(value) => value,
        None => {
            let threads_response = match send_request_with_retry(
                store,
                serve_state,
                session_id,
                "threads",
                json!({}),
                timeout,
            ) {
                Ok(value) => value,
                Err(err) => return http_json_error(&err),
            };

            match threads_response
                .get("body")
                .and_then(|body| body.get("threads"))
                .and_then(|threads| threads.as_array())
                .and_then(|threads| threads.first())
                .and_then(|thread| thread.get("id"))
                .and_then(|id| id.as_u64())
            {
                Some(value) => value,
                None => {
                    return http_json(
                        tiny_http::StatusCode(409),
                        json!({"ok": false, "error": "no_threads", "message": "no threads reported by debug adapter"}),
                    );
                }
            }
        }
    };

    let args = json!({ "threadId": thread_id });
    match send_request_with_retry(store, serve_state, session_id, command, args, timeout) {
        Ok(response) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "thread_id": thread_id, "response": response}),
        ),
        Err(err) => http_json_error(&err),
    }
}

pub(super) fn handle_debug_disconnect(
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

pub(super) fn handle_debug_terminate(
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

pub(super) fn handle_debug_adopt_subprocess(
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
