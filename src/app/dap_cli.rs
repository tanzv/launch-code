use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    DapAdoptSubprocessArgs, DapArgs, DapBatchArgs, DapBreakpointsArgs, DapCommands,
    DapContinueArgs, DapDisconnectArgs, DapEvaluateArgs, DapEvaluateContextArg, DapEventsArgs,
    DapExceptionBreakpointsArgs, DapPauseArgs, DapRequestArgs, DapScopesArgs, DapSetVariableArgs,
    DapStackTraceArgs, DapStepArgs, DapTerminateArgs, DapThreadsArgs, DapVariablesArgs,
    DapVariablesFilterArg,
};
use crate::dap::{
    DapRegistry, adopt_debugpy_subprocess, proxy_for_session, send_batch_with_retry,
    send_request_with_retry,
};
use crate::error::AppError;

pub(super) fn handle_dap(store: &StateStore, args: &DapArgs) -> Result<(), AppError> {
    match &args.command {
        DapCommands::Request(req) => handle_dap_request(store, req),
        DapCommands::Batch(req) => handle_dap_batch(store, req),
        DapCommands::Breakpoints(req) => handle_dap_breakpoints(store, req),
        DapCommands::ExceptionBreakpoints(req) => handle_dap_exception_breakpoints(store, req),
        DapCommands::Evaluate(req) => handle_dap_evaluate(store, req),
        DapCommands::SetVariable(req) => handle_dap_set_variable(store, req),
        DapCommands::Continue(req) => handle_dap_continue(store, req),
        DapCommands::Pause(req) => handle_dap_pause(store, req),
        DapCommands::Next(req) => handle_dap_next(store, req),
        DapCommands::StepIn(req) => handle_dap_step_in(store, req),
        DapCommands::StepOut(req) => handle_dap_step_out(store, req),
        DapCommands::Disconnect(req) => handle_dap_disconnect(store, req),
        DapCommands::Terminate(req) => handle_dap_terminate(store, req),
        DapCommands::AdoptSubprocess(req) => handle_dap_adopt_subprocess(store, req),
        DapCommands::Events(req) => handle_dap_events(store, req),
        DapCommands::Threads(req) => handle_dap_threads(store, req),
        DapCommands::StackTrace(req) => handle_dap_stack_trace(store, req),
        DapCommands::Scopes(req) => handle_dap_scopes(store, req),
        DapCommands::Variables(req) => handle_dap_variables(store, req),
    }
}

fn handle_dap_request(store: &StateStore, args: &DapRequestArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let arguments = parse_dap_arguments(args.arguments.as_deref())?;

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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_batch(store: &StateStore, args: &DapBatchArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));

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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn parse_dap_arguments(input: Option<&str>) -> Result<serde_json::Value, AppError> {
    match input {
        Some(raw) if !raw.trim().is_empty() => {
            let value: serde_json::Value = serde_json::from_str(raw)?;
            if value.is_null() {
                Ok(json!({}))
            } else {
                Ok(value)
            }
        }
        _ => Ok(json!({})),
    }
}

