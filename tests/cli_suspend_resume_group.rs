#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use assert_cmd::cargo::cargo_bin_cmd;
#[cfg(unix)]
use tempfile::tempdir;

#[cfg(unix)]
fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
}

#[cfg(unix)]
fn parse_session_id(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id=").map(ToString::to_string))
}

#[cfg(unix)]
fn parse_pid_from_status(output: &str) -> Option<u32> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("pid="))
        .and_then(|value| value.parse::<u32>().ok())
}

#[cfg(unix)]
fn wait_child_pid_file(path: &std::path::Path) -> u32 {
    for _ in 0..40 {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                return pid;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("child pid file should be created");
}

#[cfg(unix)]
fn process_is_stopped(pid: u32) -> Option<bool> {
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let state = String::from_utf8(output.stdout).ok()?;
    let state = state.trim();
    if state.is_empty() {
        return None;
    }
    Some(state.contains('T'))
}

#[cfg(unix)]
fn wait_for_stopped_state(pid: u32, expected_stopped: bool) -> bool {
    for _ in 0..40 {
        if let Some(is_stopped) = process_is_stopped(pid) {
            if is_stopped == expected_stopped {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
#[test]
fn cli_suspend_resume_controls_process_group_children() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("spawn_child.py");
    let child_pid_path = tmp.path().join("child.pid");
    let script = [
        "import subprocess",
        "import time",
        "child = subprocess.Popen(['sleep', '30'])",
        "with open('child.pid', 'w', encoding='utf-8') as handle:",
        "    handle.write(str(child.pid))",
        "time.sleep(30)",
        "",
    ]
    .join("\n");
    fs::write(&script_path, script).expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("group-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id =
        parse_session_id(&String::from_utf8(start_output.stdout).expect("start stdout utf8"))
            .expect("session id should exist");

    let child_pid = wait_child_pid_file(&child_pid_path);

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    let status_output = status_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("status should run");
    assert!(status_output.status.success(), "status should succeed");
    let parent_pid =
        parse_pid_from_status(&String::from_utf8(status_output.stdout).expect("status utf8"))
            .expect("parent pid should be present");

    let mut suspend_cmd = cargo_bin_cmd!("launch-code");
    let suspend_output = suspend_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("suspend")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("suspend should run");
    assert!(suspend_output.status.success(), "suspend should succeed");
    assert!(
        wait_for_stopped_state(parent_pid, true),
        "parent process should become stopped"
    );
    assert!(
        wait_for_stopped_state(child_pid, true),
        "child process should become stopped"
    );

    let mut resume_cmd = cargo_bin_cmd!("launch-code");
    let resume_output = resume_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("resume")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("resume should run");
    assert!(resume_output.status.success(), "resume should succeed");
    assert!(
        wait_for_stopped_state(parent_pid, false),
        "parent process should resume"
    );
    assert!(
        wait_for_stopped_state(child_pid, false),
        "child process should resume"
    );

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
}
