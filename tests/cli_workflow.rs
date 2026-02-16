use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use serde_json::Value;
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

#[test]
fn cli_start_supports_env_file_and_cli_env_overrides() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("app_env.py");
    let env_file = tmp.path().join(".env");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");
    fs::write(&env_file, "FILE_ONLY=1\nSHARED=from-file\n").expect("env file should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--env-file")
        .arg(env_file.to_string_lossy().to_string())
        .arg("--env")
        .arg("SHARED=from-cli")
        .arg("--env")
        .arg("CLI_ONLY=2")
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");

    let start_stdout = String::from_utf8(start_output.stdout).expect("stdout should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

    let state_path = tmp.path().join(".launch-code").join("state.json");
    let state_payload = fs::read_to_string(state_path).expect("state file should exist");
    let state_doc: Value =
        serde_json::from_str(&state_payload).expect("state should be valid json");
    let env_doc = &state_doc["sessions"][&session_id]["spec"]["env"];
    assert_eq!(env_doc["FILE_ONLY"].as_str(), Some("1"));
    assert_eq!(env_doc["CLI_ONLY"].as_str(), Some("2"));
    assert_eq!(
        env_doc["SHARED"].as_str(),
        Some("from-cli"),
        "explicit --env should override env-file values"
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .assert()
        .success();
}

#[test]
fn cli_manual_restart_increments_restart_count() {
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
        .arg("restart-count-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("stdout should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should be present");

    let mut restart_cmd = cargo_bin_cmd!("launch-code");
    restart_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("restart")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .arg("true")
        .assert()
        .success()
        .stdout(contains("restart_count=1"));

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    status_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("restart_count=1"));

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .assert()
        .success();
}
