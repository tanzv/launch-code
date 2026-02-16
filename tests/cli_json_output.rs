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
