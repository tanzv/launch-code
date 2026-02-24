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
fn running_command_lists_only_running_sessions() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let run_script = tmp.path().join("running.py");
    let stop_script = tmp.path().join("stopped.py");
    fs::write(&run_script, "import time\ntime.sleep(30)\n").expect("script should be written");
    fs::write(&stop_script, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut start_running_cmd = cargo_bin_cmd!("launch-code");
    let running_output = start_running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("running-only")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(run_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(running_output.status.success(), "start should succeed");
    let running_id =
        parse_session_id(&String::from_utf8(running_output.stdout).expect("stdout utf8"))
            .expect("running session id should exist");

    let mut start_stopped_cmd = cargo_bin_cmd!("launch-code");
    let stopped_output = start_stopped_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("running-stop")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(stop_script.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(stopped_output.status.success(), "start should succeed");
    let stopped_id =
        parse_session_id(&String::from_utf8(stopped_output.stdout).expect("stdout utf8"))
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

    let mut running_cmd = cargo_bin_cmd!("launch-code");
    let running_list_output = running_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .output()
        .expect("running should run");
    assert!(
        running_list_output.status.success(),
        "running command should succeed"
    );
    let text = String::from_utf8(running_list_output.stdout).expect("stdout utf8");
    let running_prefix: String = running_id.chars().take(12).collect();
    let stopped_prefix: String = stopped_id.chars().take(12).collect();
    assert!(
        text.contains(&running_prefix),
        "running command should include running session in compact id form"
    );
    assert!(
        !text.contains(&stopped_prefix),
        "running command should exclude stopped session from compact output"
    );
    assert!(
        text.lines().next().is_some_and(|line| {
            line.contains("ID")
                && line.contains("STATUS")
                && line.contains("NAME")
                && line.contains("LINK")
        }),
        "running command should render a compact readable header"
    );
    assert!(
        !text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ENTRY")),
        "running compact view should not include ENTRY column by default"
    );

    let mut running_wide_cmd = cargo_bin_cmd!("launch-code");
    let running_wide_output = running_wide_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--format")
        .arg("wide")
        .output()
        .expect("running --format wide should run");
    assert!(
        running_wide_output.status.success(),
        "running --format wide should succeed"
    );
    let running_wide_text = String::from_utf8(running_wide_output.stdout).expect("stdout utf8");
    assert!(
        running_wide_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ENTRY") && line.contains("RESTARTS")),
        "running --format wide should include full columns"
    );

    let mut running_default_alias_cmd = cargo_bin_cmd!("launch-code");
    let running_default_alias_output = running_default_alias_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--format")
        .arg("default")
        .output()
        .expect("running --format default should run");
    assert!(
        running_default_alias_output.status.success(),
        "running --format default should succeed"
    );
    let running_default_alias_text =
        String::from_utf8(running_default_alias_output.stdout).expect("stdout utf8");
    assert!(
        running_default_alias_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ENTRY") && line.contains("RESTARTS")),
        "running --format default alias should map to wide columns"
    );

    let mut running_short_alias_cmd = cargo_bin_cmd!("launch-code");
    let running_short_alias_output = running_short_alias_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--format")
        .arg("short")
        .output()
        .expect("running --format short should run");
    assert!(
        running_short_alias_output.status.success(),
        "running --format short should succeed"
    );
    let running_short_alias_text =
        String::from_utf8(running_short_alias_output.stdout).expect("stdout utf8");
    assert!(
        running_short_alias_text
            .lines()
            .next()
            .is_some_and(|line| !line.contains("ENTRY") && line.contains("NAME")),
        "running --format short alias should map to compact columns"
    );

    let mut running_short_id_len_cmd = cargo_bin_cmd!("launch-code");
    let running_short_id_len_output = running_short_id_len_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--short-id-len")
        .arg("8")
        .arg("--no-headers")
        .output()
        .expect("running --short-id-len should run");
    assert!(
        running_short_id_len_output.status.success(),
        "running --short-id-len should succeed"
    );
    let running_short_id_len_text =
        String::from_utf8(running_short_id_len_output.stdout).expect("stdout utf8");
    let first_id_token = running_short_id_len_text
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .expect("compact row should contain id token");
    assert_eq!(first_id_token.len(), 8);
    let running_prefix_8: String = running_id.chars().take(8).collect();
    assert_eq!(first_id_token, running_prefix_8);

    let mut running_no_headers_cmd = cargo_bin_cmd!("launch-code");
    let running_no_headers_output = running_no_headers_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--no-headers")
        .output()
        .expect("running --no-headers should run");
    assert!(
        running_no_headers_output.status.success(),
        "running --no-headers should succeed"
    );
    let running_no_headers_text =
        String::from_utf8(running_no_headers_output.stdout).expect("stdout utf8");
    assert!(
        running_no_headers_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("running-only")),
        "running --no-headers should start from session rows"
    );
    assert!(
        !running_no_headers_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("STATUS")),
        "running --no-headers should omit header row"
    );

    let mut running_quiet_cmd = cargo_bin_cmd!("launch-code");
    let running_quiet_output = running_quiet_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("-q")
        .output()
        .expect("running --quiet should run");
    assert!(
        running_quiet_output.status.success(),
        "running --quiet should succeed"
    );
    let quiet_text = String::from_utf8(running_quiet_output.stdout).expect("stdout utf8");
    assert!(
        quiet_text.lines().any(|line| line.trim() == running_id),
        "running --quiet should print raw session ids"
    );
    assert!(
        !quiet_text.contains("STATUS"),
        "running --quiet should not print a table header"
    );

    let mut running_id_format_cmd = cargo_bin_cmd!("launch-code");
    let running_id_format_output = running_id_format_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("running")
        .arg("--format")
        .arg("id")
        .output()
        .expect("running --format id should run");
    assert!(
        running_id_format_output.status.success(),
        "running --format id should succeed"
    );
    let running_id_format_text =
        String::from_utf8(running_id_format_output.stdout).expect("stdout utf8");
    assert!(
        running_id_format_text
            .lines()
            .any(|line| line.trim() == running_id),
        "running --format id should print full session ids"
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

    let mut compact_cmd = cargo_bin_cmd!("launch-code");
    let compact_output = compact_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--format")
        .arg("compact")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("compact list should run");
    assert!(
        compact_output.status.success(),
        "compact list should succeed"
    );
    let compact_text = String::from_utf8(compact_output.stdout).expect("compact stdout utf8");
    assert!(
        compact_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ID") && line.contains("NAME")),
        "compact list should include compact header"
    );
    assert!(
        !compact_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("ENTRY")),
        "compact list header should omit ENTRY column"
    );

    let mut compact_alias_cmd = cargo_bin_cmd!("launch-code");
    let compact_alias_output = compact_alias_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--format")
        .arg("short")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("list --format short should run");
    assert!(
        compact_alias_output.status.success(),
        "list --format short should succeed"
    );
    let compact_alias_text = String::from_utf8(compact_alias_output.stdout).expect("stdout utf8");
    assert!(
        compact_alias_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("NAME") && !line.contains("ENTRY")),
        "list --format short alias should map to compact columns"
    );

    let mut compact_no_headers_cmd = cargo_bin_cmd!("launch-code");
    let compact_no_headers_output = compact_no_headers_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--format")
        .arg("compact")
        .arg("--no-headers")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("list compact --no-headers should run");
    assert!(
        compact_no_headers_output.status.success(),
        "list compact --no-headers should succeed"
    );
    let compact_no_headers_text =
        String::from_utf8(compact_no_headers_output.stdout).expect("no-header stdout utf8");
    assert!(
        compact_no_headers_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("api-session")),
        "list --no-headers should start from session rows"
    );
    assert!(
        !compact_no_headers_text
            .lines()
            .next()
            .is_some_and(|line| line.contains("STATUS")),
        "list --no-headers should omit header row"
    );

    let mut list_id_cmd = cargo_bin_cmd!("launch-code");
    let list_id_output = list_id_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--format")
        .arg("id")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("list --format id should run");
    assert!(
        list_id_output.status.success(),
        "list --format id should succeed"
    );
    let list_id_text = String::from_utf8(list_id_output.stdout).expect("id list stdout utf8");
    assert!(
        list_id_text.lines().any(|line| line.trim() == api_id),
        "list --format id should include full session id"
    );
    assert!(
        !list_id_text.contains("STATUS"),
        "list --format id should not print table headers"
    );

    let mut list_short_id_len_cmd = cargo_bin_cmd!("launch-code");
    let list_short_id_len_output = list_short_id_len_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("list")
        .arg("--format")
        .arg("compact")
        .arg("--short-id-len")
        .arg("16")
        .arg("--no-headers")
        .arg("--status")
        .arg("running")
        .arg("--runtime")
        .arg("python")
        .arg("--name-contains")
        .arg("api")
        .output()
        .expect("list --short-id-len should run");
    assert!(
        list_short_id_len_output.status.success(),
        "list --short-id-len should succeed"
    );
    let list_short_id_len_text =
        String::from_utf8(list_short_id_len_output.stdout).expect("stdout utf8");
    let first_id_token = list_short_id_len_text
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .expect("compact row should contain id token");
    assert_eq!(first_id_token.len(), 16);
    let api_prefix_16: String = api_id.chars().take(16).collect();
    assert_eq!(first_id_token, api_prefix_16);

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

