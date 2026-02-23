use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

fn read_dap_message(reader: &mut BufReader<TcpStream>) -> Value {
    let mut content_len: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).expect("read line");
        assert!(bytes > 0, "unexpected eof while reading headers");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            content_len = Some(
                rest.trim()
                    .parse::<usize>()
                    .expect("content-length should parse"),
            );
        }
    }

    let len = content_len.expect("content-length header required");
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).expect("read body");
    serde_json::from_slice(&buf).expect("dap body json")
}

fn write_dap_message(stream: &mut TcpStream, msg: &Value) {
    let payload = serde_json::to_vec(msg).expect("serialize json");
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    stream.write_all(header.as_bytes()).expect("write header");
    stream.write_all(&payload).expect("write payload");
    stream.flush().expect("flush");
}

fn write_state_with_debug_session_status(
    root: &std::path::Path,
    host: &str,
    port: u16,
    status: &str,
) {
    write_state_with_runtime_debug_session_status(root, "python", host, port, status);
}

fn write_state_with_runtime_debug_session_status(
    root: &std::path::Path,
    runtime: &str,
    host: &str,
    port: u16,
    status: &str,
) {
    let (name, entry, reconnect_policy, adapter_kind, capabilities) = match runtime {
        "node" => (
            "node-debug",
            "app.js",
            "manual-reconnect",
            "node-inspector",
            json!(["vscode_attach", "inspector_attach", "dap_bridge"]),
        ),
        _ => (
            "py-debug",
            "app.py",
            "auto-retry",
            "python-debugpy",
            json!([
                "vscode_attach",
                "dap",
                "dap_bootstrap",
                "dap_subprocess_adopt"
            ]),
        ),
    };

    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");

    let state = json!({
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": name,
                    "runtime": runtime,
                    "entry": entry,
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "managed": false,
                    "mode": "debug",
                    "debug": {
                        "host": host,
                        "port": port,
                        "wait_for_client": true,
                        "subprocess": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": status,
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": {
                    "host": host,
                    "requested_port": port,
                    "active_port": port,
                    "fallback_applied": false,
                    "reconnect_policy": reconnect_policy,
                    "adapter_kind": adapter_kind,
                    "transport": "tcp",
                    "capabilities": capabilities
                },
                "created_at": 1,
                "updated_at": 2,
                "last_exit_code": null,
                "restart_count": 0
            }
        }
    });

    fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");
}

fn write_state_with_debug_session(root: &std::path::Path, host: &str, port: u16) {
    write_state_with_debug_session_status(root, host, port, "running");
}

fn reserve_unbound_local_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port should bind");
    let port = listener
        .local_addr()
        .expect("local addr should exist")
        .port();
    drop(listener);
    port
}

#[test]
fn cli_doctor_debug_collects_threads_and_events_in_json() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "event",
                "event": "output",
                "body": {
                    "category": "telemetry",
                    "output": "doctor-event"
                }
            }),
        );

        let threads = read_dap_message(&mut reader);
        assert_eq!(threads["type"], "request");
        assert_eq!(threads["command"], "threads");
        let threads_seq = threads["seq"].as_u64().expect("seq should be number");

        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": threads_seq,
                "success": true,
                "command": "threads",
                "body": {
                    "threads": [{"id": 1, "name": "MainThread"}]
                }
            }),
        );

        thread::sleep(Duration::from_millis(120));
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("doctor")
        .arg("debug")
        .arg("--id")
        .arg("session-1")
        .arg("--timeout-ms")
        .arg("1000")
        .arg("--max-events")
        .arg("10")
        .output()
        .expect("doctor debug should run");

    assert!(output.status.success(), "doctor debug should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["session_id"], "session-1");
    assert_eq!(doc["debug"]["threads"]["ok"], true);
    assert_eq!(doc["debug"]["threads"]["response"]["command"], "threads");
    assert_eq!(doc["debug"]["events"]["ok"], true);
    assert!(
        doc["debug"]["events"]["count"]
            .as_u64()
            .is_some_and(|count| count >= 1),
        "doctor should collect at least one debug event"
    );
    assert!(
        doc["diagnostics"]
            .as_array()
            .is_some_and(|items| items.is_empty()),
        "healthy session should not emit diagnostics"
    );

    let _ = server.join();
}

