use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
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

fn build_session(id: &str, name: &str) -> Value {
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
        "status": "stopped",
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

fn write_state_with_sessions(tmp: &tempfile::TempDir, sessions: Value) {
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state_doc = json!({
        "schema_version": 1,
        "profiles": {},
        "project_info": null,
        "sessions": sessions
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state_doc).expect("state json"),
    )
    .expect("state should be written");
}

fn assert_json_session_not_found_for_positional(command: &[&str]) {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .args(command)
        .arg("missing-session")
        .output()
        .expect("command should run");

    assert!(
        !output.status.success(),
        "missing positional session id target should fail cleanly"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "session_not_found");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_error_output_includes_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("status")
        .arg("--id")
        .arg("missing-session")
        .output()
        .expect("status should run");

    assert!(
        !output.status.success(),
        "status for missing session should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "session_not_found");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_stop_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["stop"]);
}

#[test]
fn json_stop_rejects_batch_flags_without_all_target() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("stop")
        .arg("session-1")
        .arg("--dry-run")
        .output()
        .expect("stop should run");

    assert!(
        !output.status.success(),
        "single-session stop with batch flags should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("batch flags")),
        "error should explain batch flag scope"
    );
}

#[test]
fn json_restart_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["restart"]);
}

#[test]
fn json_suspend_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["suspend"]);
}

#[test]
fn json_suspend_rejects_batch_flags_without_all_target() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("suspend")
        .arg("session-1")
        .arg("--dry-run")
        .output()
        .expect("suspend should run");

    assert!(
        !output.status.success(),
        "single-session suspend with batch flags should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
}

#[test]
fn json_resume_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["resume"]);
}

#[test]
fn json_resume_rejects_batch_flags_without_all_target() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("resume")
        .arg("session-1")
        .arg("--dry-run")
        .output()
        .expect("resume should run");

    assert!(
        !output.status.success(),
        "single-session resume with batch flags should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
}

#[test]
fn json_status_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["status"]);
}

#[test]
fn json_attach_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["attach"]);
}

#[test]
fn json_inspect_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["inspect"]);
}

#[test]
fn json_logs_accepts_positional_session_id() {
    assert_json_session_not_found_for_positional(&["logs"]);
}

#[test]
fn json_status_resolves_unique_session_id_prefix() {
    let tmp = tempdir().expect("temp dir should exist");
    let full_id = "a1234567890abcdef1234567890abc1";
    let mut sessions = serde_json::Map::new();
    sessions.insert(full_id.to_string(), build_session(full_id, "prefix-target"));
    write_state_with_sessions(&tmp, Value::Object(sessions));

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("status")
        .arg("a1234567890")
        .output()
        .expect("status should run");

    assert!(output.status.success(), "status by short id should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["session"]["id"], full_id);
}

#[test]
fn json_status_rejects_ambiguous_session_id_prefix() {
    let tmp = tempdir().expect("temp dir should exist");
    let id_a = "shared001234567890abcdef1234567890";
    let id_b = "shared991234567890abcdef1234567890";
    let mut sessions = serde_json::Map::new();
    sessions.insert(id_a.to_string(), build_session(id_a, "first"));
    sessions.insert(id_b.to_string(), build_session(id_b, "second"));
    write_state_with_sessions(&tmp, Value::Object(sessions));

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("status")
        .arg("shared")
        .output()
        .expect("status should run");

    assert!(
        !output.status.success(),
        "status by ambiguous short id should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "session_id_ambiguous");
}

#[test]
fn json_debug_rejects_rust_runtime_with_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("debug")
        .arg("--runtime")
        .arg("rust")
        .arg("--entry")
        .arg("demo-bin")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("debug should run");

    assert!(
        !output.status.success(),
        "rust debug should fail with unsupported runtime error"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "unsupported_debug_runtime");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("python, node, and go runtimes only")),
        "error message should explain supported runtime"
    );
}

