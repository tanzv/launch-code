use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

fn python_available() -> bool {
    Command::new("python").arg("--version").output().is_ok()
}

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

fn parse_session_id(output: &str) -> Option<String> {
    parse_field(output, "session_id").map(ToString::to_string)
}

fn build_session(id: &str, name: &str, status: &str) -> Value {
    json!({
        "id": id,
        "spec": {
            "name": name,
            "runtime": "python",
            "entry": "app.py",
            "args": [],
            "cwd": ".",
            "env": {},
            "managed": false,
            "mode": "run",
            "debug": null,
            "prelaunch_task": null,
            "poststop_task": null
        },
        "status": status,
        "pid": null,
        "supervisor_pid": null,
        "log_path": null,
        "debug_meta": null,
        "created_at": 1,
        "updated_at": 1,
        "last_exit_code": null,
        "restart_count": 0
    })
}

fn write_state(root: &Path, sessions: Value) {
    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": sessions,
        "project_info": null
    });
    fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&state).expect("state json"),
    )
    .expect("state should be written");
}

fn read_session_count(root: &Path) -> usize {
    let state_payload =
        fs::read_to_string(root.join(".launch-code").join("state.json")).expect("state exists");
    let state_doc: Value = serde_json::from_str(&state_payload).expect("state should be json");
    state_doc["sessions"]
        .as_object()
        .expect("sessions should be object")
        .len()
}

fn add_link(home_root: &Path, name: &str, workspace_path: &Path) {
    let mut add_cmd = cargo_bin_cmd!("launch-code");
    let add_output = add_cmd
        .env("HOME", home_root)
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg(name)
        .arg("--path")
        .arg(workspace_path.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(add_output.status.success(), "link add should succeed");
}

#[test]
fn cleanup_dry_run_reports_matches_without_removing_sessions() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "stopped-a": build_session("stopped-a", "a", "stopped"),
            "stopped-b": build_session("stopped-b", "b", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("cleanup")
        .arg("--dry-run")
        .output()
        .expect("cleanup dry-run should run");
    assert!(output.status.success(), "cleanup dry-run should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("matched=2"),
        "dry-run output should report matched sessions"
    );
    assert!(
        stdout.contains("removed=0"),
        "dry-run output should not remove sessions"
    );

    let state_payload = fs::read_to_string(tmp.path().join(".launch-code").join("state.json"))
        .expect("state file should exist");
    let state_doc: Value = serde_json::from_str(&state_payload).expect("state should be json");
    let sessions = state_doc["sessions"]
        .as_object()
        .expect("sessions should be object");
    assert_eq!(sessions.len(), 2, "dry-run should keep all sessions");
}

#[test]
fn cleanup_removes_stopped_sessions_from_state() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "stopped-a": build_session("stopped-a", "a", "stopped"),
            "stopped-b": build_session("stopped-b", "b", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("cleanup")
        .output()
        .expect("cleanup should run");
    assert!(output.status.success(), "cleanup should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("removed=2"),
        "cleanup should remove both stopped sessions"
    );

    let state_payload = fs::read_to_string(tmp.path().join(".launch-code").join("state.json"))
        .expect("state file should exist");
    let state_doc: Value = serde_json::from_str(&state_payload).expect("state should be json");
    let sessions = state_doc["sessions"]
        .as_object()
        .expect("sessions should be object");
    assert_eq!(sessions.len(), 0, "cleanup should remove stopped sessions");
}

#[test]
fn cleanup_keeps_running_sessions_by_default() {
    if !python_available() {
        eprintln!("python is unavailable; skipping cleanup running-session test");
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script = tmp.path().join("sleep.py");
    fs::write(&script, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_running_cmd = cargo_bin_cmd!("launch-code");
    let start_running_output = start_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("cleanup-running")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("running start should run");
    assert!(
        start_running_output.status.success(),
        "running start should succeed"
    );
    let running_id =
        parse_session_id(&String::from_utf8(start_running_output.stdout).expect("stdout utf8"))
            .expect("running session id should exist");

    let mut start_stopped_cmd = cargo_bin_cmd!("launch-code");
    let start_stopped_output = start_stopped_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("cleanup-stopped")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("stopped start should run");
    assert!(
        start_stopped_output.status.success(),
        "stopped start should succeed"
    );
    let stopped_id =
        parse_session_id(&String::from_utf8(start_stopped_output.stdout).expect("stdout utf8"))
            .expect("stopped session id should exist");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&stopped_id)
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("cleanup")
        .output()
        .expect("cleanup should run");
    assert!(cleanup_output.status.success(), "cleanup should succeed");
    let cleanup_stdout = String::from_utf8(cleanup_output.stdout).expect("stdout should be utf8");
    assert!(
        cleanup_stdout.contains("removed=1"),
        "cleanup should remove stopped session"
    );

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .output()
        .expect("list should run");
    assert!(list_output.status.success(), "list should succeed");
    let list_stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    assert!(
        list_stdout.contains(&running_id),
        "cleanup should keep running session"
    );
    assert!(
        !list_stdout.contains(&stopped_id),
        "cleanup should remove stopped session record"
    );

    let mut stop_running_cmd = cargo_bin_cmd!("launch-code");
    let stop_running_output = stop_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&running_id)
        .output()
        .expect("running stop should run");
    assert!(
        stop_running_output.status.success(),
        "running stop should succeed"
    );
}

#[test]
fn cleanup_defaults_to_global_scope_across_registered_links() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "stopped-a": build_session("stopped-a", "a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "stopped-b": build_session("stopped-b", "b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--json")
        .arg("cleanup")
        .output()
        .expect("global cleanup should run");
    assert!(
        cleanup_output.status.success(),
        "global cleanup should succeed"
    );

    let stdout = String::from_utf8(cleanup_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["scope"], "global");
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["removed_count"], 2);
    assert_eq!(doc["link_count"], 2);

    assert_eq!(read_session_count(&workspace_a), 0);
    assert_eq!(read_session_count(&workspace_b), 0);
}

#[test]
fn cleanup_with_local_flag_limits_scope_to_current_workspace() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "stopped-a": build_session("stopped-a", "a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "stopped-b": build_session("stopped-b", "b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--local")
        .arg("cleanup")
        .output()
        .expect("local cleanup should run");
    assert!(
        cleanup_output.status.success(),
        "local cleanup should succeed"
    );

    let stdout = String::from_utf8(cleanup_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("removed=1"),
        "local cleanup should only remove one workspace record"
    );

    assert_eq!(read_session_count(&workspace_a), 0);
    assert_eq!(
        read_session_count(&workspace_b),
        1,
        "local cleanup should not touch another linked workspace"
    );
}
