use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

fn write_state(path: &std::path::Path) {
    let state_dir = path.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_file = state_dir.join("state.json");
    let state = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": {
            "parent-session": {
                "id": "parent-session",
                "spec": {
                    "name": "parent",
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
                "status": "running",
                "pid": 4242,
                "supervisor_pid": null,
                "log_path": null,
                "debug_meta": null,
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            },
            "parent-session-subprocess-777": {
                "id": "parent-session-subprocess-777",
                "spec": {
                    "name": "child",
                    "runtime": "python",
                    "entry": "child.py",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "managed": false,
                    "mode": "debug",
                    "debug": {
                        "host": "127.0.0.1",
                        "port": 5678,
                        "wait_for_client": false,
                        "subprocess": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": "running",
                "pid": 7777,
                "supervisor_pid": 4242,
                "log_path": null,
                "debug_meta": {
                    "host": "127.0.0.1",
                    "requested_port": 5678,
                    "active_port": 5678,
                    "fallback_applied": false,
                    "reconnect_policy": "auto-retry"
                },
                "created_at": 1,
                "updated_at": 1,
                "last_exit_code": null,
                "restart_count": 0
            }
        },
        "project_info": null
    });
    fs::write(
        state_file,
        serde_json::to_string_pretty(&state).expect("state should serialize"),
    )
    .expect("state should be written");
}

#[test]
fn json_list_includes_parent_and_child_topology_fields() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(tmp.path());

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("list")
        .output()
        .expect("list should run");
    assert!(output.status.success(), "list should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be json");
    let items = doc["items"].as_array().expect("items should be array");

    let parent = items
        .iter()
        .find(|item| item["id"] == "parent-session")
        .expect("parent session should exist");
    assert_eq!(parent["parent_session_id"], Value::Null);
    assert_eq!(parent["child_session_count"].as_u64(), Some(1));
    assert_eq!(
        parent["child_session_ids"][0],
        "parent-session-subprocess-777"
    );

    let child = items
        .iter()
        .find(|item| item["id"] == "parent-session-subprocess-777")
        .expect("child session should exist");
    assert_eq!(child["parent_session_id"], "parent-session");
    assert_eq!(child["child_session_count"].as_u64(), Some(0));
}

#[test]
fn inspect_includes_topology_fields() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(tmp.path());

    let mut parent_cmd = cargo_bin_cmd!("launch-code");
    let parent_output = parent_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("inspect")
        .arg("--id")
        .arg("parent-session")
        .arg("--tail")
        .arg("0")
        .output()
        .expect("inspect should run");
    assert!(parent_output.status.success(), "inspect should succeed");
    let parent_doc: Value =
        serde_json::from_slice(&parent_output.stdout).expect("inspect output should be json");
    assert_eq!(parent_doc["topology"]["parent_session_id"], Value::Null);
    assert_eq!(
        parent_doc["topology"]["child_session_ids"][0],
        "parent-session-subprocess-777"
    );

    let mut child_cmd = cargo_bin_cmd!("launch-code");
    let child_output = child_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("inspect")
        .arg("--id")
        .arg("parent-session-subprocess-777")
        .arg("--tail")
        .arg("0")
        .output()
        .expect("inspect should run");
    assert!(child_output.status.success(), "inspect should succeed");
    let child_doc: Value =
        serde_json::from_slice(&child_output.stdout).expect("inspect output should be json");
    assert_eq!(child_doc["topology"]["parent_session_id"], "parent-session");
    assert_eq!(
        child_doc["topology"]["child_session_ids"]
            .as_array()
            .expect("child list should be array")
            .len(),
        0
    );
}
