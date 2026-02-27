use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn parse_json_stdout(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout should be utf8");
    serde_json::from_str(&stdout).expect("stdout should be valid json")
}

fn parse_json_stderr(output: &std::process::Output) -> Value {
    let stderr = String::from_utf8(output.stderr.clone()).expect("stderr should be utf8");
    serde_json::from_str(&stderr).expect("stderr should be valid json")
}

#[test]
fn build_help_exposes_cross_compile_flags() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("build")
        .arg("--help")
        .output()
        .expect("build help should run");
    assert!(output.status.success(), "build help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("--platform"), "build help should expose --platform");
    assert!(stdout.contains("--target"), "build help should expose --target");
    assert!(stdout.contains("--goos"), "build help should expose --goos");
    assert!(stdout.contains("--goarch"), "build help should expose --goarch");
    assert!(stdout.contains("--dry-run"), "build help should expose --dry-run");
}

#[test]
fn build_dry_run_rust_platform_resolves_target_triple() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("--json")
        .arg("build")
        .arg("--runtime")
        .arg("rust")
        .arg("--cwd")
        .arg(tmp.path())
        .arg("--entry")
        .arg("lcode")
        .arg("--platform")
        .arg("linux/amd64")
        .arg("--dry-run")
        .output()
        .expect("build dry-run should run");

    assert!(output.status.success(), "rust dry-run build should succeed");
    let doc = parse_json_stdout(&output);
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["runtime"], "rust");
    assert_eq!(doc["platform"], "linux/amd64");
    assert_eq!(doc["dry_run"], true);

    let command = doc["command"]
        .as_array()
        .expect("command should be an array");
    let command_values: Vec<String> = command
        .iter()
        .map(|item| {
            item.as_str()
                .expect("command item should be string")
                .to_string()
        })
        .collect();
    assert!(
        command_values
            .windows(2)
            .any(|window| window[0] == "--target" && window[1] == "x86_64-unknown-linux-gnu"),
        "rust dry-run should map platform to target triple"
    );
}

#[test]
fn build_dry_run_go_platform_sets_go_env() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("--json")
        .arg("build")
        .arg("--runtime")
        .arg("go")
        .arg("--cwd")
        .arg(tmp.path())
        .arg("--entry")
        .arg("./cmd/service")
        .arg("--platform")
        .arg("windows/arm64")
        .arg("--dry-run")
        .output()
        .expect("go build dry-run should run");

    assert!(output.status.success(), "go dry-run build should succeed");
    let doc = parse_json_stdout(&output);
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["runtime"], "go");
    assert_eq!(doc["platform"], "windows/arm64");
    assert_eq!(doc["env"]["GOOS"], "windows");
    assert_eq!(doc["env"]["GOARCH"], "arm64");
}

#[test]
fn build_go_rejects_rust_target_flag() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("--json")
        .arg("build")
        .arg("--runtime")
        .arg("go")
        .arg("--cwd")
        .arg(tmp.path())
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu")
        .arg("--dry-run")
        .output()
        .expect("go build validation should run");

    assert!(!output.status.success(), "invalid go build args should fail");
    let doc = parse_json_stderr(&output);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "invalid_build_options");
}

#[test]
fn build_rejects_unsupported_runtime() {
    let tmp = tempdir().expect("temp dir should exist");
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("--json")
        .arg("build")
        .arg("--runtime")
        .arg("python")
        .arg("--cwd")
        .arg(tmp.path())
        .arg("--dry-run")
        .output()
        .expect("build validation should run");

    assert!(
        !output.status.success(),
        "unsupported build runtime should fail"
    );
    let doc = parse_json_stderr(&output);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"], "unsupported_build_runtime");
}