#[test]
fn list_and_running_support_sort_and_limit_in_json_output() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("run.py");
    fs::write(&script_path, "import time\ntime.sleep(30)\n").expect("script should be written");

    let mut session_ids = Vec::new();
    for name in ["zeta-session", "alpha-session", "mid-session"] {
        let mut start_cmd = cargo_bin_cmd!("launch-code");
        let start_output = start_cmd
            .env("LAUNCH_CODE_HOME", tmp.path())
            .arg("start")
            .arg("--name")
            .arg(name)
            .arg("--runtime")
            .arg("python")
            .arg("--entry")
            .arg(script_path.to_string_lossy().to_string())
            .arg("--cwd")
            .arg(tmp.path().to_string_lossy().to_string())
            .output()
            .expect("start should run");
        assert!(start_output.status.success(), "start should succeed");
        let session_id =
            parse_session_id(&String::from_utf8(start_output.stdout).expect("stdout utf8"))
                .expect("session id should exist");
        session_ids.push(session_id);
    }

    let mut list_json_cmd = cargo_bin_cmd!("launch-code");
    let list_json_output = list_json_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("list")
        .arg("--status")
        .arg("running")
        .arg("--sort")
        .arg("name")
        .arg("--limit")
        .arg("2")
        .output()
        .expect("json list should run");
    assert!(
        list_json_output.status.success(),
        "json list should succeed"
    );
    let list_doc: Value = serde_json::from_slice(&list_json_output.stdout).expect("stdout json");
    let list_items = list_doc["items"].as_array().expect("items should be array");
    assert_eq!(list_items.len(), 2, "list --limit should cap result count");
    assert_eq!(list_items[0]["name"], "alpha-session");
    assert_eq!(list_items[1]["name"], "mid-session");

    let mut running_json_cmd = cargo_bin_cmd!("launch-code");
    let running_json_output = running_json_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("running")
        .arg("--sort")
        .arg("name")
        .arg("--limit")
        .arg("1")
        .output()
        .expect("json running should run");
    assert!(
        running_json_output.status.success(),
        "json running should succeed"
    );
    let running_doc: Value =
        serde_json::from_slice(&running_json_output.stdout).expect("stdout json");
    let running_items = running_doc["items"]
        .as_array()
        .expect("items should be array");
    assert_eq!(
        running_items.len(),
        1,
        "running --limit should cap result count"
    );
    assert_eq!(running_items[0]["name"], "alpha-session");

    for session_id in session_ids {
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
