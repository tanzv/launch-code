use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    DapEventsArgs, DapScopesArgs, DapStackTraceArgs, DapThreadsArgs, DapVariablesArgs,
    DapVariablesFilterArg,
};
use crate::dap::{proxy_for_session, send_request_with_retry};
use crate::error::AppError;

const MAX_DAP_EVENTS: usize = 1000;

pub(super) fn handle_dap_events(store: &StateStore, args: &DapEventsArgs) -> Result<(), AppError> {
    if args.max == 0 || args.max > MAX_DAP_EVENTS {
        return Err(AppError::Dap(format!(
            "max must be between 1 and {MAX_DAP_EVENTS}"
        )));
    }

    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let proxy = proxy_for_session(store, &serve_state, &args.id)?;
    let events = proxy.pop_events(args.max, timeout);

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "events": events,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_threads(
    store: &StateStore,
    args: &DapThreadsArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response =
        send_request_with_retry(store, &serve_state, &args.id, "threads", json!({}), timeout)?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_stack_trace(
    store: &StateStore,
    args: &DapStackTraceArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);

    let thread_id = match args.thread_id {
        Some(value) => value,
        None => {
            let threads_response = send_request_with_retry(
                store,
                &serve_state,
                &args.id,
                "threads",
                json!({}),
                timeout,
            )?;
            super::shared::extract_first_thread_id(&threads_response)?
        }
    };

    let mut arguments = serde_json::Map::new();
    arguments.insert("threadId".to_string(), json!(thread_id));
    if let Some(value) = args.start_frame {
        arguments.insert("startFrame".to_string(), json!(value));
    }
    if let Some(value) = args.levels {
        arguments.insert("levels".to_string(), json!(value));
    }

    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "stackTrace",
        serde_json::Value::Object(arguments),
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

pub(super) fn handle_dap_scopes(store: &StateStore, args: &DapScopesArgs) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "scopes",
        json!({ "frameId": args.frame_id }),
        timeout,
    )?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "frame_id": args.frame_id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_variables(
    store: &StateStore,
    args: &DapVariablesArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);

    let mut arguments = serde_json::Map::new();
    arguments.insert(
        "variablesReference".to_string(),
        json!(args.variables_reference),
    );
    if let Some(value) = &args.filter {
        let filter = match value {
            DapVariablesFilterArg::Named => "named",
            DapVariablesFilterArg::Indexed => "indexed",
        };
        arguments.insert("filter".to_string(), json!(filter));
    }
    if let Some(value) = args.start {
        arguments.insert("start".to_string(), json!(value));
    }
    if let Some(value) = args.count {
        arguments.insert("count".to_string(), json!(value));
    }

    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "variables",
        serde_json::Value::Object(arguments),
        timeout,
    )?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "variables_reference": args.variables_reference,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}
