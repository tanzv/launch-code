use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{DapBatchArgs, DapRequestArgs};
use crate::dap::send_batch_with_retry;
use crate::dap::send_request_with_retry;
use crate::error::AppError;

pub(super) fn handle_dap_request(
    store: &StateStore,
    args: &DapRequestArgs,
) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let arguments = super::shared::parse_dap_arguments(args.arguments.as_deref())?;

    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        &args.command,
        arguments,
        timeout,
    )?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    super::shared::print_json_doc(&doc)
}

pub(super) fn handle_dap_batch(store: &StateStore, args: &DapBatchArgs) -> Result<(), AppError> {
    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);

    let payload = std::fs::read_to_string(&args.file)?;
    let items: Vec<serde_json::Value> = serde_json::from_str(&payload)?;
    if items.is_empty() {
        return Err(AppError::Dap(
            "batch file must include at least one request".to_string(),
        ));
    }

    let mut requests = Vec::with_capacity(items.len());
    for item in items {
        let command = item
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| AppError::Dap("batch item missing command".to_string()))?;
        let mut arguments = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if arguments.is_null() {
            arguments = json!({});
        }
        requests.push((command.to_string(), arguments));
    }

    let responses = send_batch_with_retry(store, &serve_state, &args.id, requests, timeout)?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "responses": responses,
    });
    super::shared::print_json_doc(&doc)
}
