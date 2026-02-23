use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
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

fn write_state_with_debug_session(root: &std::path::Path, host: &str, port: u16) {
    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");

    let state = json!({
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "py-debug",
                    "runtime": "python",
                    "entry": "app.py",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "managed": false,
                    "mode": "debug",
                    "debug": {
                        "host": host,
                        "port": port,
                        "wait_for_client": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": "running",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": {
                    "host": host,
                    "requested_port": port,
                    "active_port": port,
                    "fallback_applied": false,
                    "reconnect_policy": "auto-retry"
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

fn write_state_with_node_debug_session(root: &std::path::Path, host: &str, port: u16) {
    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");

    let state = json!({
        "sessions": {
            "session-node": {
                "id": "session-node",
                "spec": {
                    "name": "node-debug",
                    "runtime": "node",
                    "entry": "app.js",
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
                "status": "running",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": {
                    "host": host,
                    "requested_port": port,
                    "active_port": port,
                    "fallback_applied": false,
                    "reconnect_policy": "manual-reconnect",
                    "adapter_kind": "node-inspector",
                    "transport": "tcp",
                    "capabilities": ["vscode_attach", "inspector_attach", "dap_bridge"]
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

fn discover_python_bin() -> Option<&'static str> {
    ["python", "python3"]
        .into_iter()
        .find(|candidate| Command::new(candidate).arg("--version").output().is_ok())
}

#[test]
fn cli_dap_request_sends_command_and_prints_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "initialize");
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        let response = json!({
            "seq": 1,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "initialize",
            "body": {"supportsConfigurationDoneRequest": true}
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("request")
        .arg("--id")
        .arg("session-1")
        .arg("--command")
        .arg(" initialize ")
        .arg("--arguments")
        .arg("{\"clientID\":\"launch-code-test\"}")
        .output()
        .expect("dap request should run");

    assert!(output.status.success(), "dap request should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["type"], "response");
    assert_eq!(doc["response"]["command"], "initialize");

    let _ = server.join();
}

#[test]
fn cli_dap_request_exits_non_zero_when_adapter_returns_failure_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "initialize");
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": req_seq,
                "success": false,
                "command": "initialize",
                "message": "mock adapter failure"
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("request")
        .arg("--id")
        .arg("session-1")
        .arg("--command")
        .arg("initialize")
        .output()
        .expect("dap request should run");

    assert!(
        !output.status.success(),
        "dap request should fail when adapter response success=false"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        stderr.contains("mock adapter failure"),
        "stderr should include adapter failure message"
    );

    let _ = server.join();
}

#[test]
fn cli_dap_batch_exits_non_zero_when_any_response_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let first = read_dap_message(&mut reader);
        assert_eq!(first["type"], "request");
        assert_eq!(first["command"], "initialize");
        let first_seq = first["seq"].as_u64().expect("seq should be number");

        let second = read_dap_message(&mut reader);
        assert_eq!(second["type"], "request");
        assert_eq!(second["command"], "threads");
        let second_seq = second["seq"].as_u64().expect("seq should be number");

        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": first_seq,
                "success": true,
                "command": "initialize",
                "body": {}
            }),
        );
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": second_seq,
                "success": false,
                "command": "threads",
                "message": "threads failed"
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let batch_path = tmp.path().join("batch.json");
    fs::write(
        &batch_path,
        serde_json::to_string_pretty(&json!([
            {"command": "initialize", "arguments": {}},
            {"command": "threads", "arguments": {}}
        ]))
        .expect("serialize batch"),
    )
    .expect("batch file should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("batch")
        .arg("--id")
        .arg("session-1")
        .arg("--file")
        .arg(batch_path)
        .arg("--timeout-ms")
        .arg("3000")
        .output()
        .expect("dap batch should run");

    assert!(
        !output.status.success(),
        "dap batch should fail when any adapter response success=false"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        stderr.contains("threads failed"),
        "stderr should include failing response message"
    );

    let _ = server.join();
}

#[test]
fn cli_dap_evaluate_auto_bootstraps_when_server_is_not_available() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let evaluate_first = read_dap_message(&mut reader);
        assert_eq!(evaluate_first["type"], "request");
        assert_eq!(evaluate_first["command"], "evaluate");
        assert_eq!(evaluate_first["arguments"]["expression"], "counter + 1");
        assert_eq!(evaluate_first["arguments"]["context"], "repl");
        let first_seq = evaluate_first["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": first_seq,
                "success": false,
                "command": "evaluate",
                "message": "Server is not available"
            }),
        );

        let initialize = read_dap_message(&mut reader);
        assert_eq!(initialize["type"], "request");
        assert_eq!(initialize["command"], "initialize");
        let initialize_seq = initialize["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": initialize_seq,
                "success": true,
                "command": "initialize",
                "body": {}
            }),
        );

        let attach = read_dap_message(&mut reader);
        assert_eq!(attach["type"], "request");
        assert_eq!(attach["command"], "attach");
        assert_eq!(attach["arguments"]["connect"]["host"], "127.0.0.1");
        assert_eq!(attach["arguments"]["connect"]["port"], port);
        assert_eq!(attach["arguments"]["justMyCode"], false);
        let attach_seq = attach["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 3,
                "type": "response",
                "request_seq": attach_seq,
                "success": true,
                "command": "attach",
                "body": {}
            }),
        );

        let configured = read_dap_message(&mut reader);
        assert_eq!(configured["type"], "request");
        assert_eq!(configured["command"], "configurationDone");
        let configured_seq = configured["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 4,
                "type": "response",
                "request_seq": configured_seq,
                "success": true,
                "command": "configurationDone",
                "body": {}
            }),
        );

        let evaluate_second = read_dap_message(&mut reader);
        assert_eq!(evaluate_second["type"], "request");
        assert_eq!(evaluate_second["command"], "evaluate");
        let second_seq = evaluate_second["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 5,
                "type": "response",
                "request_seq": second_seq,
                "success": true,
                "command": "evaluate",
                "body": {
                    "result": "2",
                    "type": "int"
                }
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("evaluate")
        .arg("--id")
        .arg("session-1")
        .arg("--expression")
        .arg("counter + 1")
        .arg("--context")
        .arg("repl")
        .arg("--timeout-ms")
        .arg("3000")
        .output()
        .expect("dap evaluate should run");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        output.status.success(),
        "dap evaluate should succeed, stdout: {stdout}, stderr: {stderr}"
    );
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "evaluate");
    assert_eq!(doc["response"]["success"], true);
    assert_eq!(doc["response"]["body"]["result"], "2");

    let _ = server.join();
}

