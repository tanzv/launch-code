use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

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

fn write_debug_session_state(tmp: &TempDir, dap_port: u16) {
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
}

fn start_http_server(tmp: &TempDir) -> (Child, String) {
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
    let _stdout_drain = thread::spawn(move || {
        let mut scratch = String::new();
        loop {
            scratch.clear();
            match reader.read_line(&mut scratch) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    (child, url)
}

fn read_json_response(res: &mut ureq::http::Response<ureq::Body>) -> Value {
    let text = res
        .body_mut()
        .read_to_string()
        .expect("response body should be readable");
    serde_json::from_str(&text).expect("response json")
}

#[test]
fn serve_rejects_non_numeric_thread_id_and_allows_followup_pause() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().expect("local addr").port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "pause");
        assert_eq!(msg["arguments"]["threadId"], 7);
        let req_seq = msg["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": req_seq,
                "success": true,
                "command": "pause",
                "body": {}
            }),
        );
    });

    let tmp = tempfile::tempdir().expect("temp dir should exist");
    write_debug_session_state(&tmp, dap_port);
    let (mut child, url) = start_http_server(&tmp);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/pause"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId":"main"})).unwrap())
        .expect("bad pause request should complete");
    assert_eq!(bad_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_doc = read_json_response(&mut bad_res);
    assert_eq!(bad_doc["ok"], false);
    assert_eq!(bad_doc["error"], "bad_request");
    assert_eq!(bad_doc["message"], "threadId must be a positive integer");

    let mut zero_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/pause"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId":0})).unwrap())
        .expect("zero thread pause request should complete");
    assert_eq!(zero_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let zero_doc = read_json_response(&mut zero_res);
    assert_eq!(zero_doc["ok"], false);
    assert_eq!(zero_doc["error"], "bad_request");
    assert_eq!(zero_doc["message"], "threadId must be a positive integer");

    let mut good_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/pause"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"threadId":7})).unwrap())
        .expect("good pause request should complete");
    assert_eq!(good_res.status(), ureq::http::StatusCode::OK);
    let good_doc = read_json_response(&mut good_res);
    assert_eq!(good_doc["ok"], true);
    assert_eq!(good_doc["response"]["command"], "pause");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_rejects_non_boolean_disconnect_flags_and_allows_followup_disconnect() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().expect("local addr").port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let msg = read_dap_message(&mut reader);
        assert_eq!(msg["type"], "request");
        assert_eq!(msg["command"], "disconnect");
        assert_eq!(msg["arguments"]["terminateDebuggee"], true);
        assert_eq!(msg["arguments"]["suspendDebuggee"], false);
        let req_seq = msg["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": req_seq,
                "success": true,
                "command": "disconnect",
                "body": {}
            }),
        );
    });

    let tmp = tempfile::tempdir().expect("temp dir should exist");
    write_debug_session_state(&tmp, dap_port);
    let (mut child, url) = start_http_server(&tmp);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/disconnect"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"terminateDebuggee":"yes"})).unwrap())
        .expect("bad disconnect request should complete");
    assert_eq!(bad_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_doc = read_json_response(&mut bad_res);
    assert_eq!(bad_doc["ok"], false);
    assert_eq!(bad_doc["error"], "bad_request");

    let mut good_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/disconnect"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"terminateDebuggee":true})).unwrap())
        .expect("good disconnect request should complete");
    assert_eq!(good_res.status(), ureq::http::StatusCode::OK);
    let good_doc = read_json_response(&mut good_res);
    assert_eq!(good_doc["ok"], true);
    assert_eq!(good_doc["response"]["command"], "disconnect");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_rejects_invalid_expression_payload_types_and_allows_followup_requests() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().expect("local addr").port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let evaluate_msg = read_dap_message(&mut reader);
        assert_eq!(evaluate_msg["type"], "request");
        assert_eq!(evaluate_msg["command"], "evaluate");
        assert_eq!(evaluate_msg["arguments"]["expression"], "counter + 1");
        assert_eq!(evaluate_msg["arguments"]["frameId"], 301);
        assert_eq!(evaluate_msg["arguments"]["context"], "watch");
        let evaluate_seq = evaluate_msg["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": evaluate_seq,
                "success": true,
                "command": "evaluate",
                "body": {}
            }),
        );

        let set_variable_msg = read_dap_message(&mut reader);
        assert_eq!(set_variable_msg["type"], "request");
        assert_eq!(set_variable_msg["command"], "setVariable");
        assert_eq!(set_variable_msg["arguments"]["variablesReference"], 7001);
        assert_eq!(set_variable_msg["arguments"]["name"], "counter");
        assert_eq!(set_variable_msg["arguments"]["value"], "42");
        let set_variable_seq = set_variable_msg["seq"]
            .as_u64()
            .expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": set_variable_seq,
                "success": true,
                "command": "setVariable",
                "body": {}
            }),
        );

        let exception_msg = read_dap_message(&mut reader);
        assert_eq!(exception_msg["type"], "request");
        assert_eq!(exception_msg["command"], "setExceptionBreakpoints");
        assert_eq!(exception_msg["arguments"]["filters"][0], "raised");
        let exception_seq = exception_msg["seq"].as_u64().expect("seq should be number");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 3,
                "type": "response",
                "request_seq": exception_seq,
                "success": true,
                "command": "setExceptionBreakpoints",
                "body": {}
            }),
        );
    });

    let tmp = tempfile::tempdir().expect("temp dir should exist");
    write_debug_session_state(&tmp, dap_port);
    let (mut child, url) = start_http_server(&tmp);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_evaluate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/evaluate"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"expression":"counter + 1","frameId":"main"})).unwrap())
        .expect("bad evaluate request should complete");
    assert_eq!(
        bad_evaluate_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_evaluate_doc = read_json_response(&mut bad_evaluate_res);
    assert_eq!(bad_evaluate_doc["ok"], false);
    assert_eq!(bad_evaluate_doc["error"], "bad_request");

    let mut zero_frame_evaluate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/evaluate"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"expression":"counter + 1","frameId":0})).unwrap())
        .expect("zero frameId evaluate request should complete");
    assert_eq!(
        zero_frame_evaluate_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let zero_frame_evaluate_doc = read_json_response(&mut zero_frame_evaluate_res);
    assert_eq!(zero_frame_evaluate_doc["ok"], false);
    assert_eq!(zero_frame_evaluate_doc["error"], "bad_request");
    assert_eq!(
        zero_frame_evaluate_doc["message"],
        "frameId must be a positive integer"
    );

    let mut good_evaluate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/evaluate"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"expression":"counter + 1","frameId":301,"context":"watch"}),
            )
            .unwrap(),
        )
        .expect("good evaluate request should complete");
    assert_eq!(good_evaluate_res.status(), ureq::http::StatusCode::OK);
    let good_evaluate_doc = read_json_response(&mut good_evaluate_res);
    assert_eq!(good_evaluate_doc["ok"], true);
    assert_eq!(good_evaluate_doc["response"]["command"], "evaluate");

    let mut bad_set_variable_zero_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/set-variable"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"variablesReference":0,"name":"counter","value":"42","timeout_ms":1500}),
            )
            .unwrap(),
        )
        .expect("zero variablesReference set-variable request should complete");
    assert_eq!(
        bad_set_variable_zero_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_set_variable_zero_doc = read_json_response(&mut bad_set_variable_zero_res);
    assert_eq!(bad_set_variable_zero_doc["ok"], false);
    assert_eq!(bad_set_variable_zero_doc["error"], "bad_request");
    assert_eq!(
        bad_set_variable_zero_doc["message"],
        "variablesReference must be a positive integer"
    );

    let mut bad_set_variable_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/set-variable"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"variablesReference":7001,"name":"counter","value":"42","timeout_ms":"fast"}),
            )
            .unwrap(),
        )
        .expect("bad set-variable request should complete");
    assert_eq!(
        bad_set_variable_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_set_variable_doc = read_json_response(&mut bad_set_variable_res);
    assert_eq!(bad_set_variable_doc["ok"], false);
    assert_eq!(bad_set_variable_doc["error"], "bad_request");

    let mut good_set_variable_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/set-variable"))
        .header("Authorization", "Bearer testtoken")
        .send(
            serde_json::to_string(
                &json!({"variablesReference":7001,"name":"counter","value":"42","timeout_ms":1500}),
            )
            .unwrap(),
        )
        .expect("good set-variable request should complete");
    assert_eq!(good_set_variable_res.status(), ureq::http::StatusCode::OK);
    let good_set_variable_doc = read_json_response(&mut good_set_variable_res);
    assert_eq!(good_set_variable_doc["ok"], true);
    assert_eq!(good_set_variable_doc["response"]["command"], "setVariable");

    let mut bad_exception_res = agent
        .post(&format!(
            "{url}/v1/sessions/session-1/debug/exception-breakpoints"
        ))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"filters":["raised"],"timeout_ms":"slow"})).unwrap())
        .expect("bad exception-breakpoints request should complete");
    assert_eq!(
        bad_exception_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_exception_doc = read_json_response(&mut bad_exception_res);
    assert_eq!(bad_exception_doc["ok"], false);
    assert_eq!(bad_exception_doc["error"], "bad_request");

    let mut good_exception_res = agent
        .post(&format!(
            "{url}/v1/sessions/session-1/debug/exception-breakpoints"
        ))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"filters":["raised"],"timeout_ms":1500})).unwrap())
        .expect("good exception-breakpoints request should complete");
    assert_eq!(good_exception_res.status(), ureq::http::StatusCode::OK);
    let good_exception_doc = read_json_response(&mut good_exception_res);
    assert_eq!(good_exception_doc["ok"], true);
    assert_eq!(
        good_exception_doc["response"]["command"],
        "setExceptionBreakpoints"
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_rejects_invalid_session_control_types_and_allows_followup_requests() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().expect("local addr").port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let disconnect_msg = read_dap_message(&mut reader);
        assert_eq!(disconnect_msg["type"], "request");
        assert_eq!(disconnect_msg["command"], "disconnect");
        assert_eq!(disconnect_msg["arguments"]["terminateDebuggee"], true);
        assert_eq!(disconnect_msg["arguments"]["suspendDebuggee"], false);
        let disconnect_seq = disconnect_msg["seq"].as_u64().expect("seq should exist");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": disconnect_seq,
                "success": true,
                "command": "disconnect",
                "body": {}
            }),
        );

        let terminate_msg = read_dap_message(&mut reader);
        assert_eq!(terminate_msg["type"], "request");
        assert_eq!(terminate_msg["command"], "terminate");
        assert_eq!(terminate_msg["arguments"]["restart"], true);
        let terminate_seq = terminate_msg["seq"].as_u64().expect("seq should exist");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 2,
                "type": "response",
                "request_seq": terminate_seq,
                "success": true,
                "command": "terminate",
                "body": {}
            }),
        );
    });

    let tmp = tempfile::tempdir().expect("temp dir should exist");
    write_debug_session_state(&tmp, dap_port);
    let (mut child, url) = start_http_server(&tmp);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_disconnect_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/disconnect"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"timeout_ms":"slow"})).unwrap())
        .expect("bad disconnect request should complete");
    assert_eq!(
        bad_disconnect_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_disconnect_doc = read_json_response(&mut bad_disconnect_res);
    assert_eq!(bad_disconnect_doc["ok"], false);
    assert_eq!(bad_disconnect_doc["error"], "bad_request");

    let mut good_disconnect_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/disconnect"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"terminateDebuggee":true,"timeout_ms":1500})).unwrap())
        .expect("good disconnect request should complete");
    assert_eq!(good_disconnect_res.status(), ureq::http::StatusCode::OK);
    let good_disconnect_doc = read_json_response(&mut good_disconnect_res);
    assert_eq!(good_disconnect_doc["ok"], true);
    assert_eq!(good_disconnect_doc["response"]["command"], "disconnect");

    let mut bad_terminate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/terminate"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"restart":"yes"})).unwrap())
        .expect("bad terminate request should complete");
    assert_eq!(
        bad_terminate_res.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_terminate_doc = read_json_response(&mut bad_terminate_res);
    assert_eq!(bad_terminate_doc["ok"], false);
    assert_eq!(bad_terminate_doc["error"], "bad_request");

    let mut good_terminate_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/terminate"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"restart":true,"timeout_ms":1500})).unwrap())
        .expect("good terminate request should complete");
    assert_eq!(good_terminate_res.status(), ureq::http::StatusCode::OK);
    let good_terminate_doc = read_json_response(&mut good_terminate_res);
    assert_eq!(good_terminate_doc["ok"], true);
    assert_eq!(good_terminate_doc["response"]["command"], "terminate");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}

#[test]
fn serve_rejects_invalid_exception_breakpoint_filters_and_allows_followup_request() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().expect("local addr").port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let exception_msg = read_dap_message(&mut reader);
        assert_eq!(exception_msg["type"], "request");
        assert_eq!(exception_msg["command"], "setExceptionBreakpoints");
        assert_eq!(exception_msg["arguments"]["filters"][0], "raised");
        assert_eq!(exception_msg["arguments"]["filters"][1], "uncaught");
        let exception_seq = exception_msg["seq"].as_u64().expect("seq should exist");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "response",
                "request_seq": exception_seq,
                "success": true,
                "command": "setExceptionBreakpoints",
                "body": {}
            }),
        );
    });

    let tmp = tempfile::tempdir().expect("temp dir should exist");
    write_debug_session_state(&tmp, dap_port);
    let (mut child, url) = start_http_server(&tmp);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_res = agent
        .post(&format!(
            "{url}/v1/sessions/session-1/debug/exception-breakpoints"
        ))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"filters":"raised"})).unwrap())
        .expect("bad exception-breakpoints request should complete");
    assert_eq!(bad_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_doc = read_json_response(&mut bad_res);
    assert_eq!(bad_doc["ok"], false);
    assert_eq!(bad_doc["error"], "bad_request");

    let mut good_res = agent
        .post(&format!(
            "{url}/v1/sessions/session-1/debug/exception-breakpoints"
        ))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&json!({"filters":["raised","uncaught"]})).unwrap())
        .expect("good exception-breakpoints request should complete");
    assert_eq!(good_res.status(), ureq::http::StatusCode::OK);
    let good_doc = read_json_response(&mut good_res);
    assert_eq!(good_doc["ok"], true);
    assert_eq!(good_doc["response"]["command"], "setExceptionBreakpoints");

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}
