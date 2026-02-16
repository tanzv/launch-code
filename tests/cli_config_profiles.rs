use std::fs;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn python_available() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok()
}

fn python_debug_ready() -> bool {
    if !python_available() {
        return false;
    }
    std::process::Command::new("python")
        .arg("-c")
        .arg("import debugpy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn parse_session_id(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id=").map(ToString::to_string))
}

#[test]
fn config_profile_roundtrip_save_list_show_run_delete() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("profile_app.py");
    fs::write(
        &script_path,
        "import time\nprint('profile-start', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("python-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--arg")
        .arg("from-profile")
        .arg("--env")
        .arg("PROFILE_ENV=1")
        .arg("--managed")
        .output()
        .expect("config save should run");
    assert!(save_output.status.success(), "config save should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("list")
        .output()
        .expect("config list should run");
    assert!(list_output.status.success(), "config list should succeed");
    let list_stdout = String::from_utf8(list_output.stdout).expect("list stdout should be utf8");
    assert!(
        list_stdout.contains("python-profile"),
        "config list should include saved profile"
    );

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("show")
        .arg("--name")
        .arg("python-profile")
        .output()
        .expect("config show should run");
    assert!(show_output.status.success(), "config show should succeed");
    let show_stdout = String::from_utf8(show_output.stdout).expect("show stdout should be utf8");
    assert!(
        show_stdout.contains("\"name\": \"python-profile\""),
        "config show should include profile name"
    );
    assert!(
        show_stdout.contains("\"runtime\": \"python\""),
        "config show should include runtime"
    );

    let mut run_cmd = cargo_bin_cmd!("launch-code");
    let run_output = run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("run")
        .arg("--name")
        .arg("python-profile")
        .output()
        .expect("config run should execute");
    assert!(run_output.status.success(), "config run should succeed");
    let run_stdout = String::from_utf8(run_output.stdout).expect("run stdout should be utf8");
    let session_id = parse_session_id(&run_stdout).expect("session id should be present");

    thread::sleep(Duration::from_millis(250));

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");

    let mut delete_cmd = cargo_bin_cmd!("launch-code");
    let delete_output = delete_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("delete")
        .arg("--name")
        .arg("python-profile")
        .output()
        .expect("config delete should run");
    assert!(
        delete_output.status.success(),
        "config delete should succeed"
    );

    let mut list_after_delete_cmd = cargo_bin_cmd!("launch-code");
    let list_after_delete_output = list_after_delete_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("list")
        .output()
        .expect("config list after delete should run");
    assert!(
        list_after_delete_output.status.success(),
        "config list should succeed after delete"
    );
    let list_after_delete_stdout =
        String::from_utf8(list_after_delete_output.stdout).expect("list stdout should be utf8");
    assert!(
        !list_after_delete_stdout.contains("python-profile"),
        "config list should not include deleted profile"
    );
}

#[test]
fn config_run_can_override_mode_to_debug() {
    if !python_debug_ready() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("profile_debug_override.py");
    fs::write(
        &script_path,
        "import time\nprint('profile-debug', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("python-profile-debug-override")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--mode")
        .arg("run")
        .output()
        .expect("config save should run");
    assert!(save_output.status.success(), "config save should succeed");

    let mut run_cmd = cargo_bin_cmd!("launch-code");
    let run_output = run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("run")
        .arg("--name")
        .arg("python-profile-debug-override")
        .arg("--mode")
        .arg("debug")
        .output()
        .expect("config run should execute");
    assert!(run_output.status.success(), "config run should succeed");
    let run_stdout = String::from_utf8(run_output.stdout).expect("run stdout should be utf8");
    assert!(
        run_stdout.contains("debug_port="),
        "mode override to debug should include debug metadata"
    );
    let session_id = parse_session_id(&run_stdout).expect("session id should be present");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
}

#[test]
fn config_run_can_force_managed_override() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("profile_managed_override.py");
    fs::write(
        &script_path,
        "import time\nprint('profile-managed', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("python-profile-managed-override")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("config save should run");
    assert!(save_output.status.success(), "config save should succeed");

    let mut run_cmd = cargo_bin_cmd!("launch-code");
    let run_output = run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("run")
        .arg("--name")
        .arg("python-profile-managed-override")
        .arg("--managed")
        .output()
        .expect("config run should execute");
    assert!(run_output.status.success(), "config run should succeed");
    let run_stdout = String::from_utf8(run_output.stdout).expect("run stdout should be utf8");
    let session_id = parse_session_id(&run_stdout).expect("session id should be present");

    let state_path = tmp.path().join(".launch-code").join("state.json");
    let state_payload = fs::read_to_string(state_path).expect("state file should exist");
    let state_doc: serde_json::Value =
        serde_json::from_str(&state_payload).expect("state should be valid json");
    let managed = state_doc["sessions"][&session_id]["spec"]["managed"]
        .as_bool()
        .expect("managed flag should exist");
    assert!(
        managed,
        "managed override should persist on created session"
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
}

#[test]
fn config_export_and_import_support_merge_and_replace() {
    let source = tempdir().expect("source dir should exist");
    let target = tempdir().expect("target dir should exist");
    let bundle_dir = tempdir().expect("bundle dir should exist");
    let bundle_path = bundle_dir.path().join("profiles.json");

    let src_entry = source.path().join("app.py");
    fs::write(&src_entry, "print('source')\n").expect("source entry should be written");

    let mut save_source = cargo_bin_cmd!("launch-code");
    let save_source_output = save_source
        .env("LAUNCH_CODE_HOME", source.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("from-source")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(src_entry.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(source.path().to_string_lossy().to_string())
        .output()
        .expect("source save should run");
    assert!(
        save_source_output.status.success(),
        "source config save should succeed"
    );

    let mut export_cmd = cargo_bin_cmd!("launch-code");
    let export_output = export_cmd
        .env("LAUNCH_CODE_HOME", source.path())
        .arg("config")
        .arg("export")
        .arg("--file")
        .arg(bundle_path.to_string_lossy().to_string())
        .output()
        .expect("config export should run");
    assert!(
        export_output.status.success(),
        "config export should succeed"
    );
    assert!(bundle_path.exists(), "exported bundle file should exist");

    let target_entry = target.path().join("target.py");
    fs::write(&target_entry, "print('target')\n").expect("target entry should be written");

    let mut save_target = cargo_bin_cmd!("launch-code");
    let save_target_output = save_target
        .env("LAUNCH_CODE_HOME", target.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("from-target")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(target_entry.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(target.path().to_string_lossy().to_string())
        .output()
        .expect("target save should run");
    assert!(
        save_target_output.status.success(),
        "target config save should succeed"
    );

    let mut import_merge_cmd = cargo_bin_cmd!("launch-code");
    let import_merge_output = import_merge_cmd
        .env("LAUNCH_CODE_HOME", target.path())
        .arg("config")
        .arg("import")
        .arg("--file")
        .arg(bundle_path.to_string_lossy().to_string())
        .output()
        .expect("config import merge should run");
    assert!(
        import_merge_output.status.success(),
        "config import merge should succeed"
    );

    let mut list_after_merge_cmd = cargo_bin_cmd!("launch-code");
    let list_after_merge_output = list_after_merge_cmd
        .env("LAUNCH_CODE_HOME", target.path())
        .arg("config")
        .arg("list")
        .output()
        .expect("config list should run");
    assert!(
        list_after_merge_output.status.success(),
        "config list should succeed"
    );
    let merged_stdout =
        String::from_utf8(list_after_merge_output.stdout).expect("list stdout should be utf8");
    assert!(
        merged_stdout.contains("from-source"),
        "merge import should include source profile"
    );
    assert!(
        merged_stdout.contains("from-target"),
        "merge import should keep existing profile"
    );

    let mut import_replace_cmd = cargo_bin_cmd!("launch-code");
    let import_replace_output = import_replace_cmd
        .env("LAUNCH_CODE_HOME", target.path())
        .arg("config")
        .arg("import")
        .arg("--file")
        .arg(bundle_path.to_string_lossy().to_string())
        .arg("--replace")
        .output()
        .expect("config import replace should run");
    assert!(
        import_replace_output.status.success(),
        "config import replace should succeed"
    );

    let mut list_after_replace_cmd = cargo_bin_cmd!("launch-code");
    let list_after_replace_output = list_after_replace_cmd
        .env("LAUNCH_CODE_HOME", target.path())
        .arg("config")
        .arg("list")
        .output()
        .expect("config list after replace should run");
    assert!(
        list_after_replace_output.status.success(),
        "config list should succeed after replace"
    );
    let replaced_stdout =
        String::from_utf8(list_after_replace_output.stdout).expect("list stdout should be utf8");
    assert!(
        replaced_stdout.contains("from-source"),
        "replace import should include source profile"
    );
    assert!(
        !replaced_stdout.contains("from-target"),
        "replace import should remove old profile"
    );
}

#[test]
fn config_validate_checks_profile_entry_and_cwd() {
    let tmp = tempdir().expect("temp dir should exist");
    let valid_entry = tmp.path().join("valid.py");
    fs::write(&valid_entry, "print('ok')\n").expect("valid entry should be written");

    let mut save_valid = cargo_bin_cmd!("launch-code");
    let save_valid_output = save_valid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("valid-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(valid_entry.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save valid profile should run");
    assert!(
        save_valid_output.status.success(),
        "save valid profile should succeed"
    );

    let mut validate_valid = cargo_bin_cmd!("launch-code");
    let validate_valid_output = validate_valid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("validate")
        .arg("--name")
        .arg("valid-profile")
        .output()
        .expect("validate valid profile should run");
    assert!(
        validate_valid_output.status.success(),
        "validate valid profile should succeed"
    );
    let validate_valid_stdout =
        String::from_utf8(validate_valid_output.stdout).expect("stdout should be utf8");
    assert!(
        validate_valid_stdout.contains("valid=true"),
        "validate output should indicate success"
    );

    let mut save_invalid = cargo_bin_cmd!("launch-code");
    let save_invalid_output = save_invalid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("invalid-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg("missing.py")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save invalid profile should run");
    assert!(
        save_invalid_output.status.success(),
        "save invalid profile should succeed"
    );

    let mut validate_invalid = cargo_bin_cmd!("launch-code");
    let validate_invalid_output = validate_invalid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("validate")
        .arg("--name")
        .arg("invalid-profile")
        .output()
        .expect("validate invalid profile should run");
    assert!(
        !validate_invalid_output.status.success(),
        "validate invalid profile should fail"
    );
    let validate_invalid_stderr =
        String::from_utf8(validate_invalid_output.stderr).expect("stderr should be utf8");
    assert!(
        validate_invalid_stderr.contains("profile validation failed"),
        "validation error should be reported"
    );
}

#[test]
fn config_validate_supports_rust_runtime_profiles() {
    let tmp = tempdir().expect("temp dir should exist");
    let cargo_manifest = tmp.path().join("Cargo.toml");
    fs::write(
        &cargo_manifest,
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo manifest should be written");

    let mut save_cmd = cargo_bin_cmd!("launch-code");
    let save_output = save_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("rust-profile")
        .arg("--runtime")
        .arg("rust")
        .arg("--entry")
        .arg("demo-bin")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save rust profile should run");
    assert!(
        save_output.status.success(),
        "save rust profile should succeed"
    );

    let mut validate_cmd = cargo_bin_cmd!("launch-code");
    let validate_output = validate_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("validate")
        .arg("--name")
        .arg("rust-profile")
        .output()
        .expect("validate rust profile should run");
    assert!(
        validate_output.status.success(),
        "validate rust profile should succeed"
    );
    let stdout = String::from_utf8(validate_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("valid=true"),
        "validate output should indicate success"
    );
}

#[test]
fn config_validate_all_succeeds_for_valid_profiles() {
    let tmp = tempdir().expect("temp dir should exist");
    let entry_a = tmp.path().join("a.py");
    let entry_b = tmp.path().join("b.py");
    fs::write(&entry_a, "print('a')\n").expect("entry a should be written");
    fs::write(&entry_b, "print('b')\n").expect("entry b should be written");

    let mut save_a = cargo_bin_cmd!("launch-code");
    let save_a_output = save_a
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("profile-a")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(entry_a.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save profile a should run");
    assert!(
        save_a_output.status.success(),
        "save profile a should succeed"
    );

    let mut save_b = cargo_bin_cmd!("launch-code");
    let save_b_output = save_b
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("profile-b")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(entry_b.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save profile b should run");
    assert!(
        save_b_output.status.success(),
        "save profile b should succeed"
    );

    let mut validate_all = cargo_bin_cmd!("launch-code");
    let validate_all_output = validate_all
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("validate")
        .arg("--all")
        .output()
        .expect("validate all should run");
    assert!(
        validate_all_output.status.success(),
        "validate --all should succeed for valid profiles"
    );
    let stdout = String::from_utf8(validate_all_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("validated_profiles=2"),
        "validate --all should report validated profile count"
    );
}

#[test]
fn config_validate_all_fails_if_any_profile_is_invalid() {
    let tmp = tempdir().expect("temp dir should exist");
    let entry_valid = tmp.path().join("ok.py");
    fs::write(&entry_valid, "print('ok')\n").expect("valid entry should be written");

    let mut save_valid = cargo_bin_cmd!("launch-code");
    let save_valid_output = save_valid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("ok-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(entry_valid.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save valid profile should run");
    assert!(
        save_valid_output.status.success(),
        "save valid profile should succeed"
    );

    let mut save_invalid = cargo_bin_cmd!("launch-code");
    let save_invalid_output = save_invalid
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("save")
        .arg("--name")
        .arg("bad-profile")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg("missing.py")
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("save invalid profile should run");
    assert!(
        save_invalid_output.status.success(),
        "save invalid profile should succeed"
    );

    let mut validate_all = cargo_bin_cmd!("launch-code");
    let validate_all_output = validate_all
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("config")
        .arg("validate")
        .arg("--all")
        .output()
        .expect("validate all should run");
    assert!(
        !validate_all_output.status.success(),
        "validate --all should fail when one profile is invalid"
    );
    let stderr = String::from_utf8(validate_all_output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("bad-profile"),
        "validate --all failure should include failing profile name"
    );
}
