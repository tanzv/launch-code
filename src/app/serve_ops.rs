use std::fs;
use std::io::Write;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::ServeArgs;
use crate::dap::DapRegistry;
use crate::error::AppError;
use crate::output;

pub(super) fn handle_serve(store: &StateStore, args: &ServeArgs) -> Result<(), AppError> {
    let token = resolve_serve_token(args)?;
    let max_body_bytes = args.max_body_bytes.clamp(1024, 16_777_216);
    crate::http_utils::set_http_max_json_body_bytes(max_body_bytes);
    let server =
        tiny_http::Server::http(&args.bind).map_err(|err| AppError::Http(err.to_string()))?;
    let url = format!("http://{}", server.server_addr());
    output::print_message(&format!("listening={url}"));
    std::io::stdout().flush()?;

    let worker_count = args.workers.clamp(1, 256);
    let queue_capacity = args.queue_capacity.min(4096);
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let (sender, receiver) = mpsc::sync_channel::<tiny_http::Request>(queue_capacity);
    let receiver = Arc::new(Mutex::new(receiver));

    let mut workers = Vec::new();
    for _ in 0..worker_count {
        let store = store.clone();
        let token = token.clone();
        let serve_state = Arc::clone(&serve_state);
        let receiver = Arc::clone(&receiver);
        workers.push(thread::spawn(move || {
            loop {
                let request = {
                    let guard = match receiver.lock() {
                        Ok(value) => value,
                        Err(_) => break,
                    };
                    match guard.recv() {
                        Ok(request) => request,
                        Err(_) => break,
                    }
                };

                let mut request = request;
                let response = crate::http_api::response_for_request(
                    &store,
                    &token,
                    &serve_state,
                    &mut request,
                );
                let _ = request.respond(response);
            }
        }));
    }

    for request in server.incoming_requests() {
        match sender.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(request)) => {
                let method = request.method().as_str().to_string();
                let url = request.url().to_string();
                let response = crate::http_utils::http_json(
                    tiny_http::StatusCode(503),
                    json!({
                        "ok": false,
                        "error": "server_overloaded",
                        "message": "request queue is full",
                    }),
                );
                let response = match tiny_http::Header::from_bytes("Retry-After", "1") {
                    Ok(header) => response.with_header(header),
                    Err(_) => response,
                };
                let _ = request.respond(response);
                crate::http_api::observe_response(&method, &url, 503, Duration::from_millis(0));
            }
            Err(TrySendError::Disconnected(_)) => {
                break;
            }
        }
    }

    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }

    Ok(())
}

fn resolve_serve_token(args: &ServeArgs) -> Result<String, AppError> {
    if let Some(token) = args.token.as_ref().map(|value| value.trim().to_string()) {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    if let Some(path) = &args.token_file {
        let raw = fs::read_to_string(path)?;
        let token = raw
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or_else(|| {
                AppError::Http(format!("token file is empty: {}", path.to_string_lossy()))
            })?;
        return Ok(token.to_string());
    }

    if let Ok(token) = std::env::var("LAUNCH_CODE_HTTP_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    Err(AppError::Http(
        "missing serve token: provide --token, --token-file, or LAUNCH_CODE_HTTP_TOKEN".to_string(),
    ))
}