#[test]
fn json_debug_go_reports_missing_dlv_with_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");
    fs::write(tmp.path().join("main.go"), "package main\nfunc main() {}\n")
        .expect("main.go should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env("PATH", "")
        .arg("--json")
        .arg("debug")
        .arg("--runtime")
        .arg("go")
        .arg("--entry")
        .arg("main.go")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("go debug should run");

    assert!(
        !output.status.success(),
        "go debug should fail when dlv is unavailable"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "go_dlv_unavailable");
}

#[test]
fn json_dap_rejects_node_runtime_with_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state_doc = json!({
        "schema_version": 1,
        "profiles": {},
        "project_info": null,
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "node-debug",
                    "runtime": "node",
                    "entry": "app.js",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "managed": false,
                    "mode": "debug",
                    "debug": {
                        "host": "127.0.0.1",
                        "port": 9229,
                        "wait_for_client": true,
                        "subprocess": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": "running",
                "pid": 12345,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": {
                    "host": "127.0.0.1",
                    "requested_port": 9229,
                    "active_port": 9229,
                    "fallback_applied": false,
                    "reconnect_policy": "auto-retry"
                },
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        }
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state_doc).expect("state json"),
    )
    .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env_remove("LCODE_NODE_DAP_ADAPTER_CMD")
        .env("LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY", "1")
        .arg("--json")
        .arg("dap")
        .arg("threads")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("dap threads should run");

    assert!(
        !output.status.success(),
        "dap threads should fail for node runtime"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "unsupported_dap_runtime");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("LCODE_NODE_DAP_ADAPTER_CMD")),
        "error message should explain node dap adapter configuration"
    );
}

#[test]
fn json_doctor_runtime_strict_reports_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .env_remove("LCODE_NODE_DAP_ADAPTER_CMD")
        .env("LCODE_NODE_DAP_DISABLE_AUTO_DISCOVERY", "1")
        .arg("--json")
        .arg("doctor")
        .arg("runtime")
        .arg("--runtime")
        .arg("node")
        .arg("--strict")
        .output()
        .expect("doctor runtime strict should run");

    assert!(
        !output.status.success(),
        "doctor runtime strict should fail when node adapter is unavailable"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let stdout_doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(stdout_doc["ok"], true);
    assert_eq!(stdout_doc["strict"], true);

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let err_doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(err_doc["ok"], false);
    assert_eq!(err_doc["error"], "runtime_readiness_failed");
}

#[test]
fn json_list_output_is_structured() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("list")
        .output()
        .expect("list should run");

    assert!(output.status.success(), "list should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert!(doc["items"].is_array(), "items should be an array");
}

#[test]
fn json_project_show_returns_structured_payload() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");

    assert!(output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert!(
        doc["project"].is_null(),
        "project should be null when metadata has not been set"
    );
}

#[test]
fn json_project_set_and_unset_return_message_payloads() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .arg("--language")
        .arg("rust")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");
    let set_stdout = String::from_utf8(set_output.stdout).expect("stdout should be utf8");
    let set_doc: Value = serde_json::from_str(&set_stdout).expect("stdout should be valid json");
    assert_eq!(set_doc["ok"], true);
    assert!(
        set_doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("project_info_updated=true")),
        "project set should return a stable confirmation message"
    );

    let mut unset_cmd = cargo_bin_cmd!("launch-code");
    let unset_output = unset_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("unset")
        .arg("--field")
        .arg("all")
        .output()
        .expect("project unset should run");
    assert!(
        unset_output.status.success(),
        "project unset should succeed"
    );
    let unset_stdout = String::from_utf8(unset_output.stdout).expect("stdout should be utf8");
    let unset_doc: Value =
        serde_json::from_str(&unset_stdout).expect("stdout should be valid json");
    assert_eq!(unset_doc["ok"], true);
    assert!(
        unset_doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("project_info_unset=true")),
        "project unset should return a stable confirmation message"
    );
}

#[test]
fn json_project_list_returns_structured_items() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .arg("--language")
        .arg("rust")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("list")
        .output()
        .expect("project list should run");
    assert!(list_output.status.success(), "project list should succeed");

    let list_stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    let list_doc: Value = serde_json::from_str(&list_stdout).expect("stdout should be valid json");
    assert_eq!(list_doc["ok"], true);
    assert!(
        list_doc["items"].is_array(),
        "project list should return an items array"
    );
}

