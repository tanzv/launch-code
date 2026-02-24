use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

fn python_debug_ready(python_bin: &str) -> bool {
    let python_ok = std::process::Command::new(python_bin)
        .arg("--version")
        .output()
        .is_ok();
    if !python_ok {
        return false;
    }

    std::process::Command::new(python_bin)
        .arg("-c")
        .arg("import debugpy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn discover_python_with_debugpy() -> Option<String> {
    if let Ok(explicit) = std::env::var("LAUNCH_CODE_TEST_PYTHON_BIN") {
        if python_debug_ready(&explicit) {
            return Some(explicit);
        }
        return None;
    }

    for candidate in ["python3", "python"] {
        if python_debug_ready(candidate) {
            return Some(candidate.to_string());
        }
    }

    None
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

#[test]
fn e2e_python_debug_cli_dap_can_set_breakpoint_and_capture_stacktrace() {
    let tmp = tempdir().expect("temp dir should exist");
    let python_bin = match discover_python_with_debugpy() {
        Some(value) => value,
        None => return,
    };

    let script_path = tmp.path().join("debug_target.py");
    let script = [
        "import time",
        "def run():",
        "    # Give DAP setup enough time to apply breakpoints after attach.",
        "    time.sleep(2)",
        "    value = 41",
        "    value += 1  # breakpoint",
        "    print(value, flush=True)",
        "    time.sleep(30)",
        "run()",
        "",
    ]
    .join("\n");
    fs::write(&script_path, script).expect("script should be written");
    let breakpoint_line = 6u64;

    let debug_port = TcpListener::bind("127.0.0.1:0")
        .expect("port should bind")
        .local_addr()
        .expect("listener should expose local addr")
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
        .arg("--env")
        .arg(format!("PYTHON_BIN={python_bin}"))
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(debug_port.to_string())
        .arg("--wait-for-client")
        .arg("true")
        .output()
        .expect("debug should run");
    assert!(debug_output.status.success(), "debug should succeed");
    let debug_stdout = String::from_utf8(debug_output.stdout).expect("debug stdout utf8");
    let session_id = parse_field(&debug_stdout, "session_id")
        .expect("session_id should exist")
        .to_string();

    let setup_file = tmp.path().join("dap_setup.json");
    let setup_payload = json!([
        {
            "command": "initialize",
            "arguments": {
                "clientID": "launch-code-test",
                "adapterID": "python",
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true
            }
        },
        {
            "command": "attach",
            "arguments": {
                "justMyCode": false
            }
        },
        {
            "command": "setBreakpoints",
            "arguments": {
                "source": { "path": script_path.to_string_lossy().to_string() },
                "breakpoints": [{ "line": breakpoint_line }]
            }
        },
        {
            "command": "configurationDone",
            "arguments": {}
        }
    ]);
    fs::write(
        &setup_file,
        serde_json::to_string_pretty(&setup_payload).expect("serialize setup payload"),
    )
    .expect("setup payload should be written");

    let setup_response = run_cli_json(
        tmp.path(),
        &[
            "dap".to_string(),
            "batch".to_string(),
            "--id".to_string(),
            session_id.clone(),
            "--file".to_string(),
            setup_file.to_string_lossy().to_string(),
            "--timeout-ms".to_string(),
            "5000".to_string(),
        ],
    );
    assert_eq!(setup_response["ok"], true);
    let set_breakpoints = setup_response["responses"]
        .as_array()
        .and_then(|responses| {
            responses
                .iter()
                .find(|response| response["command"] == "setBreakpoints")
        })
        .expect("setBreakpoints response should exist");
    assert_eq!(
        set_breakpoints["body"]["breakpoints"][0]["verified"], true,
        "breakpoint should be verified"
    );

    let threads_response = run_cli_json(
        tmp.path(),
        &[
            "dap".to_string(),
            "threads".to_string(),
            "--id".to_string(),
            session_id.clone(),
            "--timeout-ms".to_string(),
            "5000".to_string(),
        ],
    );
    assert_eq!(threads_response["ok"], true);
    let thread_id = threads_response["response"]["body"]["threads"]
        .as_array()
        .and_then(|threads| threads.first())
        .and_then(|thread| thread["id"].as_u64())
        .expect("thread id should be available");

    let mut stack_response: Option<Value> = None;
    for _ in 0..5 {
        let result = run_cli_json_result(
            tmp.path(),
            &[
                "dap".to_string(),
                "stack-trace".to_string(),
                "--id".to_string(),
                session_id.clone(),
                "--thread-id".to_string(),
                thread_id.to_string(),
                "--timeout-ms".to_string(),
                "5000".to_string(),
            ],
        );
        match result {
            Ok(doc) => {
                stack_response = Some(doc);
                break;
            }
            Err(message) if message.contains("timeout waiting for response") => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(message) => panic!("stack-trace failed: {message}"),
        }
    }
    let stack_response = stack_response.expect("stack-trace should succeed with retry");
    assert_eq!(stack_response["ok"], true);
    let frames = stack_response["response"]["body"]["stackFrames"]
        .as_array()
        .expect("stackFrames should be array");
    assert!(
        frames
            .iter()
            .any(|frame| frame["source"]["path"] == script_path.to_string_lossy().to_string()),
        "expected stacktrace to include script path"
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
}
