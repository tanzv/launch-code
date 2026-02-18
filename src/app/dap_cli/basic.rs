use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{DapBatchArgs, DapRequestArgs};
use crate::dap::send_batch_with_retry;
use crate::dap::send_request_with_retry;
use crate::error::AppError;

const MAX_DAP_BATCH_REQUESTS: usize = 128;

pub(super) fn handle_dap_request(
    store: &StateStore,
    args: &DapRequestArgs,
) -> Result<(), AppError> {
    let command = args.command.trim();
    if command.is_empty() {
        return Err(AppError::Dap("command cannot be empty".to_string()));
    }

    let serve_state = super::shared::fresh_registry();
    let timeout = super::shared::clamp_timeout(args.timeout_ms);
    let arguments = super::shared::parse_dap_arguments(args.arguments.as_deref())?;

    let response =
        send_request_with_retry(store, &serve_state, &args.id, command, arguments, timeout)?;
    super::shared::ensure_dap_response_success(&response)?;
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
    if items.len() > MAX_DAP_BATCH_REQUESTS {
        return Err(AppError::Dap(format!(
            "batch file must include at most {MAX_DAP_BATCH_REQUESTS} requests"
        )));
    }

    let mut requests = Vec::with_capacity(items.len());
    for item in items {
        let Some(item_obj) = item.as_object() else {
            return Err(AppError::Dap("batch item must be an object".to_string()));
        };

        let command = item
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Dap("batch item missing command".to_string()))?;
        let command = command.trim();
        if command.is_empty() {
            return Err(AppError::Dap("batch item missing command".to_string()));
        }
        let arguments = match item_obj.get("arguments") {
            None => json!({}),
            Some(value) if value.is_null() => json!({}),
            Some(value) if value.is_object() => value.clone(),
            Some(_) => {
                return Err(AppError::Dap(
                    "batch item arguments must be a JSON object".to_string(),
                ));
            }
        };
        requests.push((command.to_string(), arguments));
    }

    let responses = send_batch_with_retry(store, &serve_state, &args.id, requests, timeout)?;
    super::shared::ensure_dap_batch_success(&responses)?;
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "responses": responses,
    });
    super::shared::print_json_doc(&doc)
}
