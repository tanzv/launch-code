use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
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
