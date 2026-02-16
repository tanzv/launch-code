use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin_cmd;
use launch_code::process::is_process_alive;
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
    panic!("process did not exit before timeout");
}

#[cfg(unix)]
#[test]
fn stop_can_fail_on_timeout_then_force_kill() {
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
        .arg("ignore-term")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");

    let start_stdout = String::from_utf8(start_output.stdout).expect("start stdout utf8");
    let session_id = parse_field(&start_stdout, "session_id")
        .expect("session id should exist")
        .to_string();
    let pid: u32 = parse_field(&start_stdout, "pid")
        .expect("pid should exist")
        .parse()
        .expect("pid should parse");
    assert!(is_process_alive(pid), "process should be alive before stop");
    thread::sleep(Duration::from_millis(300));

    let mut graceful_stop = cargo_bin_cmd!("launch-code");
    let graceful_output = graceful_stop
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--grace-timeout-ms")
        .arg("100")
        .output()
        .expect("stop should run");
    assert!(
        !graceful_output.status.success(),
        "graceful stop should fail when process ignores SIGTERM"
    );
    let stderr = String::from_utf8(graceful_output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("timed out"),
        "stop timeout error should be shown"
    );
    assert!(
        is_process_alive(pid),
        "process should still be alive after graceful timeout"
    );

    let mut force_stop = cargo_bin_cmd!("launch-code");
    let force_output = force_stop
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .arg("--grace-timeout-ms")
        .arg("100")
        .output()
        .expect("forced stop should run");
    assert!(force_output.status.success(), "forced stop should succeed");
    wait_for_process_exit(pid, Duration::from_secs(3));
}
