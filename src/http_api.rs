mod dap_routes;
mod debug_routes;
mod project_routes;
mod session_routes;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use launch_code::state::StateStore;
use serde_json::json;

use crate::app::{
    api_debug_session, api_get_session, api_inspect_session, api_list_sessions, api_resume_session,
    api_suspend_session,
};
use crate::dap::DapRegistry;
use crate::http_utils::{
    http_is_authorized, http_json, http_json_body_error, http_json_error, http_path_segments,
    http_query_usize, http_read_json_object_body, http_split_url,
};

static HTTP_SERVER_STARTED_AT: OnceLock<Instant> = OnceLock::new();
static HTTP_METRICS: HttpMetrics = HttpMetrics::new();
const MAX_HTTP_INSPECT_TAIL_LINES: usize = 5000;

struct InflightGuard;

impl InflightGuard {
    fn new() -> Self {
        HTTP_METRICS
            .requests_inflight
            .fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        HTTP_METRICS
            .requests_inflight
            .fetch_sub(1, Ordering::Relaxed);
    }
}

struct HttpMetrics {
    requests_total: AtomicU64,
    requests_inflight: AtomicU64,
    responses_2xx: AtomicU64,
    responses_4xx: AtomicU64,
    responses_5xx: AtomicU64,
    responses_401: AtomicU64,
    responses_404: AtomicU64,
    responses_409: AtomicU64,
    responses_503: AtomicU64,
    total_duration_micros: AtomicU64,
}

impl HttpMetrics {
    const fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            requests_inflight: AtomicU64::new(0),
            responses_2xx: AtomicU64::new(0),
            responses_4xx: AtomicU64::new(0),
            responses_5xx: AtomicU64::new(0),
            responses_401: AtomicU64::new(0),
            responses_404: AtomicU64::new(0),
            responses_409: AtomicU64::new(0),
            responses_503: AtomicU64::new(0),
            total_duration_micros: AtomicU64::new(0),
        }
    }
}

pub(crate) fn response_for_request(
    store: &StateStore,
    token: &str,
    serve_state: &Arc<Mutex<DapRegistry>>,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let _ = HTTP_SERVER_STARTED_AT.get_or_init(Instant::now);
    let _inflight = InflightGuard::new();
    let method = request.method().as_str().to_string();
    let url = request.url().to_string();
    let (path, _) = http_split_url(&url);
    let started = Instant::now();

    let response = response_for_request_inner(store, token, serve_state, request);
    let status = response.status_code().0;
    let elapsed = started.elapsed();
    observe_response(&method, path, status, elapsed);

    response
}

pub(crate) fn observe_response(method: &str, path_or_url: &str, status: u16, elapsed: Duration) {
    let (path, _) = http_split_url(path_or_url);
    record_http_metrics(status, elapsed);
    log_http_access(method, path, status, elapsed);
}

