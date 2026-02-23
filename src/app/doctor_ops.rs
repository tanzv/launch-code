use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use launch_code::model::{RuntimeKind, SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{DoctorArgs, DoctorCommands, DoctorDebugArgs, DoctorRuntimeArgs, RuntimeArg};
use crate::dap::{
    DapRegistry, NodeAdapterResolution, inspect_node_adapter_resolution, proxy_for_session,
    send_request_with_retry,
};
use crate::error::AppError;
use crate::output;

const MAX_DOCTOR_EVENTS: usize = 1000;
const NODE_DAP_ADAPTER_CMD_ENV: &str = "LCODE_NODE_DAP_ADAPTER_CMD";
const NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV: &str = "LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY";

pub(super) fn handle_doctor(store: &StateStore, args: &DoctorArgs) -> Result<(), AppError> {
    match &args.command {
        DoctorCommands::Debug(req) => handle_doctor_debug(store, req),
        DoctorCommands::Runtime(req) => handle_doctor_runtime(req),
    }
}

fn handle_doctor_runtime(args: &DoctorRuntimeArgs) -> Result<(), AppError> {
    let runtimes = selected_runtime_kinds(args.runtime.as_ref());
    let mut checks = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        checks.push(collect_runtime_probe(runtime));
    }

    let summary = build_runtime_summary(&checks);
    let doc = json!({
        "ok": true,
        "checks": checks,
        "summary": summary
    });

    if output::is_json_mode() {
        output::print_json_doc(&doc);
        return Ok(());
    }

    print_runtime_summary_text(&doc);
    Ok(())
}

fn selected_runtime_kinds(filter: Option<&RuntimeArg>) -> Vec<RuntimeKind> {
    match filter {
        Some(RuntimeArg::Python) => vec![RuntimeKind::Python],
        Some(RuntimeArg::Node) => vec![RuntimeKind::Node],
        Some(RuntimeArg::Rust) => vec![RuntimeKind::Rust],
        None => vec![RuntimeKind::Python, RuntimeKind::Node, RuntimeKind::Rust],
    }
}

fn collect_runtime_probe(runtime: RuntimeKind) -> serde_json::Value {
    match runtime {
        RuntimeKind::Python => collect_python_runtime_probe(),
        RuntimeKind::Node => collect_node_runtime_probe(),
        RuntimeKind::Rust => collect_rust_runtime_probe(),
    }
}

fn collect_python_runtime_probe() -> serde_json::Value {
    let python = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python".to_string());
    let runtime_probe = probe_command(&python, &["--version"], "runtime_command");
    let debugpy_probe = probe_command(&python, &["-c", "import debugpy"], "debugpy_import");

    let run_ready = probe_ok(&runtime_probe);
    let debug_ready = run_ready && probe_ok(&debugpy_probe);
    let dap_ready = debug_ready;

    let mut actions = Vec::new();
    if !run_ready {
        actions.push(format!(
            "Install Python 3 and ensure `{python}` is available in PATH."
        ));
    }
    if run_ready && !probe_ok(&debugpy_probe) {
        actions.push(format!(
            "Install debugpy with `{python} -m pip install debugpy`."
        ));
    }
    if actions.is_empty() {
        actions.push("Python runtime and debugpy are available.".to_string());
    }

    json!({
        "runtime": "python",
        "run_ready": run_ready,
        "debug_ready": debug_ready,
        "dap_ready": dap_ready,
        "probes": [runtime_probe, debugpy_probe],
        "suggested_actions": actions
    })
}

fn collect_node_runtime_probe() -> serde_json::Value {
    let runtime_probe = probe_command("node", &["--version"], "runtime_command");
    let adapter_probe = node_adapter_runtime_probe();

    let run_ready = probe_ok(&runtime_probe);
    let debug_ready = run_ready;
    let dap_ready = run_ready && probe_ok(&adapter_probe);

    let mut actions = Vec::new();
    if !run_ready {
        actions.push("Install Node.js and ensure `node` is available in PATH.".to_string());
    }
    if run_ready && !probe_ok(&adapter_probe) {
        actions.extend(build_node_runtime_actions(&adapter_probe));
    }
    if actions.is_empty() {
        actions.push("Node runtime and adapter bridge are available.".to_string());
    }

    json!({
        "runtime": "node",
        "run_ready": run_ready,
        "debug_ready": debug_ready,
        "dap_ready": dap_ready,
        "probes": [runtime_probe, adapter_probe],
        "suggested_actions": actions
    })
}

fn collect_rust_runtime_probe() -> serde_json::Value {
    let cargo_probe = probe_command("cargo", &["--version"], "cargo_command");
    let rustc_probe = probe_command("rustc", &["--version"], "rustc_command");

    let run_ready = probe_ok(&cargo_probe);
    let debug_ready = false;
    let dap_ready = false;

    let mut actions = Vec::new();
    if !run_ready {
        actions.push(
            "Install Rust toolchain with `rustup` and ensure `cargo` is available in PATH."
                .to_string(),
        );
    }
    actions.push(
        "Rust debug backend is not implemented yet; run mode is currently supported.".to_string(),
    );

    json!({
        "runtime": "rust",
        "run_ready": run_ready,
        "debug_ready": debug_ready,
        "dap_ready": dap_ready,
        "probes": [cargo_probe, rustc_probe],
        "suggested_actions": actions
    })
}

