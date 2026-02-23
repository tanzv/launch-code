use std::fs;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
fn launch_supports_env_file_and_python_interpreter_override() {
    if !python_available() {
        return;
    }
    if !cfg!(unix) {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path();

    let vscode_dir = workspace.join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

    let wrapper_path = workspace.join("python_wrapper.sh");
    fs::write(
        &wrapper_path,
        "#!/bin/sh\n\necho invoked > python.invoked\nexec python \"$@\"\n",
    )
    .expect("python wrapper should be written");

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&wrapper_path)
            .expect("wrapper metadata should exist")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper_path, perms).expect("wrapper should be executable");
    }

    let env_path = workspace.join(".env");
    fs::write(
        &env_path,
        "# example env\nFROM_ENVFILE=envfile\nONLY_ENVFILE=1\n",
    )
    .expect("env file should be written");

    let script_path = workspace.join("app.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let launch_path = vscode_dir.join("launch.json");
    fs::write(
        &launch_path,
        "{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {\n      \"name\": \"Python EnvFile Config\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"${workspaceFolder}/app.py\",\n      \"cwd\": \"${workspaceFolder}\",\n      \"envFile\": \"${workspaceFolder}/.env\",\n      \"env\": {\n        \"FROM_CONFIG\": \"config\",\n        \"FROM_ENVFILE\": \"override\"\n      },\n      \"python\": \"${workspaceFolder}/python_wrapper.sh\"\n    }\n  ]\n}\n",
    )
    .expect("launch json should be written");

    let mut launch_cmd = cargo_bin_cmd!("launch-code");
    let launch_assert = launch_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .arg("launch")
        .arg("--name")
        .arg("Python EnvFile Config")
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

    for _ in 0..20 {
        if workspace.join("python.invoked").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        workspace.join("python.invoked").exists(),
        "python wrapper should be invoked"
    );

    let state_path = workspace.join(".launch-code").join("state.json");
    let state_payload = fs::read_to_string(&state_path).expect("state file should exist");
    let state: Value = serde_json::from_str(&state_payload).expect("state should be valid json");

    let env_map = &state["sessions"][&session_id]["spec"]["env"];
    assert_eq!(env_map["FROM_ENVFILE"].as_str().unwrap(), "override");
    assert_eq!(env_map["ONLY_ENVFILE"].as_str().unwrap(), "1");
    assert_eq!(env_map["FROM_CONFIG"].as_str().unwrap(), "config");

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

#[test]
fn launch_env_null_unsets_inherited_variable() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path();
    let vscode_dir = workspace.join(".vscode");
    fs::create_dir_all(&vscode_dir).expect("vscode dir should exist");

    let script_path = workspace.join("app_unset_env.py");
    fs::write(
        &script_path,
        "import os\nimport pathlib\nimport time\n\nprobe = pathlib.Path('env_null_probe.txt')\nprobe.write_text('present' if 'LCODE_ENV_NULL_TEST' in os.environ else 'missing')\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let launch_path = vscode_dir.join("launch.json");
    fs::write(
        &launch_path,
        "{\n  \"version\": \"0.2.0\",\n  \"configurations\": [\n    {\n      \"name\": \"Python Env Null Config\",\n      \"type\": \"python\",\n      \"request\": \"launch\",\n      \"program\": \"${workspaceFolder}/app_unset_env.py\",\n      \"cwd\": \"${workspaceFolder}\",\n      \"env\": {\n        \"LCODE_ENV_NULL_TEST\": null\n      }\n    }\n  ]\n}\n",
    )
    .expect("launch json should be written");

    let mut launch_cmd = cargo_bin_cmd!("launch-code");
    let launch_assert = launch_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .env("LCODE_ENV_NULL_TEST", "present-in-parent")
        .arg("launch")
        .arg("--name")
        .arg("Python Env Null Config")
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

    let probe_path = workspace.join("env_null_probe.txt");
    for _ in 0..40 {
        if probe_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let probe = fs::read_to_string(&probe_path).expect("probe output should exist");
    assert_eq!(probe.trim(), "missing");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    stop_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .arg("stop")
        .arg("--id")
        .arg(session_id)
        .arg("--force")
        .assert()
        .success();
}
