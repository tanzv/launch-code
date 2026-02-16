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
fn launch_runs_pre_and_post_tasks() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path();
    let vscode_dir = workspace.join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

    fs::write(
        workspace.join("pre_task.py"),
        "from pathlib import Path\nPath('pre.flag').write_text('ok')\n",
    )
    .expect("pre task should be written");
    fs::write(
        workspace.join("post_task.py"),
        "from pathlib import Path\nPath('post.flag').write_text('ok')\n",
    )
    .expect("post task should be written");
    fs::write(workspace.join("app.py"), "import time\ntime.sleep(30)\n")
        .expect("app script should be written");

    let launch_file = vscode_dir.join("launch.json");
    fs::write(
        &launch_file,
        "{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {\n      \"name\": \"Python With Tasks\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"${workspaceFolder}/app.py\",\n      \"cwd\": \"${workspaceFolder}\",\n      \"preLaunchTask\": \"python pre_task.py\",\n      \"postStopTask\": \"python post_task.py\"\n    }\n  ]\n}\n",
    )
    .expect("launch json should be written");

    let mut launch_cmd = cargo_bin_cmd!("launch-code");
    let launch_assert = launch_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .arg("launch")
        .arg("--name")
        .arg("Python With Tasks")
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

    assert!(
        workspace.join("pre.flag").exists(),
        "pre launch task should create pre.flag"
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .arg("stop")
        .arg("--id")
        .arg(session_id)
        .assert()
        .success()
        .stdout(contains("stopped"));

    assert!(
        workspace.join("post.flag").exists(),
        "post stop task should create post.flag"
    );
}