fn probe_command(program: &str, args: &[&str], name: &str) -> serde_json::Value {
    let command = render_command_str(program, args);
    match ProcessCommand::new(program).args(args).output() {
        Ok(output) => {
            let ok = output.status.success();
            let detail = if ok {
                first_non_empty_output_line(&output.stdout, &output.stderr)
                    .unwrap_or_else(|| "command completed successfully".to_string())
            } else {
                let stderr = first_non_empty_output_line(&output.stderr, &output.stdout)
                    .unwrap_or_else(|| "command exited with failure".to_string());
                match output.status.code() {
                    Some(code) => format!("exit_code={code}; {stderr}"),
                    None => format!("terminated by signal; {stderr}"),
                }
            };

            json!({
                "name": name,
                "ok": ok,
                "command": command,
                "detail": detail
            })
        }
        Err(err) => json!({
            "name": name,
            "ok": false,
            "command": command,
            "detail": err.to_string()
        }),
    }
}

fn node_adapter_runtime_probe() -> serde_json::Value {
    match inspect_node_adapter_resolution() {
        NodeAdapterResolution::Command(command) => json!({
            "name": "dap_adapter",
            "ok": true,
            "source": command.source.label(),
            "command": render_command(&command.program, &command.args),
            "detail": "node adapter command resolved"
        }),
        NodeAdapterResolution::InvalidEnv { message } => json!({
            "name": "dap_adapter",
            "ok": false,
            "source": "invalid_env",
            "detail": format!("invalid {NODE_DAP_ADAPTER_CMD_ENV}: {message}")
        }),
        NodeAdapterResolution::AutoDiscoveryDisabled => json!({
            "name": "dap_adapter",
            "ok": false,
            "source": "auto_discovery_disabled",
            "detail": format!("auto discovery disabled by {NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV}")
        }),
        NodeAdapterResolution::NotFound => json!({
            "name": "dap_adapter",
            "ok": false,
            "source": "not_found",
            "detail": format!(
                "set {NODE_DAP_ADAPTER_CMD_ENV} or install js-debug adapter in PATH/VSCode extensions"
            )
        }),
    }
}

