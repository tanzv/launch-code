use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn cli_dap_events_rejects_zero_max() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("events")
        .arg("--id")
        .arg("session-1")
        .arg("--max")
        .arg("0")
        .output()
        .expect("dap events should run");

    assert!(
        !output.status.success(),
        "dap events should reject max below range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max must be between 1 and 1000"),
        "error should mention max range"
    );
}

#[test]
fn cli_dap_events_rejects_oversized_max() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("events")
        .arg("--id")
        .arg("session-1")
        .arg("--max")
        .arg("1001")
        .output()
        .expect("dap events should run");

    assert!(
        !output.status.success(),
        "dap events should reject max above range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max must be between 1 and 1000"),
        "error should mention max range"
    );
}

#[test]
fn cli_dap_adopt_subprocess_rejects_zero_max_events() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("adopt-subprocess")
        .arg("--id")
        .arg("session-1")
        .arg("--max-events")
        .arg("0")
        .output()
        .expect("dap adopt-subprocess should run");

    assert!(
        !output.status.success(),
        "dap adopt-subprocess should reject max-events below range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max-events must be between 1 and 1000"),
        "error should mention max-events range"
    );
}

#[test]
fn cli_dap_request_rejects_non_object_arguments() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("request")
        .arg("--id")
        .arg("session-1")
        .arg("--command")
        .arg("threads")
        .arg("--arguments")
        .arg("[]")
        .output()
        .expect("dap request should run");

    assert!(
        !output.status.success(),
        "dap request should reject non-object arguments"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("arguments must be a JSON object"),
        "error should mention arguments object requirement"
    );
}

#[test]
fn cli_dap_batch_rejects_non_object_arguments() {
    let tmp = tempdir().expect("temp dir should exist");
    let batch_path = tmp.path().join("batch.json");
    let payload = json!([
        {
            "command": "threads",
            "arguments": []
        }
    ]);
    fs::write(&batch_path, serde_json::to_string_pretty(&payload).unwrap())
        .expect("batch file should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("batch")
        .arg("--id")
        .arg("session-1")
        .arg("--file")
        .arg(batch_path.to_string_lossy().to_string())
        .output()
        .expect("dap batch should run");

    assert!(
        !output.status.success(),
        "dap batch should reject non-object arguments"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("batch item arguments must be a JSON object"),
        "error should mention batch argument object requirement"
    );
}

#[test]
fn cli_dap_batch_rejects_oversized_request_count() {
    let tmp = tempdir().expect("temp dir should exist");
    let batch_path = tmp.path().join("batch.json");
    let payload: Vec<serde_json::Value> = (0..129)
        .map(|_| json!({"command": "threads", "arguments": {}}))
        .collect();
    fs::write(&batch_path, serde_json::to_string_pretty(&payload).unwrap())
        .expect("batch file should be written");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("batch")
        .arg("--id")
        .arg("session-1")
        .arg("--file")
        .arg(batch_path.to_string_lossy().to_string())
        .output()
        .expect("dap batch should run");

    assert!(
        !output.status.success(),
        "dap batch should reject oversized request count"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("batch file must include at most 128 requests"),
        "error should mention batch size upper bound"
    );
}

#[test]
fn cli_dap_continue_rejects_zero_thread_id() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("continue")
        .arg("--id")
        .arg("session-1")
        .arg("--thread-id")
        .arg("0")
        .output()
        .expect("dap continue should run");

    assert!(
        !output.status.success(),
        "dap continue should reject non-positive thread-id"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("--thread-id"),
        "error should reference thread-id argument"
    );
}

#[test]
fn cli_dap_variables_rejects_zero_variables_reference() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("variables")
        .arg("--id")
        .arg("session-1")
        .arg("--variables-reference")
        .arg("0")
        .output()
        .expect("dap variables should run");

    assert!(
        !output.status.success(),
        "dap variables should reject non-positive variables-reference"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("--variables-reference"),
        "error should reference variables-reference argument"
    );
}

#[test]
fn cli_dap_evaluate_rejects_zero_frame_id() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("evaluate")
        .arg("--id")
        .arg("session-1")
        .arg("--expression")
        .arg("counter + 1")
        .arg("--frame-id")
        .arg("0")
        .output()
        .expect("dap evaluate should run");

    assert!(
        !output.status.success(),
        "dap evaluate should reject non-positive frame-id"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("--frame-id"),
        "error should reference frame-id argument"
    );
}

#[test]
fn cli_dap_scopes_rejects_zero_frame_id() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("scopes")
        .arg("--id")
        .arg("session-1")
        .arg("--frame-id")
        .arg("0")
        .output()
        .expect("dap scopes should run");

    assert!(
        !output.status.success(),
        "dap scopes should reject non-positive frame-id"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("--frame-id"),
        "error should reference frame-id argument"
    );
}

#[test]
fn cli_dap_breakpoints_rejects_zero_line() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("breakpoints")
        .arg("--id")
        .arg("session-1")
        .arg("--path")
        .arg("app.py")
        .arg("--line")
        .arg("0")
        .output()
        .expect("dap breakpoints should run");

    assert!(
        !output.status.success(),
        "dap breakpoints should reject non-positive line"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("--line"),
        "error should reference line argument"
    );
}

#[test]
fn cli_dap_request_rejects_empty_command() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("request")
        .arg("--id")
        .arg("session-1")
        .arg("--command")
        .arg("")
        .output()
        .expect("dap request should run");

    assert!(
        !output.status.success(),
        "dap request should reject empty command"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("command cannot be empty"),
        "error should mention non-empty command requirement"
    );
}

#[test]
fn cli_dap_request_rejects_whitespace_command() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("request")
        .arg("--id")
        .arg("session-1")
        .arg("--command")
        .arg("   ")
        .output()
        .expect("dap request should run");

    assert!(
        !output.status.success(),
        "dap request should reject whitespace command"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("command cannot be empty"),
        "error should mention non-empty command requirement"
    );
}

#[test]
fn cli_dap_breakpoints_rejects_empty_path() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("breakpoints")
        .arg("--id")
        .arg("session-1")
        .arg("--path")
        .arg("")
        .arg("--line")
        .arg("12")
        .output()
        .expect("dap breakpoints should run");

    assert!(
        !output.status.success(),
        "dap breakpoints should reject empty path"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("path cannot be empty"),
        "error should mention non-empty path requirement"
    );
}

#[test]
fn cli_dap_breakpoints_rejects_whitespace_path() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("breakpoints")
        .arg("--id")
        .arg("session-1")
        .arg("--path")
        .arg("   ")
        .arg("--line")
        .arg("12")
        .output()
        .expect("dap breakpoints should run");

    assert!(
        !output.status.success(),
        "dap breakpoints should reject whitespace path"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("path cannot be empty"),
        "error should mention non-empty path requirement"
    );
}
