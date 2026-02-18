use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::model::{SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{DoctorArgs, DoctorCommands, DoctorDebugArgs};
use crate::dap::{DapRegistry, proxy_for_session, send_request_with_retry};
use crate::error::AppError;
use crate::output;

const MAX_DOCTOR_EVENTS: usize = 1000;

pub(super) fn handle_doctor(store: &StateStore, args: &DoctorArgs) -> Result<(), AppError> {
    match &args.command {
        DoctorCommands::Debug(req) => handle_doctor_debug(store, req),
    }
}

fn handle_doctor_debug(store: &StateStore, args: &DoctorDebugArgs) -> Result<(), AppError> {
    let session = super::api_get_session(store, &args.id)?;
    let inspect = super::api_inspect_session(store, &args.id, args.tail)?;

    let timeout = clamp_timeout(args.timeout_ms);
    let max_events = args.max_events.clamp(1, MAX_DOCTOR_EVENTS);
    let registry = Arc::new(Mutex::new(DapRegistry::default()));

    let threads =
        match send_request_with_retry(store, &registry, &args.id, "threads", json!({}), timeout) {
            Ok(response) => match dap_failure_message(&response) {
                Some(message) => json!({
                    "ok": false,
                    "error": "dap_error",
                    "message": message,
                    "response": response
                }),
                None => json!({
                    "ok": true,
                    "response": response
                }),
            },
            Err(err) => error_doc(&err),
        };

    let events = match proxy_for_session(store, &registry, &args.id) {
        Ok(proxy) => {
            let items = proxy.pop_events(max_events, timeout);
            json!({
                "ok": true,
                "count": items.len(),
                "items": items
            })
        }
        Err(err) => error_doc(&err),
    };

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "session": session,
        "inspect": inspect,
        "debug": {
            "threads": threads,
            "events": events
        }
    });

    if output::is_json_mode() {
        output::print_json_doc(&doc);
        return Ok(());
    }

    print_text_summary(&session, &threads, &events);
    Ok(())
}

fn clamp_timeout(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms.min(60_000))
}

fn error_doc(err: &AppError) -> serde_json::Value {
    json!({
        "ok": false,
        "error": err.code(),
        "message": err.to_string()
    })
}

fn dap_failure_message(response: &serde_json::Value) -> Option<String> {
    if !matches!(
        response.get("success").and_then(|value| value.as_bool()),
        Some(false)
    ) {
        return None;
    }

    response
        .get("message")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| {
            response
                .get("command")
                .and_then(|value| value.as_str())
                .map(|command| format!("dap command failed: {command}"))
        })
}

fn print_text_summary(
    session: &SessionRecord,
    threads: &serde_json::Value,
    events: &serde_json::Value,
) {
    let pid = session
        .pid
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    println!(
        "doctor_debug session_id={} status={} pid={}",
        session.id,
        status_label(&session.status),
        pid
    );

    if threads["ok"].as_bool().unwrap_or(false) {
        let count = threads["response"]["body"]["threads"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0);
        println!("threads_ok=true thread_count={count}");
    } else {
        let message = threads["message"].as_str().unwrap_or("unknown");
        println!("threads_ok=false message={message}");
    }

    if events["ok"].as_bool().unwrap_or(false) {
        let count = events["count"].as_u64().unwrap_or(0);
        println!("events_ok=true count={count}");
    } else {
        let message = events["message"].as_str().unwrap_or("unknown");
        println!("events_ok=false message={message}");
    }
}

fn status_label(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Stopped => "stopped",
        SessionStatus::Suspended => "suspended",
        SessionStatus::Unknown => "unknown",
    }
}
