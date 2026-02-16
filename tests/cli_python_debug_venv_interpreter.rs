use std::fs;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

#[test]
fn debug_uses_venv_interpreter_for_debugpy_check_even_without_path() {
    if !cfg!(unix) {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let workspace = tmp.path();

    let venv_bin_dir = workspace.join(".venv").join("bin");
    fs::create_dir_all(&venv_bin_dir).expect("venv bin dir should exist");

    let venv_python = venv_bin_dir.join("python");
    fs::write(
        &venv_python,
        "#!/bin/sh\n\nif [ \"$1\" = \"-c\" ]; then\n  exit 0\nfi\n\nif [ \"$1\" = \"-m\" ] && [ \"$2\" = \"debugpy\" ]; then\n  echo invoked > python.invoked\n  exec /bin/sleep 30\nfi\n\nexit 1\n",
    )
    .expect("venv python wrapper should be written");

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&venv_python)
            .expect("wrapper metadata should exist")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&venv_python, perms).expect("wrapper should be executable");
    }

    let script_path = workspace.join("app.py");
    fs::write(&script_path, "print('hello')\n").expect("script should be written");

    let mut debug_cmd = cargo_bin_cmd!("launch-code");
    let debug_assert = debug_cmd
        .env("LAUNCH_CODE_HOME", workspace)
        .env("PATH", "")
        .arg("debug")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(workspace.to_string_lossy().to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("5678")
        .arg("--wait-for-client")
        .arg("true")
        .assert()
        .success()
        .stdout(contains("session_id="));

    for _ in 0..20 {
        if workspace.join("python.invoked").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(workspace.join("python.invoked").exists());

    let output = String::from_utf8(debug_assert.get_output().stdout.clone())
        .expect("debug output should be utf8");
    let session_id = parse_field(&output, "session_id")
        .expect("session id should exist")
        .to_string();

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
