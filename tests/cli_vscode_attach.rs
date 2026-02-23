use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn vscode_attach_prints_python_attach_configuration_from_state() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");

    let state_path = state_dir.join("state.json");
    let state = json!({
        "sessions": {
            "session-1": {
                "id": "session-1",
                "spec": {
                    "name": "py-debug",
                    "runtime": "python",
                    "entry": "app.py",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "managed": false,
                    "mode": "debug",
                    "debug": {
                        "host": "127.0.0.1",
                        "port": 5679,
                        "wait_for_client": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": "running",
                "pid": 12345,
                "supervisor_pid": null,
                "log_path": ".launch-code/logs/session-1.log",
                "debug_meta": {
                    "host": "127.0.0.1",
                    "requested_port": 5678,
                    "active_port": 5679,
                    "fallback_applied": true,
                    "reconnect_policy": "auto-retry"
                },
                "created_at": 1,
                "updated_at": 2,
                "last_exit_code": null,
                "restart_count": 0
            }
        }
    });

    fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    cmd.env("LAUNCH_CODE_HOME", tmp.path())
        .arg("attach")
        .arg("--id")
        .arg("session-1")
        .assert()
        .success()
        .stdout(contains("\"request\": \"attach\""))
        .stdout(contains("\"port\": 5679"))
        .stdout(contains("\"host\": \"127.0.0.1\""))
        .stdout(contains("\"adapter_kind\": \"python-debugpy\""))
        .stdout(contains("\"transport\": \"tcp\""));
}

#[test]
fn vscode_attach_prints_node_attach_configuration_from_state() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");

    let state_path = state_dir.join("state.json");
    let state = json!({
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
                        "wait_for_client": true
                    },
                    "prelaunch_task": null,
                    "poststop_task": null
                },
                "status": "running",
                "pid": 12345,
                "supervisor_pid": null,
                "log_path": ".launch-code/logs/session-1.log",
                "debug_meta": {
                    "host": "127.0.0.1",
                    "requested_port": 9229,
                    "active_port": 9229,
                    "fallback_applied": false,
                    "reconnect_policy": "manual-reconnect"
                },
                "created_at": 1,
                "updated_at": 2,
                "last_exit_code": null,
                "restart_count": 0
            }
        }
    });

    fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
        .expect("state should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    cmd.env("LAUNCH_CODE_HOME", tmp.path())
        .arg("attach")
        .arg("--id")
        .arg("session-1")
        .assert()
        .success()
        .stdout(contains("\"request\": \"attach\""))
        .stdout(contains("\"type\": \"pwa-node\""))
        .stdout(contains("\"port\": 9229"))
        .stdout(contains("\"address\": \"127.0.0.1\""))
        .stdout(contains("\"adapter_kind\": \"node-inspector\""))
        .stdout(contains("\"transport\": \"tcp\""));
}
