use std::fs;
use std::net::TcpListener;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use tempfile::tempdir;

fn python_debug_ready() -> bool {
    let python_ok = std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok();
    if !python_ok {
        return false;
    }

    std::process::Command::new("python")
        .arg("-c")
        .arg("import debugpy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

#[test]
fn python_debug_reports_debug_endpoint_and_fallback_port() {
    if !python_debug_ready() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let busy_listener = TcpListener::bind("127.0.0.1:5678").expect("test debug port should bind");

    let mut debug_cmd = cargo_bin_cmd!("launch-code");
    let debug_assert = debug_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("debug")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("5678")
        .arg("--wait-for-client")
        .arg("true")
        .assert()
        .success()
        .stdout(contains("session_id="))
        .stdout(contains("debug_port="))
        .stdout(contains("debug_endpoint="));

    let debug_output =
        String::from_utf8(debug_assert.get_output().stdout.clone()).expect("debug output utf8");
    let session_id = parse_field(&debug_output, "session_id")
        .expect("session id should exist")
        .to_string();
    let debug_port = parse_field(&debug_output, "debug_port")
        .expect("debug port should exist")
        .parse::<u16>()
        .expect("debug port should be a number");
    assert_ne!(debug_port, 5678);

    drop(busy_listener);

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    status_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("status=running"))
        .stdout(contains("requested_debug_port=5678"))
        .stdout(contains("debug_fallback=true"))
        .stdout(contains("debug_endpoint=127.0.0.1:"));

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("stopped"));
}
