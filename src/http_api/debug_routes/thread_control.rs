use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::dap::{DapRegistry, send_request_with_retry};
use crate::http_utils::{http_json, http_json_body_error, http_json_error, http_read_json_body};

pub(crate) fn handle_debug_threads(
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

pub(crate) fn handle_debug_continue(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "continue")
}

pub(crate) fn handle_debug_pause(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "pause")
}

pub(crate) fn handle_debug_next(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "next")
}

pub(crate) fn handle_debug_step_in(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    handle_debug_thread_control(store, serve_state, session_id, request, "stepIn")
}

pub(crate) fn handle_debug_step_out(
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
        Err(err) => {
            return http_json_body_error(err);
        }
    };

    let timeout = Duration::from_millis(1500);
    let thread_id = match payload.get("threadId") {
        Some(value) => match value.as_u64() {
            Some(thread_id) => thread_id,
            None => {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({"ok": false, "error": "bad_request", "message": "threadId must be a non-negative integer"}),
                );
            }
        },
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
