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
fn restart_respects_force_and_grace_timeout_options() {
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
        .arg("restart-ignore-term")
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
    let session_id = parse_field(&start_stdout, "session_id")
        .expect("session id should exist")
        .to_string();
    let original_pid: u32 = parse_field(&start_stdout, "pid")
        .expect("pid should exist")
        .parse()
        .expect("pid should parse");
    assert!(
        is_process_alive(original_pid),
        "original process should be alive before restart"
    );
    thread::sleep(Duration::from_millis(300));

    let mut restart_without_force_cmd = cargo_bin_cmd!("launch-code");
    let restart_without_force_output = restart_without_force_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("restart")
        .arg("--id")
        .arg(&session_id)
        .arg("--no-force")
        .arg("--grace-timeout-ms")
        .arg("100")
        .output()
        .expect("restart without force should run");

    assert!(
        !restart_without_force_output.status.success(),
        "restart without force should fail on timeout"
    );
    let restart_without_force_stderr =
        String::from_utf8(restart_without_force_output.stderr).expect("stderr should be utf8");
    assert!(
        restart_without_force_stderr.contains("timed out"),
        "restart timeout should be reported"
    );
    assert!(
        is_process_alive(original_pid),
        "original process should remain alive after failed restart"
    );

    let mut restart_force_cmd = cargo_bin_cmd!("launch-code");
    let restart_force_output = restart_force_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("restart")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .arg("true")
        .arg("--grace-timeout-ms")
        .arg("100")
        .output()
        .expect("restart with force should run");
    assert!(
        restart_force_output.status.success(),
        "restart with force should succeed"
    );

    let restart_stdout =
        String::from_utf8(restart_force_output.stdout).expect("restart stdout should be utf8");
    let restarted_pid: u32 = parse_field(&restart_stdout, "pid")
        .expect("restarted pid should exist")
        .parse()
        .expect("restarted pid should parse");
    assert!(
        is_process_alive(restarted_pid),
        "restarted process should be alive"
    );
    wait_for_process_exit(original_pid, Duration::from_secs(3));

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "cleanup stop should succeed");
    wait_for_process_exit(restarted_pid, Duration::from_secs(3));
}
