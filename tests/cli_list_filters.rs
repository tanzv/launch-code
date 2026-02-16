use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
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
fn list_supports_status_runtime_and_name_filters() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let run_script = tmp.path().join("run.py");
    let stop_script = tmp.path().join("stop.py");
    fs::write(&run_script, "import time\ntime.sleep(30)\n").expect("script should be written");
    fs::write(&stop_script, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_run_cmd = cargo_bin_cmd!("launch-code");
    let start_run_output = start_run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("api-running")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(run_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_run_output.status.success(), "start should succeed");
    let running_id = parse_session_id(
        &String::from_utf8(start_run_output.stdout).expect("start stdout should be utf8"),
    )
    .expect("running session id should exist");

    let mut start_stop_cmd = cargo_bin_cmd!("launch-code");
    let start_stop_output = start_stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("worker-stopped")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(stop_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_stop_output.status.success(), "start should succeed");
    let stopped_id = parse_session_id(
        &String::from_utf8(start_stop_output.stdout).expect("start stdout should be utf8"),
    )
    .expect("stopped session id should exist");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&stopped_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");

    let mut list_running_cmd = cargo_bin_cmd!("launch-code");
    let list_running_output = list_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--status")
        .arg("running")
        .output()
        .expect("list should run");
    assert!(list_running_output.status.success(), "list should succeed");
    let list_running_text =
        String::from_utf8(list_running_output.stdout).expect("list output should be utf8");
    assert!(
        list_running_text.contains(&running_id),
        "running filter should include running session"
    );
    assert!(
        !list_running_text.contains(&stopped_id),
        "running filter should exclude stopped session"
    );

    let mut list_stopped_cmd = cargo_bin_cmd!("launch-code");
    let list_stopped_output = list_stopped_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("list should run");
    assert!(list_stopped_output.status.success(), "list should succeed");
    let list_stopped_text =
        String::from_utf8(list_stopped_output.stdout).expect("list output should be utf8");
    assert!(
        list_stopped_text.contains(&stopped_id),
        "stopped filter should include stopped session"
    );
    assert!(
        !list_stopped_text.contains(&running_id),
        "stopped filter should exclude running session"
    );

    let mut list_name_cmd = cargo_bin_cmd!("launch-code");
    let list_name_output = list_name_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--name-contains")
        .arg("worker")
        .output()
        .expect("list should run");
    assert!(list_name_output.status.success(), "list should succeed");
    let list_name_text = String::from_utf8(list_name_output.stdout).expect("list output utf8");
    assert!(
        list_name_text.contains(&stopped_id),
        "name filter should include matching session"
    );
    assert!(
        !list_name_text.contains(&running_id),
        "name filter should exclude non-matching session"
    );

    let mut list_runtime_cmd = cargo_bin_cmd!("launch-code");
    let list_runtime_output = list_runtime_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--runtime")
        .arg("python")
        .output()
        .expect("list should run");
    assert!(list_runtime_output.status.success(), "list should succeed");
    let list_runtime_text =
        String::from_utf8(list_runtime_output.stdout).expect("list output should be utf8");
    assert!(
        list_runtime_text.contains(&running_id),
        "runtime filter should include running python session"
    );
    assert!(
        list_runtime_text.contains(&stopped_id),
        "runtime filter should include stopped python session"
    );

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&running_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(
        cleanup_output.status.success(),
        "cleanup stop should succeed"
    );
}
