use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn run_json_error(args: &[&str]) -> Value {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .args(args)
        .output()
        .expect("command should run");

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    serde_json::from_str::<Value>(&stderr).expect("stderr should be valid json")
}

#[test]
fn debug_rejects_go_mode_for_non_go_runtime() {
    let doc = run_json_error(&[
        "debug",
        "--runtime",
        "python",
        "--entry",
        "app.py",
        "--cwd",
        ".",
        "--go-mode",
        "test",
    ]);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
}

#[test]
fn go_attach_mode_requires_positive_pid_source() {
    let doc = run_json_error(&[
        "debug",
        "--runtime",
        "go",
        "--entry",
        "not-a-pid",
        "--cwd",
        ".",
        "--go-mode",
        "attach",
    ]);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("requires a positive PID")),
        "error should explain PID requirement"
    );
}

#[test]
fn go_attach_mode_rejects_program_arguments() {
    let doc = run_json_error(&[
        "debug",
        "--runtime",
        "go",
        "--entry",
        "12345",
        "--cwd",
        ".",
        "--go-mode",
        "attach",
        "--arg",
        "extra",
    ]);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_start_options");
    assert!(
        doc["message"]
            .as_str()
            .is_some_and(|message| message.contains("not supported")),
        "error should explain attach arg restriction"
    );
}

#[test]
fn debug_help_exposes_go_mode_arguments() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("debug")
        .arg("--help")
        .output()
        .expect("help should run");
    assert!(output.status.success(), "help should succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--go-mode"),
        "debug help should include --go-mode"
    );
    assert!(
        stdout.contains("--go-attach-pid"),
        "debug help should include --go-attach-pid"
    );
}