#[test]
fn cli_dap_evaluate_auto_bootstraps_after_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let evaluate_first = read_dap_message(&mut reader);
        assert_eq!(evaluate_first["type"], "request");
        assert_eq!(evaluate_first["command"], "evaluate");
        assert_eq!(evaluate_first["arguments"]["expression"], "counter + 1");
        assert_eq!(evaluate_first["arguments"]["context"], "repl");

        thread::sleep(Duration::from_millis(350));

        let initialize = read_dap_message(&mut reader);
        assert_eq!(initialize["type"], "request");
        assert_eq!(initialize["command"], "initialize");
        let initialize_seq = initialize["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": initialize_seq,
                "success": true,
                "command": "initialize",
                "body": {}
            }),
        );

        let attach = read_dap_message(&mut reader);
        assert_eq!(attach["type"], "request");
        assert_eq!(attach["command"], "attach");
        assert_eq!(attach["arguments"]["connect"]["host"], "127.0.0.1");
        assert_eq!(attach["arguments"]["connect"]["port"], port);
        assert_eq!(attach["arguments"]["justMyCode"], false);
        let attach_seq = attach["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 3,
                "type": "response",
                "request_seq": attach_seq,
                "success": true,
                "command": "attach",
                "body": {}
            }),
        );

        let configured = read_dap_message(&mut reader);
        assert_eq!(configured["type"], "request");
        assert_eq!(configured["command"], "configurationDone");
        let configured_seq = configured["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 4,
                "type": "response",
                "request_seq": configured_seq,
                "success": true,
                "command": "configurationDone",
                "body": {}
            }),
        );

        let evaluate_second = read_dap_message(&mut reader);
        assert_eq!(evaluate_second["type"], "request");
        assert_eq!(evaluate_second["command"], "evaluate");
        let second_seq = evaluate_second["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 5,
                "type": "response",
                "request_seq": second_seq,
                "success": true,
                "command": "evaluate",
                "body": {
                    "result": "2",
                    "type": "int"
                }
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("evaluate")
        .arg("--id")
        .arg("session-1")
        .arg("--expression")
        .arg("counter + 1")
        .arg("--context")
        .arg("repl")
        .arg("--timeout-ms")
        .arg("200")
        .output()
        .expect("dap evaluate should run");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        output.status.success(),
        "dap evaluate should succeed, stdout: {stdout}, stderr: {stderr}"
    );
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "evaluate");
    assert_eq!(doc["response"]["success"], true);
    assert_eq!(doc["response"]["body"]["result"], "2");

    let _ = server.join();
}

