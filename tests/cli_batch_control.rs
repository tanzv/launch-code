use std::fs;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

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

fn add_link(home_root: &Path, name: &str, workspace_path: &Path) {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root)
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg(name)
        .arg("--path")
        .arg(workspace_path.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(output.status.success(), "link add should succeed");
}

fn read_session_status(root: &Path, session_id: &str) -> String {
    let payload =
        fs::read_to_string(root.join(".launch-code").join("state.json")).expect("state exists");
    let doc: Value = serde_json::from_str(&payload).expect("state json");
    doc["sessions"][session_id]["status"]
        .as_str()
        .expect("status should exist")
        .to_string()
}

#[test]
fn stop_all_global_scope_stops_matched_sessions_across_links() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--json")
        .arg("stop")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .arg("--yes")
        .output()
        .expect("stop all should run");
    assert!(output.status.success(), "stop all should succeed");

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "stop");
    assert_eq!(doc["scope"], "global");
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["processed_count"], 2);
    assert_eq!(doc["success_count"], 2);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);

    assert_eq!(read_session_status(&workspace_a, "session-a"), "stopped");
    assert_eq!(read_session_status(&workspace_b, "session-b"), "stopped");
}

#[test]
fn stop_all_global_scope_requires_yes_without_dry_run() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--json")
        .arg("stop")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("stop all should run");
    assert!(
        !output.status.success(),
        "global stop all without --yes should fail"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "confirmation_required");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("--yes")),
        "error should explain --yes confirmation"
    );

    assert_eq!(read_session_status(&workspace_a, "session-a"), "stopped");
    assert_eq!(read_session_status(&workspace_b, "session-b"), "stopped");
}

#[test]
fn stop_all_with_local_flag_limits_scope_to_current_workspace() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--json")
        .arg("--local")
        .arg("stop")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("local stop all should run");
    assert!(output.status.success(), "local stop all should succeed");

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "stop");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);

    assert_eq!(read_session_status(&workspace_a, "session-a"), "stopped");
    assert_eq!(
        read_session_status(&workspace_b, "session-b"),
        "stopped",
        "local stop all should not modify another linked workspace"
    );
}

#[test]
fn restart_all_dry_run_matches_running_sessions_by_default() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-running": build_session("session-running", "api-running", "stopped"),
            "session-stopped": build_session("session-stopped", "api-stopped", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("restart")
        .arg("--all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .arg("--name-contains")
        .arg("running")
        .output()
        .expect("restart all dry-run should run");
    assert!(
        output.status.success(),
        "restart all dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "restart");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);
    let items = doc["items"].as_array().expect("items should be array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "session-running");

    assert_eq!(
        read_session_status(tmp.path(), "session-running"),
        "stopped",
        "dry-run should not modify session status"
    );
    assert_eq!(
        read_session_status(tmp.path(), "session-stopped"),
        "stopped",
        "dry-run should not modify stopped session"
    );
}

#[test]
fn suspend_all_dry_run_supports_status_and_name_filters() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-running": build_session("session-running", "api-running", "running"),
            "session-suspended": build_session("session-suspended", "api-suspended", "suspended")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("suspend")
        .arg("--all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .arg("--name-contains")
        .arg("running")
        .output()
        .expect("suspend all dry-run should run");
    assert!(
        output.status.success(),
        "suspend all dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "suspend");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);
    let items = doc["items"].as_array().expect("items should be array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "session-running");
    assert_eq!(items[0]["status_before"], "stopped");
}

#[test]
fn resume_all_dry_run_supports_status_and_name_filters() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-running": build_session("session-running", "api-running", "running"),
            "session-suspended": build_session("session-suspended", "api-suspended", "suspended")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("--all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .arg("--name-contains")
        .arg("suspended")
        .output()
        .expect("resume all dry-run should run");
    assert!(output.status.success(), "resume all dry-run should succeed");

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "resume");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);
    let items = doc["items"].as_array().expect("items should be array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "session-suspended");
    assert_eq!(items[0]["status_before"], "stopped");
}

#[test]
fn suspend_all_global_scope_dry_run_matches_across_links() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    write_state(
        &workspace_a,
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );
    write_state(
        &workspace_b,
        json!({
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );
    add_link(home_root.path(), "workspace-a", &workspace_a);
    add_link(home_root.path(), "workspace-b", &workspace_b);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--json")
        .arg("suspend")
        .arg("--all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("suspend all dry-run should run");
    assert!(
        output.status.success(),
        "suspend all global dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "suspend");
    assert_eq!(doc["scope"], "global");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["processed_count"], 2);
    assert_eq!(doc["success_count"], 2);
    assert_eq!(doc["failed_count"], 0);
    assert_eq!(doc["link_error_count"], 0);
}

#[test]
fn resume_all_defaults_to_continue_on_error_with_unlimited_failures() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped"),
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("resume all should run");
    assert!(output.status.success(), "resume all should complete");

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "resume");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["continue_on_error"], true);
    assert_eq!(doc["max_failures"], 0);
    assert_eq!(doc["stopped_early"], false);
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["processed_count"], 2);
    assert_eq!(doc["success_count"], 0);
    assert_eq!(doc["failed_count"], 2);
    assert_eq!(doc["link_error_count"], 0);
}

#[test]
fn resume_all_stops_early_when_max_failures_reached() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped"),
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .arg("--max-failures")
        .arg("1")
        .output()
        .expect("resume all with max failures should run");
    assert!(
        output.status.success(),
        "resume all with max failures should complete"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "resume");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["continue_on_error"], true);
    assert_eq!(doc["max_failures"], 1);
    assert_eq!(doc["stopped_early"], true);
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 0);
    assert_eq!(doc["failed_count"], 1);
    assert_eq!(doc["link_error_count"], 0);
}

