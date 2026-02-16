use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
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
fn cli_can_launch_from_vscode_launch_json() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let vscode_dir = tmp.path().join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

    let script_path = tmp.path().join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let launch_path = vscode_dir.join("launch.json");
    fs::write(
        &launch_path,
        format!(
            "{{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {{\n      \"name\": \"Python Demo\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"{}\",\n      \"cwd\": \"{}\",\n      \"args\": [\"--env\", \"test\"]\n    }}\n  ]\n}}\n",
            script_path.to_string_lossy(),
            tmp.path().to_string_lossy()
        ),
    )
    .expect("launch json should be written");

    let mut launch_cmd = cargo_bin_cmd!("launch-code");
    let launch_assert = launch_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("launch")
        .arg("--name")
        .arg("Python Demo")
        .arg("--mode")
        .arg("run")
        .arg("--launch-file")
        .arg(launch_path.to_string_lossy().to_string())
        .assert()
        .success()
        .stdout(contains("session_id="));

    let launch_output = String::from_utf8(launch_assert.get_output().stdout.clone())
        .expect("launch output should be utf8");
    let session_id = parse_session_id(&launch_output).expect("session id should be present");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .assert()
        .success()
        .stdout(contains("stopped"));
}
