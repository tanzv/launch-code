use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use serde_json::Value;
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
fn launch_expands_workspace_and_env_variables() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path();
    let vscode_dir = workspace.join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

    let script_path = workspace.join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let launch_file = vscode_dir.join("launch.json");
    fs::write(
        &launch_file,
        "{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {\n      \"name\": \"Python Variable Config\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"${workspaceFolder}/${env:APP_ENTRY}\",\n      \"cwd\": \"${workspaceFolder}\"\n    }\n  ]\n}\n",
    )
    .expect("launch json should be written");

    let mut launch_cmd = cargo_bin_cmd!("launch-code");
    let launch_assert = launch_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .env("APP_ENTRY", "app.py")
        .arg("launch")
        .arg("--name")
        .arg("Python Variable Config")
        .arg("--mode")
        .arg("run")
        .arg("--launch-file")
        .arg(launch_file.to_string_lossy().to_string())
        .assert()
        .success()
        .stdout(contains("session_id="));

    let launch_output = String::from_utf8(launch_assert.get_output().stdout.clone())
        .expect("launch output should be utf8");
    let session_id = parse_session_id(&launch_output).expect("session id should be present");

    let state_path = workspace.join(".launch-code").join("state.json");
    let state_payload = fs::read_to_string(&state_path).expect("state file should exist");
    let state: Value = serde_json::from_str(&state_payload).expect("state should be valid json");

    let session_entry = state["sessions"][&session_id]["spec"]["entry"]
        .as_str()
        .expect("spec entry should exist");
    assert_eq!(session_entry, script_path.to_string_lossy());

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .arg("stop")
        .arg("--id")
        .arg(session_id)
        .assert()
        .success()
        .stdout(contains("stopped"));
}