#[test]
fn cli_doctor_debug_reports_dap_failure_without_crashing() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let threads = read_dap_message(&mut reader);
        assert_eq!(threads["type"], "request");
        assert_eq!(threads["command"], "threads");
        let threads_seq = threads["seq"].as_u64().expect("seq should be number");

        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": threads_seq,
                "success": false,
                "command": "threads",
                "message": "threads unavailable"
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("doctor")
        .arg("debug")
        .arg("--id")
        .arg("session-1")
        .arg("--timeout-ms")
        .arg("1000")
        .output()
        .expect("doctor debug should run");

    assert!(
        output.status.success(),
        "doctor debug should still succeed for diagnostics"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["debug"]["threads"]["ok"], false);
    assert_eq!(doc["debug"]["threads"]["error"], "dap_error");
    assert!(
        doc["debug"]["threads"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("threads unavailable"))
    );
    let diagnostics = doc["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    let d001 = diagnostics
        .iter()
        .find(|item| item["code"] == "D001")
        .expect("D001 diagnostic should exist");
    assert_eq!(d001["level"], "error");
    assert_eq!(d001["summary"], "Failed to query debug threads");
    assert!(
        d001["suggested_actions"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "D001 should include recovery actions"
    );

    let _ = server.join();
}

#[test]
fn cli_doctor_debug_reports_event_channel_and_status_diagnostics() {
    let port = reserve_unbound_local_port();
    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session_status(tmp.path(), "127.0.0.1", port, "stopped");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("doctor")
        .arg("debug")
        .arg("--id")
        .arg("session-1")
        .arg("--timeout-ms")
        .arg("300")
        .output()
        .expect("doctor debug should run");

    assert!(
        output.status.success(),
        "doctor debug should still return diagnostics document"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["debug"]["threads"]["ok"], false);
    assert_eq!(doc["debug"]["events"]["ok"], false);

    let diagnostics = doc["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(
        diagnostics.iter().any(|item| item["code"] == "D001"),
        "D001 should be emitted when thread request fails"
    );
    assert!(
        diagnostics.iter().any(|item| item["code"] == "D002"),
        "D002 should be emitted when event stream fails"
    );
    assert!(
        diagnostics.iter().any(|item| item["code"] == "D003"),
        "D003 should be emitted when session is not running"
    );
}

#[test]
fn cli_doctor_debug_reports_node_adapter_diagnostic_when_unavailable() {
    let port = reserve_unbound_local_port();
    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_runtime_debug_session_status(tmp.path(), "node", "127.0.0.1", port, "stopped");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env_remove("LCODE_NODE_DAP_ADAPTER_CMD")
        .env("LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY", "1")
        .arg("--json")
        .arg("doctor")
        .arg("debug")
        .arg("--id")
        .arg("session-1")
        .arg("--timeout-ms")
        .arg("300")
        .output()
        .expect("doctor debug should run");

    assert!(
        output.status.success(),
        "doctor debug should return diagnostics"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["debug"]["adapter"]["ok"], false);
    assert_eq!(doc["debug"]["adapter"]["source"], "auto_discovery_disabled");

    let diagnostics = doc["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(
        diagnostics.iter().any(|item| item["code"] == "D005"),
        "D005 should be emitted when node adapter is unavailable"
    );
}

#[test]
fn cli_doctor_runtime_reports_matrix_in_json() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("doctor")
        .arg("runtime")
        .output()
        .expect("doctor runtime should run");

    assert!(output.status.success(), "doctor runtime should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);

    let checks = doc["checks"].as_array().expect("checks should be an array");
    assert_eq!(checks.len(), 3, "runtime doctor should include 3 runtimes");
    assert!(
        checks.iter().any(|item| item["runtime"] == "python"),
        "python runtime check should exist"
    );
    assert!(
        checks.iter().any(|item| item["runtime"] == "node"),
        "node runtime check should exist"
    );
    assert!(
        checks.iter().any(|item| item["runtime"] == "rust"),
        "rust runtime check should exist"
    );
    assert_eq!(doc["summary"]["runtime_count"], 3);
}

#[test]
fn cli_doctor_runtime_supports_runtime_filter() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("doctor")
        .arg("runtime")
        .arg("--runtime")
        .arg("node")
        .output()
        .expect("doctor runtime should run");

    assert!(output.status.success(), "doctor runtime should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    let checks = doc["checks"].as_array().expect("checks should be an array");
    assert_eq!(checks.len(), 1, "runtime filter should keep one runtime");
    assert_eq!(checks[0]["runtime"], "node");
}

#[test]
fn cli_doctor_runtime_reports_node_adapter_probe_when_discovery_is_disabled() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env_remove("LCODE_NODE_DAP_ADAPTER_CMD")
        .env("LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY", "1")
        .arg("--json")
        .arg("doctor")
        .arg("runtime")
        .arg("--runtime")
        .arg("node")
        .output()
        .expect("doctor runtime should run");

    assert!(output.status.success(), "doctor runtime should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    let checks = doc["checks"].as_array().expect("checks should be an array");
    assert_eq!(checks.len(), 1);

    let probes = checks[0]["probes"]
        .as_array()
        .expect("probes should be array");
    let adapter_probe = probes
        .iter()
        .find(|probe| probe["name"] == "dap_adapter")
        .expect("node adapter probe should exist");
    assert_eq!(adapter_probe["ok"], false);
    assert_eq!(adapter_probe["source"], "auto_discovery_disabled");
}
