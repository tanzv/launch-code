use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use tempfile::tempdir;

fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
}

fn parse_session_id(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id=").map(ToString::to_string))
}

#[test]
fn cli_start_status_stop_workflow() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_assert = start_cmd
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
        .assert()
        .success()
        .stdout(contains("session_id="));

    let start_output = String::from_utf8(start_assert.get_output().stdout.clone())
        .expect("start output should be utf8");
    let session_id = parse_session_id(&start_output).expect("session id should be present");

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    status_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("running"));

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
