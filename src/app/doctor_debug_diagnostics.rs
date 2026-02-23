use launch_code::model::{RuntimeKind, SessionRecord, SessionStatus};
use serde_json::json;

use super::doctor_debug_adapter::build_node_adapter_actions;

pub(super) fn collect_diagnostics(
    session: &SessionRecord,
    inspect: &serde_json::Value,
    adapter: &serde_json::Value,
    threads: &serde_json::Value,
    events: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut diagnostics = Vec::new();
    let threads_ok = threads["ok"].as_bool().unwrap_or(false);
    let events_ok = events["ok"].as_bool().unwrap_or(false);
    let adapter_ok = adapter["ok"].as_bool().unwrap_or(false);

    if matches!(session.spec.runtime, RuntimeKind::Node) && !adapter_ok {
        let detail = adapter["message"]
            .as_str()
            .unwrap_or("node debug adapter is unavailable")
            .to_string();
        diagnostics.push(diagnostic_doc(
            "D005",
            "error",
            "Node debug adapter is unavailable",
            detail,
            build_node_adapter_actions(&session.id, adapter),
        ));
    }

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

pub(super) fn status_label(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Stopped => "stopped",
        SessionStatus::Suspended => "suspended",
        SessionStatus::Unknown => "unknown",
    }
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