#[test]
fn cli_dap_bootstrap_tolerates_attach_already_debugged_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let evaluate_first = read_dap_message(&mut reader);
        assert_eq!(evaluate_first["type"], "request");
        assert_eq!(evaluate_first["command"], "evaluate");
        let first_seq = evaluate_first["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": first_seq,
                "success": false,
                "command": "evaluate",
                "message": "Server is not available"
            }),
        );

        let initialize = read_dap_message(&mut reader);
        assert_eq!(initialize["command"], "initialize");
        let initialize_seq = initialize["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": initialize_seq,
                "success": true,
                "command": "initialize",
                "body": {}
            }),
        );

        let attach = read_dap_message(&mut reader);
        assert_eq!(attach["command"], "attach");
        assert_eq!(attach["arguments"]["connect"]["host"], "127.0.0.1");
        assert_eq!(attach["arguments"]["connect"]["port"], port);
        let attach_seq = attach["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 3,
                "type": "response",
                "request_seq": attach_seq,
                "success": false,
                "command": "attach",
                "message": "Server[pid=1234] is already being debugged."
            }),
        );

        let configured = read_dap_message(&mut reader);
        assert_eq!(configured["command"], "configurationDone");
        let configured_seq = configured["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 4,
                "type": "response",
                "request_seq": configured_seq,
                "success": true,
                "command": "configurationDone",
                "body": {}
            }),
        );

        let evaluate_second = read_dap_message(&mut reader);
        assert_eq!(evaluate_second["command"], "evaluate");
        let second_seq = evaluate_second["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 5,
                "type": "response",
                "request_seq": second_seq,
                "success": true,
                "command": "evaluate",
                "body": {
                    "result": "3",
                    "type": "int"
                }
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("evaluate")
        .arg("--id")
        .arg("session-1")
        .arg("--expression")
        .arg("1+2")
        .arg("--context")
        .arg("repl")
        .arg("--timeout-ms")
        .arg("3000")
        .output()
        .expect("dap evaluate should run");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        output.status.success(),
        "dap evaluate should succeed, stdout: {stdout}, stderr: {stderr}"
    );
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "evaluate");
    assert_eq!(doc["response"]["success"], true);
    assert_eq!(doc["response"]["body"]["result"], "3");

    let _ = server.join();
}

