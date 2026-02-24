use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::model::SessionRecord;
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::DoctorDebugArgs;
use crate::dap::{DapRegistry, proxy_for_session, send_request_with_retry};
use crate::error::AppError;
use crate::output;

use super::doctor_debug_adapter::collect_adapter_probe;
use super::doctor_debug_diagnostics::collect_diagnostics;
use super::doctor_debug_render::print_text_summary;

const MAX_DOCTOR_EVENTS: usize = 1000;

pub(super) fn handle_doctor_debug(
    store: &StateStore,
    args: &DoctorDebugArgs,
) -> Result<(), AppError> {
    let doc = collect_doctor_debug_report(store, args)?;
    if output::is_json_mode() {
        output::print_json_doc(&doc);
        return Ok(());
    }

    let session = serde_json::from_value::<SessionRecord>(
        doc.get("session").cloned().unwrap_or_else(|| json!({})),
    )?;
    let adapter = doc
        .get("debug")
        .and_then(|value| value.get("adapter"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let threads = doc
        .get("debug")
        .and_then(|value| value.get("threads"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let events = doc
        .get("debug")
        .and_then(|value| value.get("events"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let diagnostics = doc
        .get("diagnostics")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    print_text_summary(&session, &adapter, &threads, &events, &diagnostics);
    Ok(())
}

pub(super) fn collect_doctor_debug_report(
    store: &StateStore,
    args: &DoctorDebugArgs,
) -> Result<serde_json::Value, AppError> {
    let session = super::api_get_session(store, &args.id)?;
    let inspect = super::api_inspect_session(store, &args.id, args.tail)?;
    let adapter = collect_adapter_probe(&session);

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

    let diagnostics = collect_diagnostics(&session, &inspect, &adapter, &threads, &events);

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "session": session,
        "inspect": inspect,
        "debug": {
            "adapter": adapter,
            "threads": threads,
            "events": events
        },
        "diagnostics": diagnostics
    });
    Ok(doc)
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