#[test]
fn resume_all_supports_explicit_fail_fast_via_continue_on_error_false() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped"),
            "session-b": build_session("session-b", "api-b", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("--all")
        .arg("--status")
        .arg("stopped")
        .arg("--continue-on-error")
        .arg("false")
        .output()
        .expect("resume all fail-fast should run");
    assert!(
        output.status.success(),
        "resume all fail-fast should complete"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "resume");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["continue_on_error"], false);
    assert_eq!(doc["stopped_early"], true);
    assert_eq!(doc["matched_count"], 2);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["failed_count"], 1);
    assert_eq!(doc["link_error_count"], 0);
}

#[test]
fn stop_all_global_dry_run_tolerates_broken_link_state_and_reports_link_error() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_ok = home_root.path().join("workspace-ok");
    let workspace_bad = home_root.path().join("workspace-bad");
    fs::create_dir_all(&workspace_ok).expect("workspace ok should exist");
    fs::create_dir_all(&workspace_bad).expect("workspace bad should exist");

    write_state(
        &workspace_ok,
        json!({
            "session-ok": build_session("session-ok", "api-ok", "stopped")
        }),
    );
    let bad_state_dir = workspace_bad.join(".launch-code");
    fs::create_dir_all(&bad_state_dir).expect("bad state dir should exist");
    fs::write(bad_state_dir.join("state.json"), "{bad json").expect("bad state should be written");

    add_link(home_root.path(), "workspace-ok", &workspace_ok);
    add_link(home_root.path(), "workspace-bad", &workspace_bad);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_ok)
        .arg("--json")
        .arg("stop")
        .arg("--all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("global stop all dry-run should run");
    assert!(
        output.status.success(),
        "broken link state should not fail global batch dry-run"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["scope"], "global");
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 1);
    assert_eq!(doc["session_failed_count"], 0);
    assert_eq!(doc["link_error_count"], 1);
    let link_errors = doc["link_errors"]
        .as_array()
        .expect("link_errors should be an array");
    assert_eq!(link_errors.len(), 1);
    assert_eq!(link_errors[0]["link_name"], "workspace-bad");
}

#[test]
fn stop_keyword_all_alias_supports_batch_dry_run_without_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("stop")
        .arg("all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("stop all alias dry-run should run");
    assert!(
        output.status.success(),
        "stop all alias dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "stop");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
}

#[test]
fn restart_keyword_all_alias_supports_batch_dry_run_without_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("restart")
        .arg("all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("restart all alias dry-run should run");
    assert!(
        output.status.success(),
        "restart all alias dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "restart");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
}

#[test]
fn suspend_keyword_all_alias_supports_batch_dry_run_without_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("suspend")
        .arg("all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("suspend all alias dry-run should run");
    assert!(
        output.status.success(),
        "suspend all alias dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "suspend");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
}

#[test]
fn resume_keyword_all_alias_supports_batch_dry_run_without_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(
        tmp.path(),
        json!({
            "session-a": build_session("session-a", "api-a", "stopped")
        }),
    );

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("all")
        .arg("--dry-run")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("resume all alias dry-run should run");
    assert!(
        output.status.success(),
        "resume all alias dry-run should succeed"
    );

    let doc: Value = serde_json::from_slice(&output.stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "resume");
    assert_eq!(doc["scope"], "local");
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["processed_count"], 1);
    assert_eq!(doc["success_count"], 1);
    assert_eq!(doc["failed_count"], 0);
}
