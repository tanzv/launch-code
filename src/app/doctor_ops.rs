use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{DoctorAllArgs, DoctorArgs, DoctorCommands, DoctorDebugArgs};
use crate::error::AppError;
use crate::output;

pub(super) fn handle_doctor(store: &StateStore, args: &DoctorArgs) -> Result<(), AppError> {
    match &args.command {
        DoctorCommands::Debug(req) => super::doctor_debug_ops::handle_doctor_debug(store, req),
        DoctorCommands::Runtime(req) => super::doctor_runtime_ops::handle_doctor_runtime(req),
        DoctorCommands::All(req) => handle_doctor_all(store, req),
    }
}

fn handle_doctor_all(store: &StateStore, args: &DoctorAllArgs) -> Result<(), AppError> {
    let runtime_report = super::doctor_runtime_ops::collect_runtime_report(args.runtime.as_ref());
    let strict_not_ready = runtime_report.strict_not_ready.clone();
    let runtime_doc = json!({
        "checks": runtime_report.checks,
        "summary": runtime_report.summary
    });

    let debug_doc = if let Some(session_id) = args.id.as_deref() {
        let debug_args = DoctorDebugArgs {
            id: session_id.to_string(),
            tail: args.tail,
            max_events: args.max_events,
            timeout_ms: args.timeout_ms,
        };
        Some(super::doctor_debug_ops::collect_doctor_debug_report(
            store,
            &debug_args,
        )?)
    } else {
        None
    };

    let doc = json!({
        "ok": true,
        "strict": args.strict,
        "runtime": runtime_doc,
        "debug": debug_doc
    });

    if output::is_json_mode() {
        output::print_json_doc(&doc);
    } else {
        output::print_message(&format!(
            "doctor_all strict={} runtime_checks={} debug_included={}",
            args.strict,
            doc["runtime"]["checks"]
                .as_array()
                .map(|items| items.len())
                .unwrap_or(0),
            args.id.is_some()
        ));
        super::doctor_runtime_ops::print_runtime_summary_text(&doc["runtime"]);
        if let Some(debug) = doc.get("debug").filter(|value| !value.is_null()) {
            let diagnostics = debug
                .get("diagnostics")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0);
            let session_id = debug
                .get("session_id")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            output::print_message(&format!(
                "doctor_all_debug session_id={session_id} diagnostics={diagnostics}"
            ));
        }
    }

    if args.strict && !strict_not_ready.is_empty() {
        return Err(AppError::RuntimeReadinessFailed(format!(
            "strict runtime readiness failed for: {}",
            strict_not_ready.join(",")
        )));
    }

    Ok(())
}
