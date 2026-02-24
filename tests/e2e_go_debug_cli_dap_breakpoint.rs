use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn go_debug_ready() -> bool {
    let go_ready = std::process::Command::new("go")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !go_ready {
        return false;
    }

    std::process::Command::new("dlv")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

fn run_cli_json(home: &Path, args: &[String]) -> Value {
    let mut cmd = cargo_bin_cmd!("launch-code");
    cmd.env("LAUNCH_CODE_HOME", home);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("command should run");
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    serde_json::from_str::<Value>(&stdout).expect("stdout should be json")
}

fn run_cli_json_result(home: &Path, args: &[String]) -> Result<Value, String> {
    let mut cmd = cargo_bin_cmd!("launch-code");
    cmd.env("LAUNCH_CODE_HOME", home);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    serde_json::from_str::<Value>(&stdout).map_err(|err| err.to_string())
}

fn parse_session_id(doc: &Value) -> String {
    let message = doc["message"]
        .as_str()
        .expect("debug json message should be string");
    parse_field(message, "session_id")
        .expect("session_id should exist")
        .to_string()
}

#[test]
fn e2e_go_debug_cli_dap_keeps_session_running_across_multiple_commands() {
    if !go_debug_ready() {
        return;
    }

    let tmp = tempdir().expect("temp dir should exist");
    let script_path = tmp.path().join("main.go");
    let script = [
        "package main",
        "",
        "import (",
        "    \"fmt\"",
        "    \"time\"",
        ")",
        "",
        "func main() {",
        "    x := 41",
        "    fmt.Println(\"ready\", x)",
        "    time.Sleep(5 * time.Second)",
        "    x += 1",
        "    fmt.Println(x)",
        "}",
        "",
    ]
    .join("\n");
    fs::write(&script_path, script).expect("go script should be written");
    let breakpoint_line = 12u64;

    let debug_port = TcpListener::bind("127.0.0.1:0")
        .expect("port should bind")
        .local_addr()
        .expect("listener should expose local addr")
        .port();

    let debug_response = run_cli_json(
        tmp.path(),
        &[
            "--json".to_string(),
            "debug".to_string(),
            "--runtime".to_string(),
            "go".to_string(),
            "--entry".to_string(),
            "main.go".to_string(),
            "--cwd".to_string(),
            tmp.path().to_string_lossy().to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            debug_port.to_string(),
            "--wait-for-client".to_string(),
            "true".to_string(),
        ],
    );
    assert_eq!(debug_response["ok"], true);
    let session_id = parse_session_id(&debug_response);

    let threads_response = run_cli_json(
        tmp.path(),
        &[
            "--json".to_string(),
            "dap".to_string(),
            "threads".to_string(),
            "--id".to_string(),
            session_id.clone(),
            "--timeout-ms".to_string(),
            "5000".to_string(),
        ],
    );
    assert_eq!(threads_response["ok"], true);

    let status_after_threads = run_cli_json(
        tmp.path(),
        &[
            "--json".to_string(),
            "status".to_string(),
            "--id".to_string(),
            session_id.clone(),
        ],
    );
    assert_eq!(status_after_threads["ok"], true);
    assert_eq!(
        status_after_threads["session"]["status"],
        Value::String("running".to_string())
    );

    let mut breakpoints_doc: Option<Value> = None;
    for _ in 0..5 {
        let result = run_cli_json_result(
            tmp.path(),
            &[
                "--json".to_string(),
                "dap".to_string(),
                "breakpoints".to_string(),
                "--id".to_string(),
                session_id.clone(),
                "--path".to_string(),
                script_path.to_string_lossy().to_string(),
                "--line".to_string(),
                breakpoint_line.to_string(),
                "--timeout-ms".to_string(),
                "5000".to_string(),
            ],
        );
        match result {
            Ok(doc) => {
                breakpoints_doc = Some(doc);
                break;
            }
            Err(message) if message.contains("timeout waiting for response") => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(message) => panic!("dap breakpoints failed: {message}"),
        }
    }
    let breakpoints_response = breakpoints_doc.expect("dap breakpoints should eventually succeed");
    assert_eq!(breakpoints_response["ok"], true);
    assert_eq!(
        breakpoints_response["response"]["command"],
        Value::String("setBreakpoints".to_string())
    );
    assert_eq!(
        breakpoints_response["response"]["body"]["breakpoints"][0]["verified"],
        Value::Bool(true)
    );

    let status_after_breakpoints = run_cli_json(
        tmp.path(),
        &[
            "--json".to_string(),
            "status".to_string(),
            "--id".to_string(),
            session_id.clone(),
        ],
    );
    assert_eq!(status_after_breakpoints["ok"], true);
    assert_eq!(
        status_after_breakpoints["session"]["status"],
        Value::String("running".to_string())
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
}
