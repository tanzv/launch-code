use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    DapBreakpointsArgs, DapContinueArgs, DapDisconnectArgs, DapEvaluateArgs, DapEvaluateContextArg,
    DapExceptionBreakpointsArgs, DapPauseArgs, DapSetVariableArgs, DapStepArgs, DapTerminateArgs,
};
use crate::dap::send_request_with_retry;
use crate::error::AppError;

pub(super) fn handle_dap_breakpoints(
    store: &StateStore,
    args: &DapBreakpointsArgs,
) -> Result<(), AppError> {
    if args.path.trim().is_empty() {
        return Err(AppError::Dap("path cannot be empty".to_string()));
    }

    if args.lines.is_empty() {
        return Err(AppError::Dap("at least one --line is required".to_string()));
    }

    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let breakpoints: Vec<serde_json::Value> = args
        .lines
        .iter()
        .map(|line| {
            let mut item = serde_json::Map::new();
            item.insert("line".to_string(), json!(line));
            if let Some(condition) = &args.condition {
                item.insert("condition".to_string(), json!(condition));
            }
            if let Some(hit_condition) = &args.hit_condition {
                item.insert("hitCondition".to_string(), json!(hit_condition));
            }
            if let Some(log_message) = &args.log_message {
                item.insert("logMessage".to_string(), json!(log_message));
            }
            serde_json::Value::Object(item)
        })
        .collect();
    let arguments = json!({
        "source": { "path": args.path },
        "breakpoints": breakpoints,
    });

    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "setBreakpoints",
        arguments,
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_exception_breakpoints(
    store: &StateStore,
    args: &DapExceptionBreakpointsArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "setExceptionBreakpoints",
        json!({ "filters": args.filters }),
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_evaluate(
    store: &StateStore,
    args: &DapEvaluateArgs,
) -> Result<(), AppError> {
    if args.expression.trim().is_empty() {
        return Err(AppError::Dap("expression cannot be empty".to_string()));
    }

    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);

    let mut arguments = serde_json::Map::new();
    arguments.insert("expression".to_string(), json!(args.expression));
    if let Some(frame_id) = args.frame_id {
        arguments.insert("frameId".to_string(), json!(frame_id));
    }
    if let Some(context) = &args.context {
        let value = match context {
            DapEvaluateContextArg::Watch => "watch",
            DapEvaluateContextArg::Repl => "repl",
            DapEvaluateContextArg::Hover => "hover",
        };
        arguments.insert("context".to_string(), json!(value));
    }

    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "evaluate",
        serde_json::Value::Object(arguments),
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_set_variable(
    store: &StateStore,
    args: &DapSetVariableArgs,
) -> Result<(), AppError> {
    if args.name.trim().is_empty() {
        return Err(AppError::Dap("name cannot be empty".to_string()));
    }

    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "setVariable",
        json!({
            "variablesReference": args.variables_reference,
            "name": args.name,
            "value": args.value
        }),
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_continue(
    store: &StateStore,
    args: &DapContinueArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let (thread_id, response) = super::shared::send_thread_command(
        store,
        &serve_state,
        &args.id,
        args.thread_id,
        "continue",
        timeout,
    )?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "thread_id": thread_id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_pause(store: &StateStore, args: &DapPauseArgs) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let (thread_id, response) = super::shared::send_thread_command(
        store,
        &serve_state,
        &args.id,
        args.thread_id,
        "pause",
        timeout,
    )?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "thread_id": thread_id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_next(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "next")
}

pub(super) fn handle_dap_step_in(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "stepIn")
}

pub(super) fn handle_dap_step_out(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "stepOut")
}

fn handle_dap_step_command(
    store: &StateStore,
    session_id: &str,
    thread_id: Option<u64>,
    timeout_ms: u64,
    command: &str,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(timeout_ms);
    let (resolved_thread_id, response) = super::shared::send_thread_command(
        store,
        &serve_state,
        session_id,
        thread_id,
        command,
        timeout,
    )?;
    let doc = json!({
        "ok": true,
        "session_id": session_id,
        "thread_id": resolved_thread_id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_disconnect(
    store: &StateStore,
    args: &DapDisconnectArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "disconnect",
        json!({
            "terminateDebuggee": args.terminate_debuggee,
            "suspendDebuggee": args.suspend_debuggee
        }),
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_terminate(
    store: &StateStore,
    args: &DapTerminateArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "terminate",
        json!({ "restart": args.restart }),
        timeout,
    )?;
    super::shared::ensure_dap_response_success(&response)?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}
