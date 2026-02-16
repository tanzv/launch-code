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
fn inspect_outputs_session_and_process_details_as_json() {
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
        .arg("inspect-demo")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .assert()
        .success()
        .stdout(contains("session_id="));

    let start_output =
        String::from_utf8(start_assert.get_output().stdout.clone()).expect("start output utf8");
    let session_id = parse_session_id(&start_output).expect("session id should exist");

    let mut inspect_cmd = cargo_bin_cmd!("launch-code");
    let inspect_assert = inspect_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("inspect")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("0")
        .assert()
        .success();

    let payload =
        String::from_utf8(inspect_assert.get_output().stdout.clone()).expect("inspect output utf8");
    let doc: Value = serde_json::from_str(&payload).expect("inspect output should be json");

    assert_eq!(doc["session"]["id"].as_str().unwrap(), session_id);
    assert!(doc["process"]["alive"].as_bool().unwrap());
    assert!(doc["process"]["command"].is_array());

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(session_id)
        .assert()
        .success()
        .stdout(contains("stopped"));
}
