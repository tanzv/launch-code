use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
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
            && line.contains("listening=")
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

#[test]
fn serve_returns_503_when_request_queue_is_saturated() {
    let dap_listener = TcpListener::bind("127.0.0.1:0").expect("dap listener should bind");
    let dap_port = dap_listener
        .local_addr()
        .expect("dap listener should expose local addr")
        .port();

    let dap_thread = thread::spawn(move || {
        let (_stream, _) = dap_listener.accept().expect("dap accept should succeed");
        std::thread::sleep(Duration::from_secs(5));
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
        .arg("--workers")
        .arg("1")
        .arg("--queue-capacity")
        .arg("0")
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

    let blocking_url =
        format!("{url}/v1/sessions/session-1/debug/dap/events?timeout_ms=4000&max=1");
    let blocking_handle = thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .build()
            .into();
        let mut response = agent
            .get(&blocking_url)
            .header("Authorization", "Bearer testtoken")
            .call()
            .expect("blocking events request should complete");
        let status = response.status();
        let body = response
            .body_mut()
            .read_to_string()
            .expect("blocking response body should be readable");
        (status, body)
    });

    std::thread::sleep(Duration::from_millis(150));

    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut overloaded_res = noerr_agent
        .get(&format!("{url}/v1/health"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("overload request should complete");

    assert_eq!(
        overloaded_res.status(),
        ureq::http::StatusCode::SERVICE_UNAVAILABLE
    );
    let retry_after = overloaded_res
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        retry_after, "1",
        "overloaded response should include Retry-After header"
    );
    let overloaded_body = overloaded_res
        .body_mut()
        .read_to_string()
        .expect("overloaded body should be readable");
    let overloaded_json: Value = serde_json::from_str(&overloaded_body).expect("json body");
    assert_eq!(overloaded_json["ok"], false);
    assert_eq!(overloaded_json["error"], "server_overloaded");

    let (blocking_status, blocking_body) = blocking_handle
        .join()
        .expect("blocking request thread should join");
    assert_eq!(blocking_status, ureq::http::StatusCode::OK);
    let blocking_json: Value = serde_json::from_str(&blocking_body).expect("blocking body json");
    assert_eq!(blocking_json["ok"], true);
    assert_eq!(
        blocking_json["events"].as_array().map(std::vec::Vec::len),
        Some(0)
    );

    let mut metrics_res = noerr_agent
        .get(&format!("{url}/v1/metrics"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("metrics request should complete");
    assert_eq!(metrics_res.status(), ureq::http::StatusCode::OK);
    let metrics_text = metrics_res
        .body_mut()
        .read_to_string()
        .expect("metrics body should be readable");
    let metrics_json: Value = serde_json::from_str(&metrics_text).expect("metrics json");
    let responses_5xx = metrics_json["metrics"]["responses"]["5xx"]
        .as_u64()
        .expect("5xx should be numeric");
    let responses_503 = metrics_json["metrics"]["responses"]["503"]
        .as_u64()
        .expect("503 should be numeric");
    assert!(
        responses_5xx >= 1,
        "metrics should include overloaded response"
    );
    assert!(
        responses_503 >= 1,
        "metrics should include overloaded response in 503 bucket"
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = dap_thread.join();
}
