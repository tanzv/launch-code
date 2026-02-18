use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::tempdir;

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while std::time::Instant::now() < deadline {
        line.clear();
        if stdout
            .read_line(&mut line)
            .ok()
            .filter(|v| *v > 0)
            .is_some()
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

fn read_json_body(response: &mut ureq::http::Response<ureq::Body>) -> Value {
    let text = response
        .body_mut()
        .read_to_string()
        .expect("response body should be readable");
    serde_json::from_str(&text).expect("response body should be json")
}

fn write_session_state(root: &std::path::Path) {
    let state_dir = root.join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    let payload = json!({
        "schema_version": 1,
        "profiles": {},
        "sessions": {
            "cleanup-stopped-a": {
                "id": "cleanup-stopped-a",
                "spec": {
                    "name": "cleanup-a",
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
            },
            "cleanup-stopped-b": {
                "id": "cleanup-stopped-b",
                "spec": {
                    "name": "cleanup-b",
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
        state_path,
        serde_json::to_string_pretty(&payload).expect("state should serialize"),
    )
    .expect("state should be written");
}

#[test]
fn serve_cleanup_route_supports_dry_run_and_apply() {
    let tmp = tempdir().expect("temp dir should exist");
    write_session_state(tmp.path());

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into();

    let mut dry_run_res = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"dry_run\":true}")
        .expect("cleanup dry-run should succeed");
    assert_eq!(dry_run_res.status(), ureq::http::StatusCode::OK);
    let dry_run_doc = read_json_body(&mut dry_run_res);
    assert_eq!(dry_run_doc["ok"], true);
    assert_eq!(dry_run_doc["dry_run"], true);
    assert_eq!(dry_run_doc["matched_count"], 2);
    assert_eq!(dry_run_doc["removed_count"], 0);
    assert_eq!(dry_run_doc["kept_count"], 2);

    let mut sessions_before = agent
        .get(&format!("{url}/v1/sessions"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("sessions get should succeed");
    let sessions_before_doc = read_json_body(&mut sessions_before);
    assert_eq!(
        sessions_before_doc["sessions"]
            .as_array()
            .expect("sessions should be array")
            .len(),
        2
    );

    let mut cleanup_res = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{}")
        .expect("cleanup should succeed");
    assert_eq!(cleanup_res.status(), ureq::http::StatusCode::OK);
    let cleanup_doc = read_json_body(&mut cleanup_res);
    assert_eq!(cleanup_doc["ok"], true);
    assert_eq!(cleanup_doc["dry_run"], false);
    assert_eq!(cleanup_doc["matched_count"], 2);
    assert_eq!(cleanup_doc["removed_count"], 2);
    assert_eq!(cleanup_doc["kept_count"], 0);

    let mut sessions_after = agent
        .get(&format!("{url}/v1/sessions"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("sessions get should succeed");
    let sessions_after_doc = read_json_body(&mut sessions_after);
    assert_eq!(
        sessions_after_doc["sessions"]
            .as_array()
            .expect("sessions should be array")
            .len(),
        0
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_cleanup_route_validates_payload() {
    let tmp = tempdir().expect("temp dir should exist");
    write_session_state(tmp.path());

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_statuses_type = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"statuses\":\"stopped\"}")
        .expect("cleanup bad statuses type should complete");
    assert_eq!(
        bad_statuses_type.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_statuses_type_doc = read_json_body(&mut bad_statuses_type);
    assert_eq!(bad_statuses_type_doc["ok"], false);
    assert_eq!(bad_statuses_type_doc["error"], "bad_request");

    let mut bad_status_value = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"statuses\":[\"running\"]}")
        .expect("cleanup bad status value should complete");
    assert_eq!(
        bad_status_value.status(),
        ureq::http::StatusCode::BAD_REQUEST
    );
    let bad_status_value_doc = read_json_body(&mut bad_status_value);
    assert_eq!(bad_status_value_doc["ok"], false);
    assert_eq!(bad_status_value_doc["error"], "bad_request");

    let mut bad_dry_run = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"dry_run\":\"yes\"}")
        .expect("cleanup bad dry_run type should complete");
    assert_eq!(bad_dry_run.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_dry_run_doc = read_json_body(&mut bad_dry_run);
    assert_eq!(bad_dry_run_doc["ok"], false);
    assert_eq!(bad_dry_run_doc["error"], "bad_request");

    let mut empty_statuses = agent
        .post(&format!("{url}/v1/sessions/cleanup"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"statuses\":[]}")
        .expect("cleanup empty statuses should complete");
    assert_eq!(empty_statuses.status(), ureq::http::StatusCode::BAD_REQUEST);
    let empty_statuses_doc = read_json_body(&mut empty_statuses);
    assert_eq!(empty_statuses_doc["ok"], false);
    assert_eq!(empty_statuses_doc["error"], "bad_request");

    let _ = child.kill();
    let _ = child.wait();
}