#[test]
fn json_project_list_honors_field_filter_and_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("list")
        .arg("--field")
        .arg("name")
        .arg("--field")
        .arg("repository")
        .arg("--all")
        .output()
        .expect("project list should run");
    assert!(list_output.status.success(), "project list should succeed");

    let stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    let items = doc["items"].as_array().expect("items should be an array");
    assert_eq!(
        items.len(),
        2,
        "filtered field list should include two rows"
    );
    assert_eq!(items[0]["field"], "name");
    assert_eq!(items[0]["value"], "launch-code");
    assert_eq!(items[1]["field"], "repository");
    assert!(items[1]["value"].is_null());
}

#[test]
fn json_link_show_missing_link_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("HOME", tmp.path())
        .arg("--json")
        .arg("link")
        .arg("show")
        .arg("--name")
        .arg("missing-link")
        .output()
        .expect("link show should run");

    assert!(!output.status.success(), "link show should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "link_not_found");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_cleanup_dry_run_returns_structured_payload() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": {
            "cleanup-session": {
                "id": "cleanup-session",
                "spec": {
                    "name": "cleanup",
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
                "status": "stopped",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": null,
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        },
        "project_info": null
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).expect("state json"),
    )
    .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("cleanup")
        .arg("--dry-run")
        .output()
        .expect("cleanup should run");
    assert!(output.status.success(), "cleanup should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["matched_count"], 1);
    assert_eq!(doc["removed_count"], 0);
    assert_eq!(doc["kept_count"], 1);
}

#[test]
fn json_status_output_returns_structured_session_payload() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "api",
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
                "status": "stopped",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": null,
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        },
        "project_info": null
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).expect("state json"),
    )
    .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("status")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("status should run");
    assert!(output.status.success(), "status should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "status");
    assert_eq!(doc["session"]["id"], "session-1");
    assert_eq!(doc["session"]["status"], "stopped");
    assert_eq!(doc["session"]["runtime"], "python");
    assert_eq!(doc["session"]["mode"], "run");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("status=stopped"))
    );
}

#[test]
fn json_stop_output_returns_structured_session_payload() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "api",
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
                "status": "stopped",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": null,
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        },
        "project_info": null
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).expect("state json"),
    )
    .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("stop")
        .arg("--id")
        .arg("session-1")
        .output()
        .expect("stop should run");
    assert!(output.status.success(), "stop should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "stop");
    assert_eq!(doc["session"]["id"], "session-1");
    assert_eq!(doc["session"]["status"], "stopped");
    assert_eq!(doc["session"]["runtime"], "python");
    assert_eq!(doc["session"]["mode"], "run");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("status=stopped"))
    );
}

#[test]
fn json_list_output_includes_session_objects() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let state = json!({
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "api",
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
                "status": "stopped",
                "pid": null,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": null,
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        }
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).expect("state json"),
    )
    .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("list")
        .output()
        .expect("list should run");
    assert!(output.status.success(), "list should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    let items = doc["items"].as_array().expect("items should be an array");
    assert_eq!(items.len(), 1, "should include one session row");
    let item = &items[0];
    assert_eq!(item["id"], "session-1");
    assert_eq!(item["status"], "stopped");
    assert_eq!(item["runtime"], "python");
    assert_eq!(item["mode"], "run");
    assert_eq!(item["pid"], Value::Null);
    assert_eq!(item["name"], "api");
    assert_eq!(item["entry"], "app.py");
}

