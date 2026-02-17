use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::DapAdoptSubprocessArgs;
use crate::dap::adopt_debugpy_subprocess;
use crate::error::AppError;

const MAX_DAP_ADOPT_EVENTS: usize = 1000;

pub(super) fn handle_dap_adopt_subprocess(
    store: &StateStore,
    args: &DapAdoptSubprocessArgs,
) -> Result<(), AppError> {
    if args.max_events == 0 || args.max_events > MAX_DAP_ADOPT_EVENTS {
        return Err(AppError::Dap(format!(
            "max-events must be between 1 and {MAX_DAP_ADOPT_EVENTS}"
        )));
    }

    let serve_state = super::shared::fresh_registry();
    let adopted = adopt_debugpy_subprocess(
        store,
        &serve_state,
        &args.id,
        super::shared::clamp_timeout(args.timeout_ms),
        args.max_events,
        super::shared::clamp_timeout(args.bootstrap_timeout_ms),
        args.child_session_id.as_deref(),
    )?;

    let doc = json!({
        "ok": true,
        "parent_session_id": args.id,
        "child_session_id": adopted.child_session_id,
        "endpoint": format!("{}:{}", adopted.host, adopted.port),
        "process_id": adopted.process_id,
        "source_event": adopted.source_event,
        "bootstrap": {
            "responses": adopted.bootstrap_responses
        }
    });
    super::shared::print_json_doc(&doc)
}
