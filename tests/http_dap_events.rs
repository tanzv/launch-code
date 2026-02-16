use std::fs;
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
fn serve_exposes_dap_events_endpoint() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener.local_addr().unwrap().port();

    let dap_thread = thread::spawn(move || {
        let (mut stream, _) = dap_listener.accept().expect("dap accept");
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
            "body": {}
        });
        write_dap_message(&mut stream, &response);

        let event = json!({
            "seq": 2,
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
    let state_dir = tmp.path().join(".launch-code");
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

    fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
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

    let init_body = json!({
        "command": "initialize",
        "arguments": {}
    });

    let mut init_res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/dap/request"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&init_body).unwrap())
        .expect("dap initialize should succeed");
    assert_eq!(init_res.status(), ureq::http::StatusCode::OK);
    let _ = init_res
        .body_mut()
        .read_to_string()
        .expect("init response readable");

    let mut events_res = agent
        .get(&format!(
            "{url}/v1/sessions/session-1/debug/dap/events?timeout_ms=1000&max=10"
        ))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("events should succeed");
    assert_eq!(events_res.status(), ureq::http::StatusCode::OK);
    let events_body = events_res
        .body_mut()
        .read_to_string()
        .expect("events body readable");
    let events_json: Value = serde_json::from_str(&events_body).expect("events json");
    assert_eq!(events_json["ok"], true);
    let events = events_json["events"]
        .as_array()
        .expect("events should be array");
    assert!(
        events.iter().any(|item| item["event"] == "stopped"),
        "stopped event should be present"
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}