fn handle_dap_breakpoints(store: &StateStore, args: &DapBreakpointsArgs) -> Result<(), AppError> {
    if args.lines.is_empty() {
        return Err(AppError::Dap("at least one --line is required".to_string()));
    }

    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
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
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_continue(store: &StateStore, args: &DapContinueArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let (thread_id, response) = send_thread_command(
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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_exception_breakpoints(
    store: &StateStore,
    args: &DapExceptionBreakpointsArgs,
) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "setExceptionBreakpoints",
        json!({ "filters": args.filters }),
        timeout,
    )?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_evaluate(store: &StateStore, args: &DapEvaluateArgs) -> Result<(), AppError> {
    if args.expression.trim().is_empty() {
        return Err(AppError::Dap("expression cannot be empty".to_string()));
    }

    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));

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
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_set_variable(store: &StateStore, args: &DapSetVariableArgs) -> Result<(), AppError> {
    if args.name.trim().is_empty() {
        return Err(AppError::Dap("name cannot be empty".to_string()));
    }

    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
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
    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_pause(store: &StateStore, args: &DapPauseArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let (thread_id, response) = send_thread_command(
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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_next(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "next")
}

fn handle_dap_step_in(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "stepIn")
}

fn handle_dap_step_out(store: &StateStore, args: &DapStepArgs) -> Result<(), AppError> {
    handle_dap_step_command(store, &args.id, args.thread_id, args.timeout_ms, "stepOut")
}

fn handle_dap_step_command(
    store: &StateStore,
    session_id: &str,
    thread_id: Option<u64>,
    timeout_ms: u64,
    command: &str,
) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(timeout_ms.min(60_000));
    let (resolved_thread_id, response) =
        send_thread_command(store, &serve_state, session_id, thread_id, command, timeout)?;
    let doc = json!({
        "ok": true,
        "session_id": session_id,
        "thread_id": resolved_thread_id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_disconnect(store: &StateStore, args: &DapDisconnectArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
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

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_terminate(store: &StateStore, args: &DapTerminateArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let response = send_request_with_retry(
        store,
        &serve_state,
        &args.id,
        "terminate",
        json!({ "restart": args.restart }),
        timeout,
    )?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_adopt_subprocess(
    store: &StateStore,
    args: &DapAdoptSubprocessArgs,
) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let adopted = adopt_debugpy_subprocess(
        store,
        &serve_state,
        &args.id,
        Duration::from_millis(args.timeout_ms.min(60_000)),
        args.max_events,
        Duration::from_millis(args.bootstrap_timeout_ms.min(60_000)),
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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn send_thread_command(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    thread_id: Option<u64>,
    command: &str,
    timeout: Duration,
) -> Result<(u64, serde_json::Value), AppError> {
    let thread_id = resolve_thread_id(store, serve_state, session_id, thread_id, timeout)?;
    let response = send_request_with_retry(
        store,
        serve_state,
        session_id,
        command,
        json!({ "threadId": thread_id }),
        timeout,
    )?;
    Ok((thread_id, response))
}

fn resolve_thread_id(
    store: &StateStore,
    serve_state: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    thread_id: Option<u64>,
    timeout: Duration,
) -> Result<u64, AppError> {
    match thread_id {
        Some(value) => Ok(value),
        None => {
            let threads_response = send_request_with_retry(
                store,
                serve_state,
                session_id,
                "threads",
                json!({}),
                timeout,
            )?;
            extract_first_thread_id(&threads_response)
        }
    }
}

fn extract_first_thread_id(threads_response: &serde_json::Value) -> Result<u64, AppError> {
    threads_response
        .get("body")
        .and_then(|body| body.get("threads"))
        .and_then(|threads| threads.as_array())
        .and_then(|threads| threads.first())
        .and_then(|thread| thread.get("id"))
        .and_then(|id| id.as_u64())
        .ok_or_else(|| AppError::Dap("no thread returned by debug adapter".to_string()))
}

fn handle_dap_events(store: &StateStore, args: &DapEventsArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let proxy = proxy_for_session(store, &serve_state, &args.id)?;
    let events = proxy.pop_events(args.max, timeout);

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "events": events,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_threads(store: &StateStore, args: &DapThreadsArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
    let response =
        send_request_with_retry(store, &serve_state, &args.id, "threads", json!({}), timeout)?;

    let doc = json!({
        "ok": true,
        "session_id": args.id,
        "response": response,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_stack_trace(store: &StateStore, args: &DapStackTraceArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));

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
            extract_first_thread_id(&threads_response)?
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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_scopes(store: &StateStore, args: &DapScopesArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));
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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

fn handle_dap_variables(store: &StateStore, args: &DapVariablesArgs) -> Result<(), AppError> {
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let timeout = Duration::from_millis(args.timeout_ms.min(60_000));

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
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}
