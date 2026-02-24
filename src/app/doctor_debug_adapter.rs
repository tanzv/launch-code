use std::process::Command as ProcessCommand;

use launch_code::model::{RuntimeKind, SessionRecord};
use serde_json::json;

use crate::dap::{NodeAdapterResolution, inspect_node_adapter_resolution};

const NODE_DAP_ADAPTER_CMD_ENV: &str = "LCODE_NODE_DAP_ADAPTER_CMD";
const NODE_DAP_DISABLE_AUTO_DISCOVERY_ENV: &str = "LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY";

pub(super) fn build_node_adapter_actions(
    session_id: &str,
    adapter: &serde_json::Value,
) -> Vec<String> {
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

pub(super) fn collect_adapter_probe(session: &SessionRecord) -> serde_json::Value {
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
        RuntimeKind::Go => probe_go_dlv_adapter(session),
    }
}

fn probe_go_dlv_adapter(session: &SessionRecord) -> serde_json::Value {
    let mut cmd = ProcessCommand::new("dlv");
    cmd.arg("version").current_dir(&session.spec.cwd);
    for key in &session.spec.env_remove {
        cmd.env_remove(key);
    }
    match cmd.envs(session.spec.env.iter()).output() {
        Ok(output) if output.status.success() => json!({
            "ok": true,
            "runtime": "go",
            "backend": "go-delve",
            "source": "builtin",
            "command": "tcp://dlv-dap"
        }),
        Ok(_) => json!({
            "ok": false,
            "runtime": "go",
            "backend": "go-delve",
            "source": "missing_runtime",
            "message": "dlv command failed; install delve and ensure `dlv` is available in PATH"
        }),
        Err(err) => json!({
            "ok": false,
            "runtime": "go",
            "backend": "go-delve",
            "source": "missing_runtime",
            "message": format!("unable to execute dlv: {err}")
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