#[test]
fn cli_dap_batch_can_complete_attach_sequence_without_deadlock() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let attach = read_dap_message(&mut reader);
        assert_eq!(attach["type"], "request");
        assert_eq!(attach["command"], "attach");
        let attach_seq = attach["seq"].as_u64().expect("seq should be number");

        let config_done = read_dap_message(&mut reader);
        assert_eq!(config_done["type"], "request");
        assert_eq!(config_done["command"], "configurationDone");
        let config_seq = config_done["seq"].as_u64().expect("seq should be number");

        // Respond out-of-order to ensure client can correlate by request_seq.
        let config_resp = json!({
            "seq": 1,
            "type": "response",
            "request_seq": config_seq,
            "success": true,
            "command": "configurationDone",
            "body": {}
        });
        write_dap_message(&mut stream, &config_resp);

        thread::sleep(Duration::from_millis(50));

        let attach_resp = json!({
            "seq": 2,
            "type": "response",
            "request_seq": attach_seq,
            "success": true,
            "command": "attach",
            "body": {}
        });
        write_dap_message(&mut stream, &attach_resp);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);
    let batch_path = tmp.path().join("batch.json");
    let batch = json!([
        {"command": " attach ", "arguments": {"justMyCode": false}},
        {"command": " configurationDone ", "arguments": {}}
    ]);
    fs::write(&batch_path, serde_json::to_string_pretty(&batch).unwrap())
        .expect("batch file written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("batch")
        .arg("--id")
        .arg("session-1")
        .arg("--file")
        .arg(batch_path)
        .arg("--timeout-ms")
        .arg("2000")
        .output()
        .expect("dap batch should run");

    assert!(output.status.success(), "dap batch should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    let responses = doc["responses"].as_array().expect("responses array");
    assert_eq!(responses.len(), 2);

    let _ = server.join();
}

#[test]
fn cli_dap_adopt_subprocess_registers_child_session_and_bootstraps_debug_sequence() {
    let parent_listener = TcpListener::bind("127.0.0.1:0").expect("parent listener should bind");
    let parent_port = parent_listener.local_addr().unwrap().port();
    let child_listener = TcpListener::bind("127.0.0.1:0").expect("child listener should bind");
    let child_port = child_listener.local_addr().unwrap().port();

    let parent_server = thread::spawn(move || {
        let (mut stream, _) = parent_listener.accept().expect("parent dap accept");
        let event = json!({
            "seq": 1,
            "type": "event",
            "event": "debugpyAttach",
            "body": {
                "name": "Subprocess 4321",
                "type": "python",
                "request": "attach",
                "connect": {
                    "host": "127.0.0.1",
                    "port": child_port
                },
                "subProcessId": 4321,
                "justMyCode": false
            }
        });
        write_dap_message(&mut stream, &event);
        thread::sleep(Duration::from_millis(200));
    });

    let child_server = thread::spawn(move || {
        let (mut stream, _) = child_listener.accept().expect("child dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone child stream"));

        let initialize = read_dap_message(&mut reader);
        assert_eq!(initialize["type"], "request");
        assert_eq!(initialize["command"], "initialize");
        let initialize_seq = initialize["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": initialize_seq,
                "success": true,
                "command": "initialize",
                "body": {}
            }),
        );

        let attach = read_dap_message(&mut reader);
        assert_eq!(attach["type"], "request");
        assert_eq!(attach["command"], "attach");
        assert_eq!(attach["arguments"]["subProcessId"], 4321);
        let attach_seq = attach["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": attach_seq,
                "success": true,
                "command": "attach",
                "body": {}
            }),
        );

        let configured = read_dap_message(&mut reader);
        assert_eq!(configured["type"], "request");
        assert_eq!(configured["command"], "configurationDone");
        let configured_seq = configured["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 3,
                "type": "response",
                "request_seq": configured_seq,
                "success": true,
                "command": "configurationDone",
                "body": {}
            }),
        );

        drop(reader);
        drop(stream);

        let (mut stream, _) = child_listener
            .accept()
            .expect("child dap second accept should succeed");
        let mut reader = BufReader::new(stream.try_clone().expect("clone child stream"));
        let threads = read_dap_message(&mut reader);
        assert_eq!(threads["type"], "request");
        assert_eq!(threads["command"], "threads");
        let threads_seq = threads["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 4,
                "type": "response",
                "request_seq": threads_seq,
                "success": true,
                "command": "threads",
                "body": {
                    "threads": [{"id": 11, "name": "SubprocessMain"}]
                }
            }),
        );
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", parent_port);

    let mut adopt_cmd = cargo_bin_cmd!("launch-code");
    let adopt_output = adopt_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("adopt-subprocess")
        .arg("--id")
        .arg("session-1")
        .arg("--timeout-ms")
        .arg("2000")
        .output()
        .expect("dap adopt-subprocess should run");
    assert!(
        adopt_output.status.success(),
        "dap adopt-subprocess should succeed"
    );
    let adopt_stdout = String::from_utf8(adopt_output.stdout).expect("stdout utf8");
    let adopt_doc: Value = serde_json::from_str(&adopt_stdout).expect("stdout json");
    assert_eq!(adopt_doc["ok"], true);
    let child_session_id = adopt_doc["child_session_id"]
        .as_str()
        .expect("child_session_id should exist")
        .to_string();
    assert_eq!(adopt_doc["endpoint"], format!("127.0.0.1:{child_port}"));

    let state_path = tmp.path().join(".launch-code").join("state.json");
    let payload = fs::read_to_string(&state_path).expect("state file should be readable");
    let state: Value = serde_json::from_str(&payload).expect("state json should parse");
    let child = state["sessions"]
        .get(&child_session_id)
        .expect("child session should be persisted");
    assert_eq!(child["debug_meta"]["host"], "127.0.0.1");
    assert_eq!(child["debug_meta"]["active_port"], child_port);
    assert_eq!(child["pid"], 4321);

    let mut threads_cmd = cargo_bin_cmd!("launch-code");
    let threads_output = threads_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("threads")
        .arg("--id")
        .arg(&child_session_id)
        .output()
        .expect("dap threads should run");
    assert!(
        threads_output.status.success(),
        "dap threads should succeed"
    );
    let threads_stdout = String::from_utf8(threads_output.stdout).expect("stdout utf8");
    let threads_doc: Value = serde_json::from_str(&threads_stdout).expect("stdout json");
    assert_eq!(threads_doc["ok"], true);
    assert_eq!(threads_doc["response"]["command"], "threads");

    let _ = parent_server.join();
    let _ = child_server.join();
}

