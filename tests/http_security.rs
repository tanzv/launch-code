use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

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

#[test]
fn serve_rejects_unauthorized_requests() {
    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
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
        .http_status_as_error(false)
        .build()
        .into();

    let mut res = agent
        .get(&format!("{url}/v1/health"))
        .call()
        .expect("request should complete");
    assert_eq!(res.status(), ureq::http::StatusCode::UNAUTHORIZED);
    let body = res
        .body_mut()
        .read_to_string()
        .expect("body should be readable");
    let doc: Value = serde_json::from_str(&body).expect("body should be json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "unauthorized");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_rejects_oversized_json_payloads() {
    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
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

    let oversized_body = "x".repeat(1_100_000);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut res = agent
        .post(&format!("{url}/v1/sessions/session-1/debug/dap/request"))
        .header("Authorization", "Bearer testtoken")
        .send(oversized_body)
        .expect("request should complete");

    assert_eq!(res.status(), ureq::http::StatusCode::PAYLOAD_TOO_LARGE);
    let body = res
        .body_mut()
        .read_to_string()
        .expect("body should be readable");
    let doc: Value = serde_json::from_str(&body).expect("body should be json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "payload_too_large");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_requires_token_source() {
    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let output = Command::new(exe)
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .output()
        .expect("serve should execute");

    assert!(!output.status.success(), "serve should fail without token");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("missing serve token"),
        "stderr should explain token requirement"
    );
}

#[test]
fn serve_accepts_token_from_env_when_flag_omitted() {
    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .env("LAUNCH_CODE_HTTP_TOKEN", "envtoken")
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

    let mut response = agent
        .get(&format!("{url}/v1/health"))
        .header("Authorization", "Bearer envtoken")
        .call()
        .expect("health request should succeed");
    assert_eq!(response.status(), ureq::http::StatusCode::OK);
    let body = response
        .body_mut()
        .read_to_string()
        .expect("body should be readable");
    assert!(body.contains("\"ok\":true"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_accepts_token_from_file_when_flag_omitted() {
    let tmp = tempfile::tempdir().expect("temp dir should exist");
    let token_file = tmp.path().join("token.txt");
    let mut file = std::fs::File::create(&token_file).expect("token file should be created");
    writeln!(file, "filetoken").expect("token file should be written");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token-file")
        .arg(token_file.to_string_lossy().to_string())
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

    let mut response = agent
        .get(&format!("{url}/v1/health"))
        .header("Authorization", "Bearer filetoken")
        .call()
        .expect("health request should succeed");
    assert_eq!(response.status(), ureq::http::StatusCode::OK);
    let body = response
        .body_mut()
        .read_to_string()
        .expect("body should be readable");
    assert!(body.contains("\"ok\":true"));

    let _ = child.kill();
    let _ = child.wait();
}
