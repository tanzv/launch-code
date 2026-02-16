use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin_cmd;
use launch_code::process::is_process_alive;
use predicates::str::contains;
use tempfile::tempdir;

fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
}

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

fn wait_for_process_exit(pid: u32, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_process_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("managed worker did not exit before timeout");
}

#[test]
fn managed_session_restarts_when_worker_exits() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("one_shot.py");
    fs::write(&script_path, "import time\ntime.sleep(1)\n").expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_assert = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("managed-python")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--managed")
        .assert()
        .success()
        .stdout(contains("session_id="))
        .stdout(contains("pid="));

    let start_output = String::from_utf8(start_assert.get_output().stdout.clone())
        .expect("start output should be utf8");
    let session_id = parse_field(&start_output, "session_id")
        .expect("session id should be present")
        .to_string();
    let pid: u32 = parse_field(&start_output, "pid")
        .expect("pid should be present")
        .parse()
        .expect("pid should parse");

    wait_for_process_exit(pid, Duration::from_secs(8));

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    status_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("status=running"))
        .stdout(contains("restart_count=1"));

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
