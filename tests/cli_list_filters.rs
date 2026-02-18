use std::fs;
use std::net::TcpListener;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
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
fn list_supports_status_runtime_and_name_filters() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let run_script = tmp.path().join("run.py");
    let stop_script = tmp.path().join("stop.py");
    fs::write(&run_script, "import time\ntime.sleep(30)\n").expect("script should be written");
    fs::write(&stop_script, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_run_cmd = cargo_bin_cmd!("launch-code");
    let start_run_output = start_run_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("api-running")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(run_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_run_output.status.success(), "start should succeed");
    let running_id = parse_session_id(
        &String::from_utf8(start_run_output.stdout).expect("start stdout should be utf8"),
    )
    .expect("running session id should exist");

    let mut start_stop_cmd = cargo_bin_cmd!("launch-code");
    let start_stop_output = start_stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("worker-stopped")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(stop_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_stop_output.status.success(), "start should succeed");
    let stopped_id = parse_session_id(
        &String::from_utf8(start_stop_output.stdout).expect("start stdout should be utf8"),
    )
    .expect("stopped session id should exist");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&stopped_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");

    let mut list_running_cmd = cargo_bin_cmd!("launch-code");
    let list_running_output = list_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--status")
        .arg("running")
        .output()
        .expect("list should run");
    assert!(list_running_output.status.success(), "list should succeed");
    let list_running_text =
        String::from_utf8(list_running_output.stdout).expect("list output should be utf8");
    assert!(
        list_running_text.contains(&running_id),
        "running filter should include running session"
    );
    assert!(
        !list_running_text.contains(&stopped_id),
        "running filter should exclude stopped session"
    );

    let mut list_stopped_cmd = cargo_bin_cmd!("launch-code");
    let list_stopped_output = list_stopped_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--status")
        .arg("stopped")
        .output()
        .expect("list should run");
    assert!(list_stopped_output.status.success(), "list should succeed");
    let list_stopped_text =
        String::from_utf8(list_stopped_output.stdout).expect("list output should be utf8");
    assert!(
        list_stopped_text.contains(&stopped_id),
        "stopped filter should include stopped session"
    );
    assert!(
        !list_stopped_text.contains(&running_id),
        "stopped filter should exclude running session"
    );

    let mut list_name_cmd = cargo_bin_cmd!("launch-code");
    let list_name_output = list_name_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--name-contains")
        .arg("worker")
        .output()
        .expect("list should run");
    assert!(list_name_output.status.success(), "list should succeed");
    let list_name_text = String::from_utf8(list_name_output.stdout).expect("list output utf8");
    assert!(
        list_name_text.contains(&stopped_id),
        "name filter should include matching session"
    );
    assert!(
        !list_name_text.contains(&running_id),
        "name filter should exclude non-matching session"
    );

    let mut list_runtime_cmd = cargo_bin_cmd!("launch-code");
    let list_runtime_output = list_runtime_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--runtime")
        .arg("python")
        .output()
        .expect("list should run");
    assert!(list_runtime_output.status.success(), "list should succeed");
    let list_runtime_text =
        String::from_utf8(list_runtime_output.stdout).expect("list output should be utf8");
    assert!(
        list_runtime_text.contains(&running_id),
        "runtime filter should include running python session"
    );
    assert!(
        list_runtime_text.contains(&stopped_id),
        "runtime filter should include stopped python session"
    );

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&running_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(
        cleanup_output.status.success(),
        "cleanup stop should succeed"
    );
}

