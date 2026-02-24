use std::fs;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::cargo::cargo_bin_cmd;
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
fn logs_can_tail_recent_lines() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("emit.py");
    fs::write(
        &script_path,
        "import time\nprint('line-1', flush=True)\nprint('line-2', flush=True)\nprint('line-3', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-tail")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("2")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(stdout.contains("line-2"), "tail should include line-2");
    assert!(stdout.contains("line-3"), "tail should include line-3");

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
fn logs_follow_streams_until_process_exit() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("follow.py");
    fs::write(
        &script_path,
        "import time\nprint('follow-1', flush=True)\ntime.sleep(0.3)\nprint('follow-2', flush=True)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-follow")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("100")
        .arg("--follow")
        .arg("--poll-ms")
        .arg("50")
        .output()
        .expect("logs follow should run");
    assert!(
        logs_output.status.success(),
        "logs follow should succeed and exit after process stops"
    );
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("follow-1"),
        "follow output should include first line"
    );
    assert!(
        stdout.contains("follow-2"),
        "follow output should include second line"
    );
}

#[test]
fn logs_can_filter_tail_by_contains() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("filter_tail.py");
    fs::write(
        &script_path,
        "import time\nprint('INFO startup', flush=True)\nprint('ERROR boom', flush=True)\nprint('INFO done', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-filter-tail")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("10")
        .arg("--contains")
        .arg("ERROR")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("ERROR boom"),
        "tail filter should include matching line"
    );
    assert!(
        !stdout.contains("INFO startup"),
        "tail filter should remove non-matching line"
    );
    assert!(
        !stdout.contains("INFO done"),
        "tail filter should remove non-matching line"
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
fn logs_follow_can_filter_ignore_case() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("filter_follow.py");
    fs::write(
        &script_path,
        "import time\nprint('INFO warmup', flush=True)\ntime.sleep(0.2)\nprint('Error one', flush=True)\ntime.sleep(0.2)\nprint('ERROR two', flush=True)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-filter-follow")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("0")
        .arg("--follow")
        .arg("--contains")
        .arg("error")
        .arg("--ignore-case")
        .arg("--poll-ms")
        .arg("50")
        .output()
        .expect("logs follow should run");
    assert!(logs_output.status.success(), "logs follow should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("Error one"),
        "follow filter should include first match"
    );
    assert!(
        stdout.contains("ERROR two"),
        "follow filter should include second match"
    );
    assert!(
        !stdout.contains("INFO warmup"),
        "follow filter should remove non-matching line"
    );
}

#[test]
fn logs_can_exclude_lines() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("exclude_tail.py");
    fs::write(
        &script_path,
        "import time\nprint('INFO keep', flush=True)\nprint('SECRET token=abc', flush=True)\nprint('WARN keep-too', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-exclude-tail")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("20")
        .arg("--exclude")
        .arg("SECRET")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("INFO keep"),
        "exclude should keep non-matching line"
    );
    assert!(
        stdout.contains("WARN keep-too"),
        "exclude should keep non-matching line"
    );
    assert!(
        !stdout.contains("SECRET token=abc"),
        "exclude should remove matching line"
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
fn logs_can_filter_with_regex() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("regex_tail.py");
    fs::write(
        &script_path,
        "import time\nprint('INFO one', flush=True)\nprint('ERROR E100', flush=True)\nprint('ERROR E200', flush=True)\nprint('WARN two', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-regex-tail")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("20")
        .arg("--regex")
        .arg("^ERROR\\s+E(100|200)$")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("ERROR E100"),
        "regex should include first matching line"
    );
    assert!(
        stdout.contains("ERROR E200"),
        "regex should include second matching line"
    );
    assert!(
        !stdout.contains("INFO one"),
        "regex should remove non-match line"
    );
    assert!(
        !stdout.contains("WARN two"),
        "regex should remove non-match line"
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
fn logs_can_exclude_with_regex() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("exclude_regex_tail.py");
    fs::write(
        &script_path,
        "import time\nprint('DEBUG ping', flush=True)\nprint('INFO keep', flush=True)\nprint('TRACE worker', flush=True)\nprint('ERROR keep', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-exclude-regex-tail")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("20")
        .arg("--exclude-regex")
        .arg("^(DEBUG|TRACE)")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("INFO keep"),
        "exclude regex should keep non-matching line"
    );
    assert!(
        stdout.contains("ERROR keep"),
        "exclude regex should keep non-matching line"
    );
    assert!(
        !stdout.contains("DEBUG ping"),
        "exclude regex should remove matching line"
    );
    assert!(
        !stdout.contains("TRACE worker"),
        "exclude regex should remove matching line"
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
fn logs_follow_can_exclude_with_regex() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("exclude_regex_follow.py");
    fs::write(
        &script_path,
        "import time\nprint('DEBUG warmup', flush=True)\ntime.sleep(0.6)\nprint('ERROR visible 1', flush=True)\ntime.sleep(0.2)\nprint('ERROR visible 2', flush=True)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-exclude-regex-follow")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("0")
        .arg("--follow")
        .arg("--exclude-regex")
        .arg("^DEBUG")
        .arg("--poll-ms")
        .arg("50")
        .output()
        .expect("logs follow should run");
    assert!(logs_output.status.success(), "logs follow should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("ERROR visible"),
        "follow exclude regex should keep non-matching line"
    );
    assert!(
        !stdout.contains("DEBUG warmup"),
        "follow exclude regex should remove matching line"
    );
}

#[test]
fn logs_supports_since_and_until_time_window_for_timestamped_lines() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("time_window.py");
    fs::write(
        &script_path,
        "import time\nnow=int(time.time())\nprint(f'{now-20} old-line', flush=True)\nprint(f'{now} keep-line', flush=True)\nprint(f'{now+20} future-line', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-time-window")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_secs();
    let since = now.saturating_sub(5);
    let until = now.saturating_add(5);

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("20")
        .arg("--since")
        .arg(since.to_string())
        .arg("--until")
        .arg(until.to_string())
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    assert!(
        stdout.contains("keep-line"),
        "time window should include in-range line"
    );
    assert!(
        !stdout.contains("old-line"),
        "time window should exclude line older than --since"
    );
    assert!(
        !stdout.contains("future-line"),
        "time window should exclude line newer than --until"
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
fn logs_can_prefix_output_with_timestamps() {
    if !python_available() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("timestamps.py");
    fs::write(
        &script_path,
        "import time\nprint('stamp-line', flush=True)\ntime.sleep(30)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("start")
        .arg("--name")
        .arg("log-timestamps")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(script_path.to_string_lossy().to_string())
        .arg("--cwd")
        .arg(tmp.path().to_string_lossy().to_string())
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let session_id = parse_session_id(&String::from_utf8(start_output.stdout).unwrap())
        .expect("session id should exist");

    thread::sleep(Duration::from_millis(300));

    let mut logs_cmd = cargo_bin_cmd!("launch-code");
    let logs_output = logs_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("logs")
        .arg("--id")
        .arg(&session_id)
        .arg("--tail")
        .arg("5")
        .arg("--timestamps")
        .output()
        .expect("logs should run");
    assert!(logs_output.status.success(), "logs should succeed");
    let stdout = String::from_utf8(logs_output.stdout).expect("logs stdout utf8");
    let line = stdout
        .lines()
        .find(|item| item.contains("stamp-line"))
        .expect("output should include stamp-line");
    assert!(
        line.starts_with('[') && line.contains("] stamp-line"),
        "timestamps mode should prefix lines with unix timestamp brackets"
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
