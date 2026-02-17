use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
}

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while std::time::Instant::now() < deadline {
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

fn parse_session_id(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id=").map(ToString::to_string))
}

#[cfg(unix)]
#[test]
fn concurrent_restart_requests_keep_session_consistent() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("restart_concurrent.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-restart-concurrent")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should exist");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--workers")
        .arg("2")
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
        .expect("listening url should exist")
        .to_string();

    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let barrier_for_thread = Arc::clone(&barrier);
        let url_for_thread = url.clone();
        let session_id_for_thread = session_id.clone();
        joins.push(thread::spawn(move || {
            let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(8)))
                .http_status_as_error(false)
                .build()
                .into();

            barrier_for_thread.wait();
            let mut res = noerr_agent
                .post(&format!(
                    "{url_for_thread}/v1/sessions/{session_id_for_thread}/restart"
                ))
                .header("Authorization", "Bearer testtoken")
                .send_empty()
                .expect("restart request should complete");
            let status = res.status();
            let body = res
                .body_mut()
                .read_to_string()
                .expect("restart response should be readable");
            (status, body)
        }));
    }

    barrier.wait();
    let responses: Vec<(ureq::http::StatusCode, String)> = joins
        .into_iter()
        .map(|join| join.join().expect("thread should join"))
        .collect();

    assert!(
        responses
            .iter()
            .any(|(status, _)| *status == ureq::http::StatusCode::OK),
        "at least one restart request should succeed"
    );
    assert!(
        responses.iter().all(|(status, _)| {
            *status == ureq::http::StatusCode::OK || *status == ureq::http::StatusCode::CONFLICT
        }),
        "concurrent restart should only return 200 or 409"
    );

    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut session_res = noerr_agent
        .get(&format!("{url}/v1/sessions/{session_id}"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("get session should complete");
    assert_eq!(session_res.status(), ureq::http::StatusCode::OK);
    let session_body = session_res
        .body_mut()
        .read_to_string()
        .expect("session response should be readable");
    let session_doc: Value =
        serde_json::from_str(&session_body).expect("session response should be valid json");
    assert_eq!(session_doc["ok"], true);
    assert_eq!(session_doc["session"]["status"], "running");
    let restart_count = session_doc["session"]["restart_count"]
        .as_u64()
        .expect("restart count should be numeric");
    assert!(
        (1..=2).contains(&restart_count),
        "restart count should remain within expected concurrent bounds"
    );

    let force_payload = serde_json::json!({
        "force": true,
        "grace_timeout_ms": 100
    });
    let mut stop_res = noerr_agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&force_payload).expect("payload should serialize"))
        .expect("forced stop should complete");
    assert_eq!(stop_res.status(), ureq::http::StatusCode::OK);
    let _ = stop_res
        .body_mut()
        .read_to_string()
        .expect("stop response should be readable");

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn concurrent_stop_requests_are_idempotent() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("stop_concurrent.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-stop-concurrent")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should exist");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--workers")
        .arg("2")
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
        .expect("listening url should exist")
        .to_string();

    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let barrier_for_thread = Arc::clone(&barrier);
        let url_for_thread = url.clone();
        let session_id_for_thread = session_id.clone();
        joins.push(thread::spawn(move || {
            let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(8)))
                .http_status_as_error(false)
                .build()
                .into();
            let payload = serde_json::json!({
                "force": true,
                "grace_timeout_ms": 100
            });

            barrier_for_thread.wait();
            let mut res = noerr_agent
                .post(&format!(
                    "{url_for_thread}/v1/sessions/{session_id_for_thread}/stop"
                ))
                .header("Authorization", "Bearer testtoken")
                .send(serde_json::to_string(&payload).expect("payload should serialize"))
                .expect("stop request should complete");
            let status = res.status();
            let body = res
                .body_mut()
                .read_to_string()
                .expect("stop response should be readable");
            (status, body)
        }));
    }

    barrier.wait();
    let responses: Vec<(ureq::http::StatusCode, String)> = joins
        .into_iter()
        .map(|join| join.join().expect("thread should join"))
        .collect();
    assert!(
        responses
            .iter()
            .all(|(status, _)| *status == ureq::http::StatusCode::OK),
        "concurrent forced stop requests should both succeed"
    );

    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut session_res = noerr_agent
        .get(&format!("{url}/v1/sessions/{session_id}"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("get session should complete");
    assert_eq!(session_res.status(), ureq::http::StatusCode::OK);
    let session_body = session_res
        .body_mut()
        .read_to_string()
        .expect("session response should be readable");
    let session_doc: Value =
        serde_json::from_str(&session_body).expect("session response should be valid json");
    assert_eq!(session_doc["ok"], true);
    assert_eq!(session_doc["session"]["status"], "stopped");
    assert_eq!(session_doc["session"]["pid"], Value::Null);

    let _ = child.kill();
    let _ = child.wait();
}
