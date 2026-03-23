use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
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
fn start_rejects_non_file_log_mode_without_foreground() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("bg_invalid_mode.py");
    fs::write(&script_path, "print('invalid-mode', flush=True)\n")
        .expect("script should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("invalid-mode")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--log-mode")
        .arg("stdout")
        .output()
        .expect("start should run");

    assert!(
        !output.status.success(),
        "start should fail with invalid options"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid options should return exit code 2"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("invalid start options"),
        "error should explain invalid start options"
    );
}

#[test]
fn start_foreground_stdout_streams_process_output() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("foreground_stdout.py");
    fs::write(
        &script_path,
        "print('foreground-line-1', flush=True)\nprint('foreground-line-2', flush=True)\n",
    )
    .expect("script should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("foreground-stdout")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--foreground")
        .arg("--log-mode")
        .arg("stdout")
        .output()
        .expect("start should run");

    assert!(output.status.success(), "start should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("foreground-line-1"),
        "foreground stdout mode should stream first line"
    );
    assert!(
        stdout.contains("foreground-line-2"),
        "foreground stdout mode should stream second line"
    );
}

#[test]
fn start_tail_follows_background_logs_until_exit() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("tail_follow.py");
    fs::write(
        &script_path,
        "import time\nprint('tail-line-1', flush=True)\ntime.sleep(0.2)\nprint('tail-line-2', flush=True)\n",
    )
    .expect("script should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("tail-follow")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--tail")
        .output()
        .expect("start should run");

    assert!(output.status.success(), "start should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("session_id="),
        "start output should include session id"
    );
    assert!(
        stdout.contains("tail-line-1"),
        "tail mode should include first emitted line"
    );
    assert!(
        stdout.contains("tail-line-2"),
        "tail mode should include second emitted line"
    );
}

#[test]
fn start_default_temporary_logs_are_deleted_after_stop() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("temporary_logs.py");
    fs::write(
        &script_path,
        "import time\nprint('temporary-line', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("temporary-logs")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");

    let start_stdout = String::from_utf8(start_output.stdout).expect("start stdout should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should exist");
    let log_path = tmp
        .path()
        .join(".launch-code")
        .join("logs")
        .join(format!("{session_id}.log"));
    assert!(log_path.exists(), "running session should create log file");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
    assert!(
        !log_path.exists(),
        "temporary log file should be deleted after stop"
    );

    let mut inspect_cmd = cargo_bin_cmd!("launch-code");
    let inspect_output = inspect_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("inspect")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("10")
        .output()
        .expect("inspect should run");
    assert!(inspect_output.status.success(), "inspect should succeed");
    let inspect_stdout =
        String::from_utf8(inspect_output.stdout).expect("inspect stdout should be utf8");
    let inspect_doc: Value =
        serde_json::from_str(&inspect_stdout).expect("inspect output should be json");
    assert_eq!(inspect_doc["log"]["text"], "");
    assert_eq!(inspect_doc["session"]["spec"]["log_retention"], "temporary");
}

#[test]
fn start_persistent_logs_are_retained_after_stop() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("persistent_logs.py");
    fs::write(
        &script_path,
        "import time\nprint('persistent-line', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("persistent-logs")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--log-retention")
        .arg("persistent")
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");

    let start_stdout = String::from_utf8(start_output.stdout).expect("start stdout should be utf8");
    let session_id = parse_session_id(&start_stdout).expect("session id should exist");
    let log_path = tmp
        .path()
        .join(".launch-code")
        .join("logs")
        .join(format!("{session_id}.log"));
    assert!(log_path.exists(), "running session should create log file");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
    assert!(
        log_path.exists(),
        "persistent log file should remain after stop"
    );

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("10")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let logs_stdout = String::from_utf8(logs_output.stdout).expect("logs stdout should be utf8");
    assert!(
        logs_stdout.contains("persistent-line"),
        "persistent log should still be readable after stop"
    );
}
