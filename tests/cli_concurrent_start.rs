use std::fs;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;

use assert_cmd::cargo::cargo_bin_cmd;
use launch_code::model::AppState;
use predicates::str::contains;
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
fn concurrent_start_processes_persist_all_sessions() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path().to_path_buf();
    let script_path = workspace.join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let workers = 8usize;
    let barrier = Arc::new(Barrier::new(workers));
    let (tx, rx) = mpsc::channel::<String>();
    let mut joins = Vec::new();

    for idx in 0..workers {
        let barrier_for_thread = Arc::clone(&barrier);
        let tx_for_thread = tx.clone();
        let workspace_for_thread = workspace.clone();
        let script_for_thread = script_path.clone();

        joins.push(thread::spawn(move || {
            barrier_for_thread.wait();

            let mut start_cmd = cargo_bin_cmd!("launch-code");
            let start_assert = start_cmd
                .env("LAUNCH_CODE_HOME", &workspace_for_thread)
                .arg("start")
                .arg("--name")
                .arg(format!("python-session-{idx}"))
                .arg("--runtime")
                .arg("python")
                .arg("--entry")
                .arg(script_for_thread.to_string_lossy().to_string())
                .arg("--cwd")
                .arg(workspace_for_thread.to_string_lossy().to_string())
                .assert()
                .success()
                .stdout(contains("session_id="));

            let output = String::from_utf8(start_assert.get_output().stdout.clone())
                .expect("start output should be utf8");
            let session_id = parse_session_id(&output).expect("session id should be present");
            tx_for_thread
                .send(session_id)
                .expect("session id should be sent");
        }));
    }

    drop(tx);
    for join in joins {
        join.join().expect("thread should join");
    }

    let started_ids: Vec<String> = rx.iter().collect();
    assert_eq!(started_ids.len(), workers);

    let state_path = workspace.join(".launch-code").join("state.json");
    let payload = fs::read_to_string(&state_path).expect("state file should exist");
    let state: AppState = serde_json::from_str(&payload).expect("state should deserialize");
    assert_eq!(state.sessions.len(), workers);

    for session_id in started_ids {
        let mut stop_cmd = cargo_bin_cmd!("launch-code");
        stop_cmd
            .env("LAUNCH_CODE_HOME", &workspace)
            .arg("stop")
            .arg("--id")
            .arg(session_id)
            .assert()
            .success()
            .stdout(contains("stopped"));
    }
}
