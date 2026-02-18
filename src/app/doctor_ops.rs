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

    let diagnostics = collect_diagnostics(&session, &inspect, &threads, &events);

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "session": session,
        "inspect": inspect,
        "debug": {
            "threads": threads,
            "events": events
        },
        "diagnostics": diagnostics
    });

    if output::is_json_mode() {
        output::print_json_doc(&doc);
        return Ok(());
    }

    print_text_summary(&session, &threads, &events, &diagnostics);
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
    diagnostics: &[serde_json::Value],
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

    if diagnostics.is_empty() {
        println!("diagnostics=none");
        return;
    }

    println!("diagnostics_count={}", diagnostics.len());
    for item in diagnostics {
        let code = item["code"].as_str().unwrap_or("unknown");
        let level = item["level"].as_str().unwrap_or("unknown");
        let summary = item["summary"].as_str().unwrap_or("unknown");
        println!("diagnostic code={code} level={level} summary={summary}");
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

fn collect_diagnostics(
    session: &SessionRecord,
    inspect: &serde_json::Value,
    threads: &serde_json::Value,
    events: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut diagnostics = Vec::new();
    let threads_ok = threads["ok"].as_bool().unwrap_or(false);
    let events_ok = events["ok"].as_bool().unwrap_or(false);

    if !matches!(session.status, SessionStatus::Running) && (!threads_ok || !events_ok) {
        diagnostics.push(diagnostic_doc(
            "D003",
            "warning",
            "Session is not running",
            format!(
                "Session status is {}. Debug adapter checks may fail until the session is running.",
                status_label(&session.status)
            ),
            vec![
                format!(
                    "Start or restart the session with `lcode restart --id {}`.",
                    session.id
                ),
                "Use `lcode status --id <session_id>` to confirm the process is running."
                    .to_string(),
            ],
        ));
    }

    if !threads_ok {
        let message = threads["message"]
            .as_str()
            .unwrap_or("threads request failed")
            .to_string();
        diagnostics.push(diagnostic_doc(
            "D001",
            "error",
            "Failed to query debug threads",
            message.clone(),
            build_thread_actions(&session.id, &message),
        ));
    }

    if !events_ok {
        let message = events["message"]
            .as_str()
            .unwrap_or("event stream unavailable")
            .to_string();
        diagnostics.push(diagnostic_doc(
            "D002",
            "warning",
            "Failed to read debug events",
            message,
            vec![
                format!(
                    "Run `lcode dap events --id {} --max 20 --timeout-ms 1500` to verify event channel health.",
                    session.id
                ),
                format!(
                    "If the issue persists, restart the session with `lcode restart --id {}`.",
                    session.id
                ),
            ],
        ));
    }

    if let Some(line) = detect_debug_warning_line(inspect) {
        diagnostics.push(diagnostic_doc(
            "D004",
            "info",
            "Debugger warning found in log tail",
            line,
            vec![
                format!(
                    "Inspect extended logs with `lcode logs --id {} --tail 200`.",
                    session.id
                ),
                "Address warning lines before retrying debugger commands.".to_string(),
            ],
        ));
    }

    diagnostics
}

fn build_thread_actions(session_id: &str, message: &str) -> Vec<String> {
    let lower = message.to_ascii_lowercase();
    let mut actions = vec![
        format!(
            "Run `lcode dap threads --id {session_id} --timeout-ms 1500` to confirm adapter availability."
        ),
        format!("Restart the session with `lcode restart --id {session_id}` if this repeats."),
    ];

    if lower.contains("timeout") {
        actions.push("Increase `--timeout-ms` and retry when the target is busy.".to_string());
    }

    if lower.contains("connection refused")
        || lower.contains("channel disconnected")
        || lower.contains("not connected")
    {
        actions.push(format!(
            "Verify debug endpoint metadata with `lcode attach --id {session_id}`."
        ));
    }

    actions
}

fn detect_debug_warning_line(inspect: &serde_json::Value) -> Option<String> {
    let log_text = inspect
        .get("log")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())?;

    for raw_line in log_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let contains_warning = lower.contains("warning")
            || lower.contains("warn")
            || lower.contains("traceback")
            || lower.contains("exception");
        if lower.contains("debugpy") && contains_warning {
            return Some(line.to_string());
        }
    }

    None
}

fn diagnostic_doc(
    code: &str,
    level: &str,
    summary: &str,
    detail: String,
    suggested_actions: Vec<String>,
) -> serde_json::Value {
    json!({
        "code": code,
        "level": level,
        "summary": summary,
        "detail": detail,
        "suggested_actions": suggested_actions,
    })
}