fn build_runtime_summary(checks: &[serde_json::Value]) -> serde_json::Value {
    let runtime_count = checks.len() as u64;
    let run_ready_count = checks
        .iter()
        .filter(|item| probe_ok_field(item, "run_ready"))
        .count() as u64;
    let debug_ready_count = checks
        .iter()
        .filter(|item| probe_ok_field(item, "debug_ready"))
        .count() as u64;
    let dap_ready_count = checks
        .iter()
        .filter(|item| probe_ok_field(item, "dap_ready"))
        .count() as u64;
    let not_ready = checks
        .iter()
        .filter(|item| {
            !(probe_ok_field(item, "run_ready")
                && probe_ok_field(item, "debug_ready")
                && probe_ok_field(item, "dap_ready"))
        })
        .filter_map(|item| {
            item.get("runtime")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .collect::<Vec<String>>();

    json!({
        "runtime_count": runtime_count,
        "run_ready_count": run_ready_count,
        "debug_ready_count": debug_ready_count,
        "dap_ready_count": dap_ready_count,
        "not_fully_ready": not_ready
    })
}

fn print_runtime_summary_text(doc: &serde_json::Value) {
    let checks = doc
        .get("checks")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let summary = doc.get("summary").cloned().unwrap_or_else(|| json!({}));

    println!("doctor_runtime checks={}", checks.len());
    for item in &checks {
        let runtime = item
            .get("runtime")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let run_ready = probe_ok_field(item, "run_ready");
        let debug_ready = probe_ok_field(item, "debug_ready");
        let dap_ready = probe_ok_field(item, "dap_ready");
        println!(
            "runtime={runtime} run_ready={run_ready} debug_ready={debug_ready} dap_ready={dap_ready}"
        );

        if let Some(probes) = item.get("probes").and_then(|value| value.as_array()) {
            for probe in probes {
                let name = probe
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let ok = probe_ok(probe);
                let detail = probe
                    .get("detail")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                println!("probe name={name} ok={ok} detail={detail}");
            }
        }
    }

    let runtime_count = summary
        .get("runtime_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let run_ready_count = summary
        .get("run_ready_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let debug_ready_count = summary
        .get("debug_ready_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let dap_ready_count = summary
        .get("dap_ready_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    println!(
        "summary runtime_count={runtime_count} run_ready_count={run_ready_count} debug_ready_count={debug_ready_count} dap_ready_count={dap_ready_count}"
    );
}

fn probe_ok(probe: &serde_json::Value) -> bool {
    probe
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn probe_ok_field(value: &serde_json::Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(|item| item.as_bool())
        .unwrap_or(false)
}

fn build_node_runtime_actions(adapter_probe: &serde_json::Value) -> Vec<String> {
    let source = adapter_probe
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    let mut actions = vec![format!(
        "Set `{NODE_DAP_ADAPTER_CMD_ENV}` to a JSON array command, for example [\"node\",\"/path/to/js-debug/src/dapDebugServer.js\"]."
    )];
    if source == "auto_discovery_disabled" {
        actions.insert(
            0,
            format!(
                "Unset `{NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV}` or set it to `0` to enable PATH/VSCode discovery."
            ),
        );
    }
    if source == "not_found" {
        actions.insert(
            0,
            "Install `js-debug-adapter` in PATH or install VSCode/Cursor JavaScript debugger extension."
                .to_string(),
        );
    }
    actions
}

fn render_command_str(program: &str, args: &[&str]) -> String {
    let mut command = String::from(program);
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    command
}

fn first_non_empty_output_line(primary: &[u8], secondary: &[u8]) -> Option<String> {
    extract_first_non_empty_line(primary).or_else(|| extract_first_non_empty_line(secondary))
}

fn extract_first_non_empty_line(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trim_to_len(trimmed, 200));
        }
    }
    None
}

fn trim_to_len(value: &str, max_len: usize) -> String {
    let len = value.chars().count();
    if len <= max_len {
        return value.to_string();
    }
    let truncated = value.chars().take(max_len).collect::<String>();
    format!("{truncated}...")
}

fn handle_doctor_debug(store: &StateStore, args: &DoctorDebugArgs) -> Result<(), AppError> {
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

    if output::is_json_mode() {
        output::print_json_doc(&doc);
        return Ok(());
    }

    print_text_summary(&session, &adapter, &threads, &events, &diagnostics);
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
    adapter: &serde_json::Value,
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

    if adapter["ok"].as_bool().unwrap_or(false) {
        let source = adapter["source"].as_str().unwrap_or("unknown");
        let command = adapter["command"].as_str().unwrap_or("-");
        println!("adapter_ok=true source={source} command={command}");
    } else {
        let source = adapter["source"].as_str().unwrap_or("unknown");
        let message = adapter["message"].as_str().unwrap_or("unknown");
        println!("adapter_ok=false source={source} message={message}");
    }

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

fn build_node_adapter_actions(session_id: &str, adapter: &serde_json::Value) -> Vec<String> {
    let source = adapter["source"].as_str().unwrap_or("unknown");
    let mut actions = vec![format!(
        "Set `{NODE_DAP_ADAPTER_CMD_ENV}` to a JSON array command, for example [\"node\",\"/path/to/js-debug/src/dapDebugServer.js\"]."
    )];

    if source == "auto_discovery_disabled" {
        actions.insert(
            0,
            format!(
                "Unset `{NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV}` or set it to `0` to allow PATH/VSCode discovery."
            ),
        );
    }

    if source == "not_found" {
        actions.insert(
            0,
            "Install `js-debug-adapter` in PATH or install the VSCode/Cursor JavaScript debugger extension."
                .to_string(),
        );
    }

    actions.push(format!(
        "Re-run `lcode doctor debug --id {session_id}` after adapter configuration."
    ));
    actions
}

fn collect_adapter_probe(session: &SessionRecord) -> serde_json::Value {
    match session.spec.runtime {
        RuntimeKind::Python => json!({
            "ok": true,
            "runtime": "python",
            "backend": "python-debugpy",
            "source": "builtin",
            "command": "tcp://debugpy"
        }),
        RuntimeKind::Node => match inspect_node_adapter_resolution() {
            NodeAdapterResolution::Command(command) => {
                let rendered = render_command(&command.program, &command.args);
                json!({
                    "ok": true,
                    "runtime": "node",
                    "backend": "node-inspector",
                    "source": command.source.label(),
                    "program": command.program,
                    "args": command.args,
                    "command": rendered
                })
            }
            NodeAdapterResolution::InvalidEnv { message } => json!({
                "ok": false,
                "runtime": "node",
                "backend": "node-inspector",
                "source": "invalid_env",
                "message": format!("invalid {NODE_DAP_ADAPTER_CMD_ENV}: {message}")
            }),
            NodeAdapterResolution::AutoDiscoveryDisabled => json!({
                "ok": false,
                "runtime": "node",
                "backend": "node-inspector",
                "source": "auto_discovery_disabled",
                "message": format!("auto discovery disabled by {NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV}")
            }),
            NodeAdapterResolution::NotFound => json!({
                "ok": false,
                "runtime": "node",
                "backend": "node-inspector",
                "source": "not_found",
                "message": format!("node adapter not found; set {NODE_DAP_ADAPTER_CMD_ENV} or install js-debug adapter in PATH/VSCode extensions")
            }),
        },
        RuntimeKind::Rust => json!({
            "ok": false,
            "runtime": "rust",
            "backend": "unsupported",
            "source": "unsupported",
            "message": "dap operations are unavailable for this runtime/backend"
        }),
    }
}

fn render_command(program: &str, args: &[String]) -> String {
    let mut command = String::from(program);
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    command
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
