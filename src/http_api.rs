mod dap_routes;
mod debug_routes;

use std::sync::{Arc, Mutex};

use launch_code::state::StateStore;
use serde_json::json;

use crate::app::{
    api_debug_session, api_get_session, api_inspect_session, api_list_sessions,
    api_restart_session, api_resume_session, api_stop_session, api_suspend_session,
};
use crate::dap::DapRegistry;
use crate::http_utils::{
    http_is_authorized, http_json, http_json_error, http_path_segments, http_query_usize,
    http_split_url,
};

pub(crate) fn response_for_request(
    store: &StateStore,
    token: &str,
    serve_state: &Arc<Mutex<DapRegistry>>,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    if !http_is_authorized(request, token) {
        return http_json(
            tiny_http::StatusCode(401),
            json!({"ok": false, "error": "unauthorized"}),
        );
    }

    let url = request.url().to_string();
    let (path, query) = http_split_url(&url);
    let segments = http_path_segments(path);

    match (request.method(), segments.as_slice()) {
        (&tiny_http::Method::Get, ["v1", "health"]) => {
            http_json(tiny_http::StatusCode(200), json!({"ok": true}))
        }
        (&tiny_http::Method::Get, ["v1", "sessions"]) => match api_list_sessions(store) {
            Ok(sessions) => http_json(
                tiny_http::StatusCode(200),
                json!({"ok": true, "sessions": sessions}),
            ),
            Err(err) => http_json_error(&err),
        },
        (&tiny_http::Method::Get, ["v1", "sessions", session_id]) => {
            match api_get_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Get, ["v1", "sessions", session_id, "inspect"]) => {
            let tail = match http_query_usize(query, "tail") {
                Ok(value) => value.unwrap_or(50),
                Err(msg) => {
                    return http_json(
                        tiny_http::StatusCode(400),
                        json!({"ok": false, "error": "bad_request", "message": msg}),
                    );
                }
            };

            match api_inspect_session(store, session_id, tail) {
                Ok(doc) => http_json(tiny_http::StatusCode(200), doc),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Get, ["v1", "sessions", session_id, "debug"]) => {
            match api_debug_session(store, session_id) {
                Ok(doc) => http_json(tiny_http::StatusCode(200), doc),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Get, ["v1", "sessions", session_id, "debug", "threads"]) => {
            debug_routes::handle_debug_threads(store, serve_state, session_id)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "breakpoints"]) => {
            debug_routes::handle_debug_breakpoints(store, serve_state, session_id, request)
        }
        (
            &tiny_http::Method::Post,
            [
                "v1",
                "sessions",
                session_id,
                "debug",
                "exception-breakpoints",
            ],
        ) => debug_routes::handle_debug_exception_breakpoints(
            store,
            serve_state,
            session_id,
            request,
        ),
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "evaluate"]) => {
            debug_routes::handle_debug_evaluate(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "set-variable"]) => {
            debug_routes::handle_debug_set_variable(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "continue"]) => {
            debug_routes::handle_debug_continue(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "pause"]) => {
            debug_routes::handle_debug_pause(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "next"]) => {
            debug_routes::handle_debug_next(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "step-in"]) => {
            debug_routes::handle_debug_step_in(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "step-out"]) => {
            debug_routes::handle_debug_step_out(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "disconnect"]) => {
            debug_routes::handle_debug_disconnect(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "terminate"]) => {
            debug_routes::handle_debug_terminate(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "adopt-subprocess"]) => {
            debug_routes::handle_debug_adopt_subprocess(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Get, ["v1", "sessions", session_id, "debug", "dap", "events"]) => {
            dap_routes::handle_dap_events(store, serve_state, session_id, query)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "debug", "dap", "request"]) => {
            dap_routes::handle_dap_request(store, serve_state, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "stop"]) => {
            match api_stop_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "restart"]) => {
            match api_restart_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "suspend"]) => {
            match api_suspend_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "resume"]) => {
            match api_resume_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Get, ["v1", "sessions", _, _])
        | (&tiny_http::Method::Post, ["v1", "sessions", _, _]) => http_json(
            tiny_http::StatusCode(404),
            json!({"ok": false, "error": "not_found"}),
        ),
        _ => http_json(
            tiny_http::StatusCode(404),
            json!({"ok": false, "error": "not_found"}),
        ),
    }
}