fn response_for_request_inner(
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
        (&tiny_http::Method::Get, ["v1", "metrics"]) => {
            http_json(tiny_http::StatusCode(200), build_metrics_doc())
        }
        (&tiny_http::Method::Get, ["v1", "project"]) => project_routes::handle_project_get(store),
        (&tiny_http::Method::Put, ["v1", "project"]) => {
            project_routes::handle_project_put_or_patch(store, request)
        }
        (&tiny_http::Method::Patch, ["v1", "project"]) => {
            project_routes::handle_project_put_or_patch(store, request)
        }
        (&tiny_http::Method::Delete, ["v1", "project"]) => {
            project_routes::handle_project_delete(store, request)
        }
        (&tiny_http::Method::Get, ["v1", "sessions"]) => match api_list_sessions(store) {
            Ok(sessions) => http_json(
                tiny_http::StatusCode(200),
                json!({"ok": true, "sessions": sessions}),
            ),
            Err(err) => http_json_error(&err),
        },
        (&tiny_http::Method::Post, ["v1", "sessions", "cleanup"]) => {
            session_routes::handle_cleanup(store, request)
        }
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
            if tail > MAX_HTTP_INSPECT_TAIL_LINES {
                return http_json(
                    tiny_http::StatusCode(400),
                    json!({
                        "ok": false,
                        "error": "bad_request",
                        "message": format!("invalid query parameter: tail should be <= {MAX_HTTP_INSPECT_TAIL_LINES}"),
                    }),
                );
            }

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
            debug_routes::handle_debug_threads(store, serve_state, session_id, query)
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
            session_routes::handle_stop(store, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "restart"]) => {
            session_routes::handle_restart(store, session_id, request)
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "suspend"]) => {
            if let Err(response) = ensure_json_payload(request) {
                return response;
            }
            match api_suspend_session(store, session_id) {
                Ok(session) => http_json(
                    tiny_http::StatusCode(200),
                    json!({"ok": true, "session": session}),
                ),
                Err(err) => http_json_error(&err),
            }
        }
        (&tiny_http::Method::Post, ["v1", "sessions", session_id, "resume"]) => {
            if let Err(response) = ensure_json_payload(request) {
                return response;
            }
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

fn record_http_metrics(status: u16, duration: Duration) {
    HTTP_METRICS.requests_total.fetch_add(1, Ordering::Relaxed);
    match status / 100 {
        2 => {
            HTTP_METRICS.responses_2xx.fetch_add(1, Ordering::Relaxed);
        }
        4 => {
            HTTP_METRICS.responses_4xx.fetch_add(1, Ordering::Relaxed);
        }
        5 => {
            HTTP_METRICS.responses_5xx.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }

    if status == 401 {
        HTTP_METRICS.responses_401.fetch_add(1, Ordering::Relaxed);
    }
    if status == 404 {
        HTTP_METRICS.responses_404.fetch_add(1, Ordering::Relaxed);
    }
    if status == 409 {
        HTTP_METRICS.responses_409.fetch_add(1, Ordering::Relaxed);
    }
    if status == 503 {
        HTTP_METRICS.responses_503.fetch_add(1, Ordering::Relaxed);
    }

    let duration_micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    HTTP_METRICS
        .total_duration_micros
        .fetch_add(duration_micros, Ordering::Relaxed);
}

fn build_metrics_doc() -> serde_json::Value {
    let started = HTTP_SERVER_STARTED_AT.get_or_init(Instant::now);
    let uptime_secs = started.elapsed().as_secs();
    let requests_total = HTTP_METRICS.requests_total.load(Ordering::Relaxed);
    let total_duration_micros = HTTP_METRICS.total_duration_micros.load(Ordering::Relaxed);
    let average_latency_ms = if requests_total == 0 {
        0.0
    } else {
        (total_duration_micros as f64) / (requests_total as f64) / 1000.0
    };

    json!({
        "ok": true,
        "metrics": {
            "uptime_seconds": uptime_secs,
            "requests_total": requests_total,
            "requests_inflight": HTTP_METRICS.requests_inflight.load(Ordering::Relaxed),
            "responses": {
                "2xx": HTTP_METRICS.responses_2xx.load(Ordering::Relaxed),
                "4xx": HTTP_METRICS.responses_4xx.load(Ordering::Relaxed),
                "5xx": HTTP_METRICS.responses_5xx.load(Ordering::Relaxed),
                "401": HTTP_METRICS.responses_401.load(Ordering::Relaxed),
                "404": HTTP_METRICS.responses_404.load(Ordering::Relaxed),
                "409": HTTP_METRICS.responses_409.load(Ordering::Relaxed),
                "503": HTTP_METRICS.responses_503.load(Ordering::Relaxed),
            },
            "latency": {
                "total_micros": total_duration_micros,
                "average_ms": average_latency_ms
            }
        }
    })
}

fn log_http_access(method: &str, path: &str, status: u16, elapsed: Duration) {
    println!(
        "http_access method={method} path={path} status={status} duration_ms={}",
        elapsed.as_millis()
    );
}

fn ensure_json_payload(
    request: &mut tiny_http::Request,
) -> Result<(), tiny_http::Response<std::io::Cursor<Vec<u8>>>> {
    match http_read_json_object_body(request) {
        Ok(_) => Ok(()),
        Err(err) => Err(http_json_body_error(err)),
    }
}
