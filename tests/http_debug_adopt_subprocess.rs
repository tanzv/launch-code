use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::tempdir;

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while Instant::now() < deadline {
        line.clear();
        if stdout
            .read_line(&mut line)
            .ok()
            .filter(|value| *value > 0)
            .is_some()
        {
            return line;
        }
        thread::sleep(Duration::from_millis(20));
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
fn serve_can_adopt_python_subprocess_session_and_route_commands() {
    let parent_listener = TcpListener::bind("127.0.0.1:0").expect("parent listener should bind");
    let parent_port = parent_listener.local_addr().unwrap().port();
    let child_listener = TcpListener::bind("127.0.0.1:0").expect("child listener should bind");
    let child_port = child_listener.local_addr().unwrap().port();

    let parent_thread = thread::spawn(move || {
        let (mut stream, _) = parent_listener.accept().expect("parent dap accept");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 1,
                "type": "event",
                "event": "debugpyAttach",
                "body": {
                    "name": "Subprocess 9876",
                    "type": "python",
                    "request": "attach",
                    "connect": {
                        "host": "127.0.0.1",
                        "port": child_port
                    },
                    "subProcessId": 9876,
                    "justMyCode": false
                }
            }),
        );
        thread::sleep(Duration::from_millis(200));
    });

    let child_thread = thread::spawn(move || {
        let (mut stream, _) = child_listener.accept().expect("child dap accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone child stream"));

        let initialize = read_dap_message(&mut reader);
        assert_eq!(initialize["command"], "initialize");
        let initialize_seq = initialize["seq"].as_u64().expect("seq should exist");
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
        assert_eq!(attach["command"], "attach");
        assert_eq!(attach["arguments"]["subProcessId"], 9876);
        let attach_seq = attach["seq"].as_u64().expect("seq should exist");
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
        assert_eq!(configured["command"], "configurationDone");
        let configured_seq = configured["seq"].as_u64().expect("seq should exist");
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
        assert_eq!(threads["command"], "threads");
        let threads_seq = threads["seq"].as_u64().expect("seq should exist");
        write_dap_message(
            &mut stream,
            &json!({
                "seq": 4,
                "type": "response",
                "request_seq": threads_seq,
                "success": true,
                "command": "threads",
                "body": {
                    "threads": [{"id": 1, "name": "SubprocessMain"}]
                }
            }),
        );
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
                        "port": parent_port,
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
                    "requested_port": parent_port,
                    "active_port": parent_port,
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
    let mut serve = Command::new(exe)
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

    let stdout = serve.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let base_url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into();

    let adopt_url = format!("{base_url}/v1/sessions/session-1/debug/adopt-subprocess");
    let mut adopt_res = agent
        .post(&adopt_url)
        .header("Authorization", "Bearer testtoken")
        .header("Content-Type", "application/json")
        .send("{}")
        .expect("adopt-subprocess should succeed");
    assert_eq!(adopt_res.status(), ureq::http::StatusCode::OK);
    let adopt_text = adopt_res
        .body_mut()
        .read_to_string()
        .expect("adopt response readable");
    let adopt_doc: Value = serde_json::from_str(&adopt_text).expect("adopt response json");
    assert_eq!(adopt_doc["ok"], true);
    let child_session_id = adopt_doc["child_session_id"]
        .as_str()
        .expect("child_session_id should exist")
        .to_string();
    assert_eq!(adopt_doc["endpoint"], format!("127.0.0.1:{child_port}"));

    let threads_url = format!("{base_url}/v1/sessions/{child_session_id}/debug/threads");
    let mut threads_res = agent
        .get(&threads_url)
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("threads should succeed");
    assert_eq!(threads_res.status(), ureq::http::StatusCode::OK);
    let threads_text = threads_res
        .body_mut()
        .read_to_string()
        .expect("threads response readable");
    let threads_doc: Value = serde_json::from_str(&threads_text).expect("threads response json");
    assert_eq!(threads_doc["ok"], true);
    assert_eq!(threads_doc["response"]["command"], "threads");

    let _ = serve.kill();
    let _ = serve.wait();
    let _ = parent_thread.join();
    let _ = child_thread.join();
}
