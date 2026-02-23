use std::fs;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

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

fn write_state(root: &Path, sessions: Vec<Value>) {
    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");

    let mut session_map = serde_json::Map::new();
    for session in sessions {
        let id = session["id"]
            .as_str()
            .expect("session id should exist")
            .to_string();
        session_map.insert(id, session);
    }

    let state_doc = json!({
        "schema_version": 1,
        "profiles": {},
        "project_info": null,
        "sessions": session_map
    });
    fs::write(
        state_dir.join("state.json"),
        serde_json::to_string_pretty(&state_doc).expect("state json should serialize"),
    )
    .expect("state should be written");
}

#[test]
fn trace_time_list_emits_phase_metrics_to_stderr() {
    let tmp = tempdir().expect("temp dir should exist");
    write_state(tmp.path(), vec![build_session("session-a", "alpha")]);

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--trace-time")
        .arg("--local")
        .arg("list")
        .output()
        .expect("list should run");
    assert!(output.status.success(), "list should succeed");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("trace_time"),
        "trace-time should emit timing line to stderr"
    );
    assert!(
        stderr.contains("command=list"),
        "trace-time output should include command label"
    );
}
