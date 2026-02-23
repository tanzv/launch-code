use std::process::Command as ProcessCommand;

use launch_code::model::RuntimeKind;
use serde_json::json;

use crate::cli::{DoctorRuntimeArgs, RuntimeArg};
use crate::dap::{NodeAdapterResolution, inspect_node_adapter_resolution};
use crate::error::AppError;
use crate::output;

const NODE_DAP_ADAPTER_CMD_ENV: &str = "LCODE_NODE_DAP_ADAPTER_CMD";
const NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV: &str = "LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY";

pub(super) fn handle_doctor_runtime(args: &DoctorRuntimeArgs) -> Result<(), AppError> {
    let runtimes = selected_runtime_kinds(args.runtime.as_ref());
    let mut checks = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        checks.push(collect_runtime_probe(runtime));
    }

    let strict_not_ready = strict_not_ready_runtimes(&checks);
    let summary = build_runtime_summary(&checks, &strict_not_ready);
    let doc = json!({
        "ok": true,
        "strict": args.strict,
        "checks": checks,
        "summary": summary
    });

    if output::is_json_mode() {
        output::print_json_doc(&doc);
    } else {
        print_runtime_summary_text(&doc);
    }

    if args.strict && !strict_not_ready.is_empty() {
        return Err(AppError::RuntimeReadinessFailed(format!(
            "strict runtime readiness failed for: {}",
            strict_not_ready.join(",")
        )));
    }

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

fn build_runtime_summary(
    checks: &[serde_json::Value],
    strict_not_ready: &[String],
) -> serde_json::Value {
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
        "not_fully_ready": not_ready,
        "strict_not_ready": strict_not_ready,
        "strict_ready_count": runtime_count.saturating_sub(strict_not_ready.len() as u64)
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
    let strict_ready_count = summary
        .get("strict_ready_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let strict_not_ready = summary
        .get("strict_not_ready")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<&str>>()
                .join(",")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "summary runtime_count={runtime_count} run_ready_count={run_ready_count} debug_ready_count={debug_ready_count} dap_ready_count={dap_ready_count} strict_ready_count={strict_ready_count} strict_not_ready={strict_not_ready}"
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

fn strict_not_ready_runtimes(checks: &[serde_json::Value]) -> Vec<String> {
    checks
        .iter()
        .filter(|item| !is_strict_ready(item))
        .filter_map(|item| {
            item.get("runtime")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .collect()
}

fn is_strict_ready(check: &serde_json::Value) -> bool {
    let runtime = check
        .get("runtime")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let run_ready = probe_ok_field(check, "run_ready");
    let debug_ready = probe_ok_field(check, "debug_ready");
    let dap_ready = probe_ok_field(check, "dap_ready");

    match runtime {
        "python" | "node" => run_ready && debug_ready && dap_ready,
        "rust" => run_ready,
        _ => false,
    }
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

fn render_command(program: &str, args: &[String]) -> String {
    let mut command = String::from(program);
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    command
}