#[test]
fn list_supports_combined_filters_and_rich_columns() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("run.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_api_cmd = cargo_bin_cmd!("launch-code");
    let start_api_output = start_api_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("api-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_api_output.status.success(), "start should succeed");
    let api_id =
        parse_session_id(&String::from_utf8(start_api_output.stdout).expect("stdout utf8"))
            .expect("api session id should exist");

    let mut start_worker_cmd = cargo_bin_cmd!("launch-code");
    let start_worker_output = start_worker_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("worker-session")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_worker_output.status.success(), "start should succeed");
    let worker_id =
        parse_session_id(&String::from_utf8(start_worker_output.stdout).expect("stdout utf8"))
            .expect("worker session id should exist");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("list should run");
    assert!(list_output.status.success(), "list should succeed");
    let list_text = String::from_utf8(list_output.stdout).expect("list output should be utf8");
    assert!(
        list_text.contains(&api_id),
        "combined filter should include api session"
    );
    assert!(
        !list_text.contains(&worker_id),
        "combined filter should exclude worker session"
    );
    assert!(
        list_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ID") && line.contains("STATUS")),
        "list output should include a readable header row"
    );
    assert!(
        list_text.contains("api-session"),
        "list line should include session name"
    );
    assert!(
        list_text.contains("ENTRY"),
        "list output should include entry column header"
    );
    assert!(
        list_text.contains("DEBUG"),
        "list output should include debug column header"
    );
    assert!(
        list_text.contains("LINK"),
        "list output should include link column header"
    );

    for session_id in [api_id, worker_id] {
        let mut stop_cmd = cargo_bin_cmd!("launch-code");
        let stop_output = stop_cmd
            .env("LAUNCH_CODE_HOME", tmp.path())
            .arg("stop")
            .arg("--id")
            .arg(session_id)
            .arg("--force")
            .output()
            .expect("stop should run");
        assert!(stop_output.status.success(), "stop should succeed");
    }
}

#[test]
fn json_list_filters_return_filtered_items() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("run.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_running_cmd = cargo_bin_cmd!("launch-code");
    let start_running_output = start_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("json-running")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(
        start_running_output.status.success(),
        "start should succeed"
    );
    let running_id =
        parse_session_id(&String::from_utf8(start_running_output.stdout).expect("stdout utf8"))
            .expect("running session id should exist");

    let mut start_stopped_cmd = cargo_bin_cmd!("launch-code");
    let start_stopped_output = start_stopped_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("json-stopped")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(
        start_stopped_output.status.success(),
        "start should succeed"
    );
    let stopped_id =
        parse_session_id(&String::from_utf8(start_stopped_output.stdout).expect("stdout utf8"))
            .expect("stopped session id should exist");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&stopped_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");

    let mut list_json_cmd = cargo_bin_cmd!("launch-code");
    let list_json_output = list_json_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("list")
        .arg("--status")
        .arg("running")
        .output()
        .expect("list should run");
    assert!(list_json_output.status.success(), "list should succeed");
    let doc: Value = serde_json::from_slice(&list_json_output.stdout).expect("stdout json");
    let items = doc["items"].as_array().expect("items should be array");
    assert!(
        items.iter().any(|item| {
            item["id"].as_str() == Some(&running_id)
                && item["status"].as_str() == Some("running")
                && item["runtime"].as_str() == Some("python")
                && item["mode"].as_str() == Some("run")
        }),
        "running json list should include running session id"
    );
    assert!(
        !items
            .iter()
            .any(|item| item["id"].as_str() == Some(&stopped_id)),
        "running json list should exclude stopped session id"
    );

    let mut cleanup_cmd = cargo_bin_cmd!("launch-code");
    let cleanup_output = cleanup_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&running_id)
        .arg("--force")
        .output()
        .expect("cleanup stop should run");
    assert!(
        cleanup_output.status.success(),
        "cleanup stop should succeed"
    );
}

#[test]
fn list_shows_debug_endpoint_for_debug_sessions() {
    if !python_debug_ready() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("debug.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let port = TcpListener::bind("127.0.0.1:0")
        .expect("port should bind")
        .local_addr()
        .expect("addr should exist")
        .port();

    let mut debug_cmd = cargo_bin_cmd!("launch-code");
    let debug_output = debug_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("debug")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .arg("--name")
        .arg("debug-list")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--wait-for-client")
        .arg("true")
        .output()
        .expect("debug should run");
    assert!(debug_output.status.success(), "debug should succeed");
    let session_id =
        parse_session_id(&String::from_utf8(debug_output.stdout).expect("debug stdout utf8"))
            .expect("session id should exist");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--name-contains")
        .arg("debug-list")
        .output()
        .expect("list should run");
    assert!(list_output.status.success(), "list should succeed");
    let text = String::from_utf8(list_output.stdout).expect("list stdout utf8");
    assert!(
        text.contains(&session_id),
        "list should include debug session id"
    );
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.contains("MODE") && line.contains("DEBUG")),
        "list output should include mode and debug headers"
    );
    assert!(
        text.contains("127.0.0.1:"),
        "list should include debug endpoint value in the debug column"
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
