use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn run_lcode(args: &[&str], launch_code_home: &std::path::Path) -> std::process::Output {
    let mut cmd = cargo_bin_cmd!("lcode");
    cmd.env("LAUNCH_CODE_HOME", launch_code_home)
        .args(args)
        .output()
        .expect("lcode should run")
}

#[test]
fn lcode_help_is_available() {
    let tmp = tempdir().expect("temp dir should exist");
    let output = run_lcode(&["--help"], tmp.path());
    assert!(output.status.success(), "lcode help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("lcode"),
        "help output should include lcode binary name"
    );
}

#[test]
fn lcode_stop_accepts_positional_session_id_without_panic() {
    let tmp = tempdir().expect("temp dir should exist");
    let output = run_lcode(&["--json", "stop", "missing-session"], tmp.path());

    assert!(
        !output.status.success(),
        "missing session should fail with a structured error"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let doc: Value = serde_json::from_str(&stderr).expect("stderr should be valid json");
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "session_not_found");
    assert!(
        !stderr.contains("panicked at"),
        "command should return a normal error without panic"
    );
}

#[test]
fn lcode_batch_all_dry_run_does_not_trigger_clap_panic() {
    let tmp = tempdir().expect("temp dir should exist");
    let output = run_lcode(
        &[
            "--json",
            "stop",
            "--all",
            "--dry-run",
            "--status",
            "stopped",
        ],
        tmp.path(),
    );
    assert!(output.status.success(), "batch dry-run should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["action"], "stop");
    assert_eq!(doc["all"], true);
    assert_eq!(doc["dry_run"], true);
}