#[test]
fn cli_dap_breakpoints_sets_multiple_lines_without_manual_json() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "setBreakpoints");
        assert_eq!(msg["arguments"]["source"]["path"], "app.py");
        let lines: Vec<u64> = msg["arguments"]["breakpoints"]
            .as_array()
            .expect("breakpoints should be array")
            .iter()
            .map(|item| item["line"].as_u64().expect("line should be number"))
            .collect();
        assert_eq!(lines, vec![12, 34]);
        assert_eq!(msg["arguments"]["breakpoints"][0]["condition"], "x > 10");
        assert_eq!(msg["arguments"]["breakpoints"][0]["hitCondition"], "==2");
        assert_eq!(
            msg["arguments"]["breakpoints"][0]["logMessage"],
            "value={x}"
        );
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        let response = json!({
            "seq": 1,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "setBreakpoints",
            "body": {
                "breakpoints": [
                    {"verified": true, "line": 12},
                    {"verified": true, "line": 34}
                ]
            }
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("breakpoints")
        .arg("--id")
        .arg("session-1")
        .arg("--path")
        .arg("app.py")
        .arg("--line")
        .arg("12")
        .arg("--line")
        .arg("34")
        .arg("--condition")
        .arg("x > 10")
        .arg("--hit-condition")
        .arg("==2")
        .arg("--log-message")
        .arg("value={x}")
        .output()
        .expect("dap breakpoints should run");

    assert!(output.status.success(), "dap breakpoints should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "setBreakpoints");

    let _ = server.join();
}

#[test]
fn cli_dap_control_commands_send_expected_dap_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        for expected in [
            ("pause", true),
            ("next", true),
            ("stepIn", true),
            ("stepOut", true),
            ("disconnect", false),
            ("terminate", false),
        ] {
            let (mut stream, _) = listener.accept().expect("dap accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

            let msg = read_dap_message(&mut reader);
            assert_eq!(msg["type"], "request");
            assert_eq!(msg["command"], expected.0);
            if expected.1 {
                assert_eq!(msg["arguments"]["threadId"], 7);
            }
            if expected.0 == "disconnect" {
                assert_eq!(msg["arguments"]["terminateDebuggee"], true);
            }
            let req_seq = msg["seq"].as_u64().expect("seq should be number");
            let response = json!({
                "seq": 1,
                "type": "response",
                "request_seq": req_seq,
                "success": true,
                "command": expected.0,
                "body": {}
            });
            write_dap_message(&mut stream, &response);
        }
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut pause = cargo_bin_cmd!("launch-code");
    let pause_output = pause
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("pause")
        .arg("--id")
        .arg("session-1")
        .arg("--thread-id")
        .arg("7")
        .output()
        .expect("dap pause should run");
    assert!(pause_output.status.success(), "dap pause should succeed");

    let mut next = cargo_bin_cmd!("launch-code");
    let next_output = next
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("next")
        .arg("--id")
        .arg("session-1")
        .arg("--thread-id")
        .arg("7")
        .output()
        .expect("dap next should run");
    assert!(next_output.status.success(), "dap next should succeed");

    let mut step_in = cargo_bin_cmd!("launch-code");
    let step_in_output = step_in
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("step-in")
        .arg("--id")
        .arg("session-1")
        .arg("--thread-id")
        .arg("7")
        .output()
        .expect("dap step-in should run");
    assert!(
        step_in_output.status.success(),
        "dap step-in should succeed"
    );

    let mut step_out = cargo_bin_cmd!("launch-code");
    let step_out_output = step_out
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("step-out")
        .arg("--id")
        .arg("session-1")
        .arg("--thread-id")
        .arg("7")
        .output()
        .expect("dap step-out should run");
    assert!(
        step_out_output.status.success(),
        "dap step-out should succeed"
    );

    let mut disconnect = cargo_bin_cmd!("launch-code");
    let disconnect_output = disconnect
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("disconnect")
        .arg("--id")
        .arg("session-1")
        .arg("--terminate-debuggee")
        .output()
        .expect("dap disconnect should run");
    assert!(
        disconnect_output.status.success(),
        "dap disconnect should succeed"
    );

    let mut terminate = cargo_bin_cmd!("launch-code");
    let terminate_output = terminate
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("terminate")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("dap terminate should run");
    assert!(
        terminate_output.status.success(),
        "dap terminate should succeed"
    );

    let _ = server.join();
}

#[test]
fn cli_dap_expression_commands_send_expected_dap_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        for expected in ["setExceptionBreakpoints", "evaluate", "setVariable"] {
            let (mut stream, _) = listener.accept().expect("dap accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

            let msg = read_dap_message(&mut reader);
            assert_eq!(msg["type"], "request");
            assert_eq!(msg["command"], expected);
            if expected == "setExceptionBreakpoints" {
                assert_eq!(msg["arguments"]["filters"][0], "raised");
                assert_eq!(msg["arguments"]["filters"][1], "uncaught");
            }
            if expected == "evaluate" {
                assert_eq!(msg["arguments"]["expression"], "counter + 1");
                assert_eq!(msg["arguments"]["frameId"], 301);
                assert_eq!(msg["arguments"]["context"], "watch");
            }
            if expected == "setVariable" {
                assert_eq!(msg["arguments"]["variablesReference"], 7001);
                assert_eq!(msg["arguments"]["name"], "counter");
                assert_eq!(msg["arguments"]["value"], "42");
            }
            let req_seq = msg["seq"].as_u64().expect("seq should be number");
            let response = json!({
                "seq": 1,
                "type": "response",
                "request_seq": req_seq,
                "success": true,
                "command": expected,
                "body": {}
            });
            write_dap_message(&mut stream, &response);
        }
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut set_exception = cargo_bin_cmd!("launch-code");
    let set_exception_output = set_exception
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("exception-breakpoints")
        .arg("--id")
        .arg("session-1")
        .arg("--filter")
        .arg("raised")
        .arg("--filter")
        .arg("uncaught")
        .output()
        .expect("dap exception-breakpoints should run");
    assert!(
        set_exception_output.status.success(),
        "dap exception-breakpoints should succeed"
    );

    let mut evaluate = cargo_bin_cmd!("launch-code");
    let evaluate_output = evaluate
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("evaluate")
        .arg("--id")
        .arg("session-1")
        .arg("--expression")
        .arg("counter + 1")
        .arg("--frame-id")
        .arg("301")
        .arg("--context")
        .arg("watch")
        .output()
        .expect("dap evaluate should run");
    assert!(
        evaluate_output.status.success(),
        "dap evaluate should succeed"
    );

    let mut set_variable = cargo_bin_cmd!("launch-code");
    let set_variable_output = set_variable
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("set-variable")
        .arg("--id")
        .arg("session-1")
        .arg("--variables-reference")
        .arg("7001")
        .arg("--name")
        .arg("counter")
        .arg("--value")
        .arg("42")
        .output()
        .expect("dap set-variable should run");
    assert!(
        set_variable_output.status.success(),
        "dap set-variable should succeed"
    );

    let _ = server.join();
}

#[test]
fn cli_dap_continue_without_thread_id_uses_first_positive_thread() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let threads_req = read_dap_message(&mut reader);
        assert_eq!(threads_req["type"], "request");
        assert_eq!(threads_req["command"], "threads");
        let threads_seq = threads_req["seq"].as_u64().expect("seq should be number");
        let threads_resp = json!({
            "seq": 1,
            "type": "response",
            "request_seq": threads_seq,
            "success": true,
            "command": "threads",
            "body": {
                "threads": [{"id": 0, "name": "InvalidThread"}, {"id": 7, "name": "MainThread"}]
            }
        });
        write_dap_message(&mut stream, &threads_resp);

        let continue_req = read_dap_message(&mut reader);
        assert_eq!(continue_req["type"], "request");
        assert_eq!(continue_req["command"], "continue");
        assert_eq!(continue_req["arguments"]["threadId"], 7);
        let continue_seq = continue_req["seq"].as_u64().expect("seq should be number");
        let continue_resp = json!({
            "seq": 2,
            "type": "response",
            "request_seq": continue_seq,
            "success": true,
            "command": "continue",
            "body": {}
        });
        write_dap_message(&mut stream, &continue_resp);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("continue")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("dap continue should run");

    assert!(output.status.success(), "dap continue should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "continue");
    assert_eq!(doc["thread_id"], 7);

    let _ = server.join();
}

#[test]
fn cli_dap_events_can_poll_event_queue() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let event = json!({
            "seq": 1,
            "type": "event",
            "event": "stopped",
            "body": {
                "reason": "breakpoint",
                "threadId": 1
            }
        });
        write_dap_message(&mut stream, &event);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("events")
        .arg("--id")
        .arg("session-1")
        .arg("--max")
        .arg("10")
        .arg("--timeout-ms")
        .arg("1000")
        .output()
        .expect("dap events should run");

    assert!(output.status.success(), "dap events should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    let events = doc["events"].as_array().expect("events should be array");
    assert!(
        events.iter().any(|item| item["event"] == "stopped"),
        "expected stopped event"
    );

    let _ = server.join();
}

#[test]
fn cli_dap_threads_requests_threads_command() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "threads");
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        let response = json!({
            "seq": 1,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "threads",
            "body": {
                "threads": [{"id": 11, "name": "MainThread"}]
            }
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("threads")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("dap threads should run");

    assert!(output.status.success(), "dap threads should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "threads");
    assert_eq!(doc["response"]["body"]["threads"][0]["id"], 11);

    let _ = server.join();
}

#[test]
fn cli_dap_stack_trace_without_thread_id_uses_first_positive_thread() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let threads_req = read_dap_message(&mut reader);
        assert_eq!(threads_req["type"], "request");
        assert_eq!(threads_req["command"], "threads");
        let threads_seq = threads_req["seq"].as_u64().expect("seq should be number");
        let threads_resp = json!({
            "seq": 1,
            "type": "response",
            "request_seq": threads_seq,
            "success": true,
            "command": "threads",
            "body": {
                "threads": [{"id": 0, "name": "InvalidThread"}, {"id": 21, "name": "MainThread"}]
            }
        });
        write_dap_message(&mut stream, &threads_resp);

        let stack_req = read_dap_message(&mut reader);
        assert_eq!(stack_req["type"], "request");
        assert_eq!(stack_req["command"], "stackTrace");
        assert_eq!(stack_req["arguments"]["threadId"], 21);
        assert_eq!(stack_req["arguments"]["levels"], 20);
        let stack_seq = stack_req["seq"].as_u64().expect("seq should be number");
        let stack_resp = json!({
            "seq": 2,
            "type": "response",
            "request_seq": stack_seq,
            "success": true,
            "command": "stackTrace",
            "body": {
                "stackFrames": [{"id": 301, "name": "main", "line": 12, "column": 1}],
                "totalFrames": 1
            }
        });
        write_dap_message(&mut stream, &stack_resp);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("stack-trace")
        .arg("--id")
        .arg("session-1")
        .arg("--levels")
        .arg("20")
        .output()
        .expect("dap stack-trace should run");

    assert!(output.status.success(), "dap stack-trace should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["thread_id"], 21);
    assert_eq!(doc["response"]["command"], "stackTrace");
    assert_eq!(doc["response"]["body"]["stackFrames"][0]["id"], 301);

    let _ = server.join();
}

#[test]
fn cli_dap_scopes_requests_scopes_for_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "scopes");
        assert_eq!(msg["arguments"]["frameId"], 301);
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        let response = json!({
            "seq": 1,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "scopes",
            "body": {
                "scopes": [
                    {"name": "Locals", "variablesReference": 7001, "expensive": false}
                ]
            }
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("scopes")
        .arg("--id")
        .arg("session-1")
        .arg("--frame-id")
        .arg("301")
        .output()
        .expect("dap scopes should run");

    assert!(output.status.success(), "dap scopes should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "scopes");
    assert_eq!(
        doc["response"]["body"]["scopes"][0]["variablesReference"],
        7001
    );

    let _ = server.join();
}

#[test]
fn cli_dap_variables_requests_variables_with_paging_options() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let port = listener.local_addr().unwrap().port();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "variables");
        assert_eq!(msg["arguments"]["variablesReference"], 7001);
        assert_eq!(msg["arguments"]["filter"], "named");
        assert_eq!(msg["arguments"]["start"], 1);
        assert_eq!(msg["arguments"]["count"], 2);
        let req_seq = msg["seq"].as_u64().expect("seq should be number");

        let response = json!({
            "seq": 1,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "variables",
            "body": {
                "variables": [
                    {"name": "x", "value": "1", "type": "int", "variablesReference": 0}
                ]
            }
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_debug_session(tmp.path(), "127.0.0.1", port);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("dap")
        .arg("variables")
        .arg("--id")
        .arg("session-1")
        .arg("--variables-reference")
        .arg("7001")
        .arg("--filter")
        .arg("named")
        .arg("--start")
        .arg("1")
        .arg("--count")
        .arg("2")
        .output()
        .expect("dap variables should run");

    assert!(output.status.success(), "dap variables should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "variables");
    assert_eq!(doc["response"]["body"]["variables"][0]["name"], "x");

    let _ = server.join();
}

#[test]
fn cli_dap_threads_supports_node_bridge_adapter_when_configured() {
    let Some(python_bin) = discover_python_bin() else {
        eprintln!("python is unavailable; skipping node dap bridge test");
        return;
    };

    let tmp = tempdir().expect("temp dir should exist");
    write_state_with_node_debug_session(tmp.path(), "127.0.0.1", 9229);

    let script_path = tmp.path().join("mock_node_dap_adapter.py");
    let log_path = tmp.path().join("mock_node_dap_commands.log");
    let script = r#"import json
import pathlib
import sys

log_path = pathlib.Path(sys.argv[1])
expected_port = int(sys.argv[2])

def log(value):
    with log_path.open("a", encoding="utf-8") as stream:
        stream.write(value + "\n")

def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        text = line.decode("utf-8").strip("\r\n")
        if text == "":
            break
        name, value = text.split(":", 1)
        headers[name.strip().lower()] = value.strip()

    length = int(headers.get("content-length", "0"))
    if length <= 0:
        return None
    payload = sys.stdin.buffer.read(length)
    if not payload:
        return None
    return json.loads(payload.decode("utf-8"))

def write_message(message):
    payload = json.dumps(message).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(payload)}\r\n\r\n".encode("utf-8"))
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()

threads_seen = 0
seq = 1

while True:
    request = read_message()
    if request is None:
        break

    command = request.get("command", "")
    log(command)

    response = {
        "seq": seq,
        "type": "response",
        "request_seq": request.get("seq", 0),
        "success": True,
        "command": command,
        "body": {},
    }
    seq += 1

    if command == "threads":
        threads_seen += 1
        if threads_seen == 1:
            response["success"] = False
            response["message"] = "Server is not available"
        else:
            response["body"] = {"threads": [{"id": 9, "name": "Main"}]}
    elif command == "attach":
        arguments = request.get("arguments", {})
        if arguments.get("address") != "127.0.0.1" or int(arguments.get("port", 0)) != expected_port:
            response["success"] = False
            response["message"] = "attach arguments mismatch"
    elif command in ("initialize", "configurationDone"):
        pass
    else:
        response["success"] = False
        response["message"] = "unsupported command"

    write_message(response)
"#;
    fs::write(&script_path, script).expect("mock node adapter script should be written");

    let adapter_cmd = serde_json::to_string(&json!([
        python_bin,
        script_path.to_string_lossy().to_string(),
        log_path.to_string_lossy().to_string(),
        "9229"
    ]))
    .expect("adapter command json");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env("LCODE_NODE_DAP_ADAPTER_CMD", adapter_cmd)
        .arg("dap")
        .arg("threads")
        .arg("--id")
        .arg("session-node")
        .arg("--timeout-ms")
        .arg("3000")
        .output()
        .expect("node dap threads should run");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        output.status.success(),
        "node dap threads should succeed, stdout: {stdout}, stderr: {stderr}"
    );
    let doc: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["response"]["command"], "threads");
    assert_eq!(doc["response"]["body"]["threads"][0]["id"], 9);

    let log = fs::read_to_string(&log_path).expect("adapter command log should be readable");
    let commands = log
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>();
    assert_eq!(
        commands,
        vec![
            "threads",
            "initialize",
            "attach",
            "configurationDone",
            "threads"
        ]
    );
}