#[test]
fn json_logs_invalid_regex_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("logs")
        .arg("--id")
        .arg("unused")
        .arg("--regex")
        .arg("[")
        .output()
        .expect("logs should run");

    assert!(
        !output.status.success(),
        "logs with invalid regex should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_log_regex");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_logs_invalid_exclude_regex_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("logs")
        .arg("--id")
        .arg("unused")
        .arg("--exclude-regex")
        .arg("[")
        .output()
        .expect("logs should run");

    assert!(
        !output.status.success(),
        "logs with invalid exclude regex should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_log_regex");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_start_invalid_env_file_line_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");
    let env_file = tmp.path().join("bad.env");
    std::fs::write(&env_file, "BROKEN_LINE\n").expect("env file should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("start")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg("app.py")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--env-file")
        .arg(env_file.to_string_lossy().to_string())
        .output()
        .expect("start should execute");

    assert!(
        !output.status.success(),
        "start with invalid env file line should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_env_file_line");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_config_run_missing_profile_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("config")
        .arg("run")
        .arg("--name")
        .arg("missing-profile")
        .output()
        .expect("config run should execute");

    assert!(
        !output.status.success(),
        "config run with missing profile should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "profile_not_found");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_config_import_unsupported_bundle_version_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");
    let bundle = tmp.path().join("profiles-unsupported.json");
    std::fs::write(&bundle, "{\n  \"version\": 999,\n  \"profiles\": {}\n}\n")
        .expect("bundle should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("config")
        .arg("import")
        .arg("--file")
        .arg(bundle.to_string_lossy().to_string())
        .output()
        .expect("config import should run");

    assert!(
        !output.status.success(),
        "config import with unsupported version should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "profile_bundle_version_unsupported");
    assert!(doc["message"].as_str().is_some());
}

#[test]
fn json_config_validate_invalid_profile_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("invalid-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg("missing.py")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("config save should run");
    assert!(save_output.status.success(), "config save should succeed");

    let mut validate_cmd = cargo_bin_cmd!("launch-code");
    let output = validate_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("config")
        .arg("validate")
        .arg("--name")
        .arg("invalid-profile")
        .output()
        .expect("config validate should run");

    assert!(
        !output.status.success(),
        "config validate for invalid profile should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "profile_validation_failed");
    assert!(doc["message"].as_str().is_some());
}

#[cfg(unix)]
#[test]
fn json_restart_timeout_uses_stop_timeout_error_code() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("ignore_term.py");
    std::fs::write(
        &script_path,
        "import signal\nimport time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('ready', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("json-restart-timeout")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("start output should be utf8");
    let session_id = parse_field(&start_stdout, "session_id")
        .expect("session id should be present")
        .to_string();
    std::thread::sleep(std::time::Duration::from_millis(300));

    let mut restart_cmd = cargo_bin_cmd!("launch-code");
    let restart_output = restart_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("restart")
        .arg("--id")
        .arg(&session_id)
        .arg("--no-force")
        .arg("--grace-timeout-ms")
        .arg("100")
        .output()
        .expect("restart should run");
    assert!(
        !restart_output.status.success(),
        "restart without force should fail on timeout"
    );
    let restart_stderr = String::from_utf8(restart_output.stderr).expect("stderr should be utf8");
    let restart_doc: Value =
        serde_json::from_str(&restart_stderr).expect("stderr should be valid json");
    assert_eq!(restart_doc["ok"], false);
    assert_eq!(restart_doc["error"], "stop_timeout");

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("cleanup stop should run");
    assert!(
        cleanup_output.status.success(),
        "cleanup stop should succeed"
    );
}

#[test]
fn json_config_run_invalid_env_file_line_has_stable_error_code() {
    let tmp = tempdir().expect("temp dir should exist");
    let entry = tmp.path().join("app.py");
    let env_file = tmp.path().join("bad.env");
    std::fs::write(&entry, "print('ok')\n").expect("entry should be written");
    std::fs::write(&env_file, "BROKEN_LINE\n").expect("env file should be written");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("env-file-bad-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(entry.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("config save should run");
    assert!(save_output.status.success(), "config save should succeed");

    let mut run_cmd = cargo_bin_cmd!("launch-code");
    let output = run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("config")
        .arg("run")
        .arg("--name")
        .arg("env-file-bad-profile")
        .arg("--env-file")
        .arg(env_file.to_string_lossy().to_string())
        .output()
        .expect("config run should execute");

    assert!(
        !output.status.success(),
        "config run with invalid env file line should fail"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_env_file_line");
    assert!(doc["message"].as_str().is_some());
}
