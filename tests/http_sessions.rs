use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
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
            .filter(|v| *v > 0)
            .is_some()
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

fn parse_session_id(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id=").map(ToString::to_string))
}

#[test]
fn serve_can_list_sessions_and_stop_via_http() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");

    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

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

    let mut list_res = agent
        .get(&format!("{url}/v1/sessions"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("list sessions should succeed");
    assert_eq!(list_res.status(), ureq::http::StatusCode::OK);
    let list_body = list_res
        .body_mut()
        .read_to_string()
        .expect("list response body should be readable");
    let list_json: Value = serde_json::from_str(&list_body).expect("list response json");
    assert_eq!(list_json["ok"], true);
    let sessions = list_json["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert!(
        sessions.iter().any(|item| item["id"] == session_id),
        "session should be present in list"
    );

    let mut stop_res = agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send_empty()
        .expect("stop should succeed");
    assert_eq!(stop_res.status(), ureq::http::StatusCode::OK);
    let stop_body = stop_res
        .body_mut()
        .read_to_string()
        .expect("stop response body should be readable");
    let stop_json: Value = serde_json::from_str(&stop_body).expect("stop response json");
    assert_eq!(stop_json["ok"], true);
    assert_eq!(stop_json["session"]["status"], "stopped");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_restart_accepts_options_and_validates_payload() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app_restart.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-restart-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

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
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build()
        .into();

    let restart_payload = serde_json::json!({
        "force": true,
        "grace_timeout_ms": 250
    });
    let mut restart_res = agent
        .post(&format!("{url}/v1/sessions/{session_id}/restart"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&restart_payload).expect("payload should serialize"))
        .expect("restart should succeed");
    assert_eq!(restart_res.status(), ureq::http::StatusCode::OK);
    let restart_body = restart_res
        .body_mut()
        .read_to_string()
        .expect("restart body should be readable");
    let restart_json: Value = serde_json::from_str(&restart_body).expect("restart json");
    assert_eq!(restart_json["ok"], true);
    assert_eq!(restart_json["session"]["status"], "running");
    assert_eq!(restart_json["session"]["restart_count"].as_u64(), Some(1));

    let bad_payload = serde_json::json!({"force": "bad"});
    let mut bad_res = noerr_agent
        .post(&format!("{url}/v1/sessions/{session_id}/restart"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&bad_payload).expect("payload should serialize"))
        .expect("bad request should complete");
    assert_eq!(bad_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_body = bad_res
        .body_mut()
        .read_to_string()
        .expect("bad response body should be readable");
    let bad_json: Value = serde_json::from_str(&bad_body).expect("bad response json");
    assert_eq!(bad_json["ok"], false);
    assert_eq!(bad_json["error"], "bad_request");

    let mut stop_res = agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send_empty()
        .expect("stop should succeed");
    assert_eq!(stop_res.status(), ureq::http::StatusCode::OK);
    let _ = stop_res
        .body_mut()
        .read_to_string()
        .expect("stop response body should be readable");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_stop_accepts_options_and_validates_payload() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app_stop.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-stop-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

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
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build()
        .into();

    let bad_payload = serde_json::json!({"grace_timeout_ms": "oops"});
    let mut bad_res = noerr_agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&bad_payload).expect("payload should serialize"))
        .expect("bad request should complete");
    assert_eq!(bad_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_body = bad_res
        .body_mut()
        .read_to_string()
        .expect("bad response body should be readable");
    let bad_json: Value = serde_json::from_str(&bad_body).expect("bad response json");
    assert_eq!(bad_json["ok"], false);
    assert_eq!(bad_json["error"], "bad_request");

    let stop_payload = serde_json::json!({
        "force": true,
        "grace_timeout_ms": 300
    });
    let mut stop_res = agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&stop_payload).expect("payload should serialize"))
        .expect("stop should succeed");
    assert_eq!(stop_res.status(), ureq::http::StatusCode::OK);
    let stop_body = stop_res
        .body_mut()
        .read_to_string()
        .expect("stop response body should be readable");
    let stop_json: Value = serde_json::from_str(&stop_body).expect("stop response json");
    assert_eq!(stop_json["ok"], true);
    assert_eq!(stop_json["session"]["status"], "stopped");

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn serve_stop_timeout_returns_conflict_error() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("ignore_term.py");
    fs::write(
        &script_path,
        "import signal\nimport time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('ready', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("python-stop-timeout")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

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

    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build()
        .into();
    let timeout_payload = serde_json::json!({
        "force": false,
        "grace_timeout_ms": 0
    });
    let mut timeout_res = noerr_agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&timeout_payload).expect("payload should serialize"))
        .expect("stop timeout request should complete");
    assert_eq!(timeout_res.status(), ureq::http::StatusCode::CONFLICT);
    let timeout_body = timeout_res
        .body_mut()
        .read_to_string()
        .expect("timeout response body should be readable");
    let timeout_json: Value = serde_json::from_str(&timeout_body).expect("timeout response json");
    assert_eq!(timeout_json["ok"], false);
    assert_eq!(timeout_json["error"], "stop_timeout");

    let mut metrics_res = noerr_agent
        .get(&format!("{url}/v1/metrics"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("metrics request should complete");
    assert_eq!(metrics_res.status(), ureq::http::StatusCode::OK);
    let metrics_body = metrics_res
        .body_mut()
        .read_to_string()
        .expect("metrics body should be readable");
    let metrics_json: Value = serde_json::from_str(&metrics_body).expect("metrics body json");
    let responses_409 = metrics_json["metrics"]["responses"]["409"]
        .as_u64()
        .expect("409 bucket should be numeric");
    assert!(
        responses_409 >= 1,
        "metrics should record stop timeout conflict responses"
    );

    let force_payload = serde_json::json!({
        "force": true,
        "grace_timeout_ms": 100
    });
    let mut force_res = noerr_agent
        .post(&format!("{url}/v1/sessions/{session_id}/stop"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&force_payload).expect("payload should serialize"))
        .expect("forced stop should complete");
    assert_eq!(force_res.status(), ureq::http::StatusCode::OK);
    let _ = force_res
        .body_mut()
        .read_to_string()
        .expect("force stop response body should be readable");

    let _ = child.kill();
    let _ = child.wait();
}
