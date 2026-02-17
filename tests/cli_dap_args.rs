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
