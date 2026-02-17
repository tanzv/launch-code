use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::tempdir;

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while std::time::Instant::now() < deadline {
        line.clear();
        if stdout
            .read_line(&mut line)
            .ok()
            .filter(|v| *v > 0)
            .is_some()
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

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

#[test]
fn serve_exposes_high_level_debug_endpoints() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().unwrap().port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "setBreakpoints");
        assert_eq!(msg["arguments"]["source"]["path"], "app.py");
        let lines: Vec<i64> = msg["arguments"]["breakpoints"]
            .as_array()
            .expect("breakpoints should be array")
            .iter()
            .map(|item| item["line"].as_i64().expect("line should be number"))
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

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "threads");
        let req_seq = msg["seq"].as_u64().expect("seq should be number");
        let response = json!({
            "seq": 2,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "threads",
            "body": {
                "threads": [{"id": 0, "name": "Invalid"}, {"id": 1, "name": "Main"}]
            }
        });
        write_dap_message(&mut stream, &response);

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "continue");
        assert_eq!(msg["arguments"]["threadId"], 1);
        let req_seq = msg["seq"].as_u64().expect("seq should be number");
        let response = json!({
            "seq": 3,
            "type": "response",
            "request_seq": req_seq,
            "success": true,
            "command": "continue",
            "body": {}
        });
        write_dap_message(&mut stream, &response);
    });

    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    std::fs::create_dir_all(&state_dir).expect("state dir should exist");

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
                        "host": "127.0.0.1",
                        "port": dap_port,
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
                    "host": "127.0.0.1",
                    "requested_port": dap_port,
                    "active_port": dap_port,
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

    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();

    let breakpoints_body = json!({
        "path": "app.py",
        "lines": [
            {"line": 12, "condition": "x > 10", "hitCondition": "==2", "logMessage": "value={x}"},
            {"line": 34, "condition": "x > 10", "hitCondition": "==2", "logMessage": "value={x}"}
        ]
    });
    let mut breakpoints_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/breakpoints"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&breakpoints_body).unwrap())
        .expect("breakpoints should succeed");
    assert_eq!(breakpoints_res.status(), ureq::http::StatusCode::OK);
    let breakpoints_text = breakpoints_res
        .body_mut()
        .read_to_string()
        .expect("breakpoints body readable");
    let breakpoints_json: Value =
        serde_json::from_str(&breakpoints_text).expect("breakpoints response json");
    assert_eq!(breakpoints_json["ok"], true);
    assert_eq!(breakpoints_json["response"]["command"], "setBreakpoints");

    let continue_body = json!({});
    let mut continue_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/continue"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&continue_body).unwrap())
        .expect("continue should succeed");
    assert_eq!(continue_res.status(), ureq::http::StatusCode::OK);
    let continue_text = continue_res
        .body_mut()
        .read_to_string()
        .expect("continue body readable");
    let continue_json: Value =
        serde_json::from_str(&continue_text).expect("continue response json");
    assert_eq!(continue_json["ok"], true);
    assert_eq!(continue_json["response"]["command"], "continue");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_exposes_debug_control_endpoints() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().unwrap().port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        for expected in [
            ("pause", true),
            ("next", true),
            ("stepIn", true),
            ("stepOut", true),
            ("disconnect", false),
            ("terminate", false),
        ] {
            let msg = read_dap_message(&mut reader);
            assert_eq!(msg["type"], "request");
            assert_eq!(msg["command"], expected.0);
            if expected.1 {
                assert_eq!(msg["arguments"]["threadId"], 7);
            }
            if expected.0 == "disconnect" {
                assert_eq!(msg["arguments"]["terminateDebuggee"], true);
            }
            if expected.0 == "terminate" {
                assert_eq!(msg["arguments"]["restart"], false);
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
    let state_dir = tmp.path().join(".launch-code");
    std::fs::create_dir_all(&state_dir).expect("state dir should exist");

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
                        "host": "127.0.0.1",
                        "port": dap_port,
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
                    "host": "127.0.0.1",
                    "requested_port": dap_port,
                    "active_port": dap_port,
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

    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();

    let mut pause_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/pause"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId": 7})).unwrap())
        .expect("pause should succeed");
    assert_eq!(pause_res.status(), ureq::http::StatusCode::OK);
    let pause_body = pause_res
        .body_mut()
        .read_to_string()
        .expect("pause body readable");
    let pause_json: Value = serde_json::from_str(&pause_body).expect("pause response json");
    assert_eq!(pause_json["ok"], true);
    assert_eq!(pause_json["response"]["command"], "pause");

    let mut next_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/next"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId": 7})).unwrap())
        .expect("next should succeed");
    assert_eq!(next_res.status(), ureq::http::StatusCode::OK);
    let next_body = next_res
        .body_mut()
        .read_to_string()
        .expect("next body readable");
    let next_json: Value = serde_json::from_str(&next_body).expect("next response json");
    assert_eq!(next_json["ok"], true);
    assert_eq!(next_json["response"]["command"], "next");

    let mut step_in_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/step-in"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId": 7})).unwrap())
        .expect("step-in should succeed");
    assert_eq!(step_in_res.status(), ureq::http::StatusCode::OK);
    let step_in_body = step_in_res
        .body_mut()
        .read_to_string()
        .expect("step-in body readable");
    let step_in_json: Value = serde_json::from_str(&step_in_body).expect("step-in response json");
    assert_eq!(step_in_json["ok"], true);
    assert_eq!(step_in_json["response"]["command"], "stepIn");

    let mut step_out_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/step-out"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId": 7})).unwrap())
        .expect("step-out should succeed");
    assert_eq!(step_out_res.status(), ureq::http::StatusCode::OK);
    let step_out_body = step_out_res
        .body_mut()
        .read_to_string()
        .expect("step-out body readable");
    let step_out_json: Value =
        serde_json::from_str(&step_out_body).expect("step-out response json");
    assert_eq!(step_out_json["ok"], true);
    assert_eq!(step_out_json["response"]["command"], "stepOut");

    let mut disconnect_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/disconnect"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"terminateDebuggee": true})).unwrap())
        .expect("disconnect should succeed");
    assert_eq!(disconnect_res.status(), ureq::http::StatusCode::OK);
    let disconnect_body = disconnect_res
        .body_mut()
        .read_to_string()
        .expect("disconnect body readable");
    let disconnect_json: Value =
        serde_json::from_str(&disconnect_body).expect("disconnect response json");
    assert_eq!(disconnect_json["ok"], true);
    assert_eq!(disconnect_json["response"]["command"], "disconnect");

    let mut terminate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/terminate"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"restart": false})).unwrap())
        .expect("terminate should succeed");
    assert_eq!(terminate_res.status(), ureq::http::StatusCode::OK);
    let terminate_body = terminate_res
        .body_mut()
        .read_to_string()
        .expect("terminate body readable");
    let terminate_json: Value =
        serde_json::from_str(&terminate_body).expect("terminate response json");
    assert_eq!(terminate_json["ok"], true);
    assert_eq!(terminate_json["response"]["command"], "terminate");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_exposes_debug_expression_endpoints() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().unwrap().port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        for expected in ["setExceptionBreakpoints", "evaluate", "setVariable"] {
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
    let state_dir = tmp.path().join(".launch-code");
    std::fs::create_dir_all(&state_dir).expect("state dir should exist");

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
                        "host": "127.0.0.1",
                        "port": dap_port,
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
                    "host": "127.0.0.1",
                    "requested_port": dap_port,
                    "active_port": dap_port,
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

    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();

    let mut set_exception_res = agent
        .post(&format!(
            "{url}/v1/sessions/session-1/debug/exception-breakpoints"
        ))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"filters":["raised","uncaught"]})).unwrap())
        .expect("exception-breakpoints should succeed");
    assert_eq!(set_exception_res.status(), ureq::http::StatusCode::OK);
    let set_exception_body = set_exception_res
        .body_mut()
        .read_to_string()
        .expect("exception-breakpoints body readable");
    let set_exception_json: Value =
        serde_json::from_str(&set_exception_body).expect("exception-breakpoints response json");
    assert_eq!(set_exception_json["ok"], true);
    assert_eq!(
        set_exception_json["response"]["command"],
        "setExceptionBreakpoints"
    );

    let mut evaluate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/evaluate"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"expression":"counter + 1","frameId":301,"context":"watch"}),
            )
            .unwrap(),
        )
        .expect("evaluate should succeed");
    assert_eq!(evaluate_res.status(), ureq::http::StatusCode::OK);
    let evaluate_body = evaluate_res
        .body_mut()
        .read_to_string()
        .expect("evaluate body readable");
    let evaluate_json: Value =
        serde_json::from_str(&evaluate_body).expect("evaluate response json");
    assert_eq!(evaluate_json["ok"], true);
    assert_eq!(evaluate_json["response"]["command"], "evaluate");

    let mut set_variable_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/set-variable"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"variablesReference":7001,"name":"counter","value":"42"}),
            )
            .unwrap(),
        )
        .expect("set-variable should succeed");
    assert_eq!(set_variable_res.status(), ureq::http::StatusCode::OK);
    let set_variable_body = set_variable_res
        .body_mut()
        .read_to_string()
        .expect("set-variable body readable");
    let set_variable_json: Value =
        serde_json::from_str(&set_variable_body).expect("set-variable response json");
    assert_eq!(set_variable_json["ok"], true);
    assert_eq!(set_variable_json["response"]["command"], "setVariable");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}
