use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

fn python_with_debugpy(workdir: &std::path::Path) -> Option<String> {
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

    let bootstrap = ["python3", "python"].into_iter().find(|bin| {
        std::process::Command::new(bin)
            .arg("--version")
            .output()
            .is_ok()
    })?;

    let venv_dir = workdir.join(".venv-debugpy");
    let venv_dir_str = venv_dir.to_string_lossy().to_string();
    let status = std::process::Command::new(bootstrap)
        .args(["-m", "venv", &venv_dir_str])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    let venv_python = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };
    let venv_python_str = venv_python.to_string_lossy().to_string();

    let install_status = std::process::Command::new(&venv_python_str)
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .args(["-m", "pip", "-q", "install", "debugpy"])
        .status()
        .ok()?;
    if !install_status.success() {
        return None;
    }

    if python_debug_ready(&venv_python_str) {
        Some(venv_python_str)
    } else {
        None
    }
}

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while Instant::now() < deadline {
        line.clear();
        if stdout
            .read_line(&mut line)
            .ok()
            .filter(|v| *v > 0)
            .is_some()
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

fn parse_field<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

#[test]
fn e2e_python_debug_http_can_hit_breakpoint_and_report_stacktrace() {
    let tmp = tempdir().expect("temp dir should exist");
    let python_bin = match python_with_debugpy(tmp.path()) {
        Some(value) => value,
        None => return,
    };

    let port = TcpListener::bind("127.0.0.1:0")
        .expect("debug port should bind")
        .local_addr()
        .expect("debug port should have addr")
        .port();

    if !python_debug_ready(&python_bin) {
        return;
    }

    let script_path = tmp.path().join("app.py");
    let script = [
        "import time",
        "def run():",
        "    x = 0",
        "    x += 1  # breakpoint",
        "    print('ready', flush=True)",
        "    time.sleep(30)",
        "run()",
        "",
    ]
    .join("\n");
    fs::write(&script_path, script).expect("script should be written");
    let breakpoint_line = 4u64;

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
        .arg(port.to_string())
        .arg("--wait-for-client")
        .arg("true")
        .output()
        .expect("debug should run");

    assert!(debug_output.status.success(), "debug should succeed");
    let debug_stdout = String::from_utf8(debug_output.stdout).expect("debug stdout utf8");
    let session_id = parse_field(&debug_stdout, "session_id")
        .expect("session_id should exist")
        .to_string();

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut serve = Command::new(exe)
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = serve.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let base_url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into();
    let agent_no_status_err: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build()
        .into();

    let debug_info = {
        let url = format!("{base_url}/v1/sessions/{session_id}/debug");
        let mut res = agent
            .get(&url)
            .header("Authorization", "Bearer testtoken")
            .call()
            .expect("debug info should succeed");
        assert_eq!(res.status(), ureq::http::StatusCode::OK);
        let text = res.body_mut().read_to_string().expect("body readable");
        serde_json::from_str::<Value>(&text).expect("json")
    };
    assert_eq!(debug_info["ok"], true);
    assert!(debug_info["endpoint"].as_str().is_some());

    let dap_req_url = format!("{base_url}/v1/sessions/{session_id}/debug/dap/request");
    let dap_events_url =
        format!("{base_url}/v1/sessions/{session_id}/debug/dap/events?timeout_ms=500&max=50");

    let setup = json!({
        "batch": [
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
        ],
        "timeout_ms": 5000
    });
    let _setup_resp = {
        let mut res = agent_no_status_err
            .post(&dap_req_url)
            .header("Authorization", "Bearer testtoken")
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(&setup).unwrap())
            .expect("debug setup should succeed");
        let status = res.status();
        let text = res.body_mut().read_to_string().expect("body readable");
        if status != ureq::http::StatusCode::OK {
            panic!("debug setup failed status={status} body={text}");
        }
        serde_json::from_str::<Value>(&text).expect("json")
    };

    let mut stopped: Option<Value> = None;
    let pre_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < pre_deadline && stopped.is_none() {
        let mut res = agent
            .get(&dap_events_url)
            .header("Authorization", "Bearer testtoken")
            .call()
            .expect("events should succeed");
        assert_eq!(res.status(), ureq::http::StatusCode::OK);
        let text = res.body_mut().read_to_string().expect("body readable");
        let events_json: Value = serde_json::from_str(&text).expect("events json");
        if let Some(events) = events_json["events"].as_array() {
            for event in events {
                if event["type"] == "event"
                    && event["event"] == "stopped"
                    && event["body"]["reason"] == "breakpoint"
                {
                    stopped = Some(event.clone());
                    break;
                }
            }
        }
    }

    if stopped.is_none() {
        let url = format!("{base_url}/v1/sessions/{session_id}/debug/continue");
        let mut res = agent
            .post(&url)
            .header("Authorization", "Bearer testtoken")
            .header("Content-Type", "application/json")
            .send("{}")
            .expect("continue should succeed");
        assert_eq!(res.status(), ureq::http::StatusCode::OK);
        let _ = res.body_mut().read_to_string().expect("body readable");
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && stopped.is_none() {
        let mut res = agent
            .get(&dap_events_url)
            .header("Authorization", "Bearer testtoken")
            .call()
            .expect("events should succeed");
        assert_eq!(res.status(), ureq::http::StatusCode::OK);
        let text = res.body_mut().read_to_string().expect("body readable");
        let events_json: Value = serde_json::from_str(&text).expect("events json");
        if let Some(events) = events_json["events"].as_array() {
            for event in events {
                if event["type"] == "event"
                    && event["event"] == "stopped"
                    && event["body"]["reason"] == "breakpoint"
                {
                    stopped = Some(event.clone());
                    break;
                }
            }
        }
    }

    let stopped = stopped.expect("expected a stopped event from breakpoint");
    let thread_id = stopped["body"]["threadId"]
        .as_u64()
        .expect("threadId should exist in stopped event");

    let stack_trace = json!({
        "command": "stackTrace",
        "arguments": {
            "threadId": thread_id
        }
    });
    let stack_resp = {
        let mut res = agent
            .post(&dap_req_url)
            .header("Authorization", "Bearer testtoken")
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(&stack_trace).unwrap())
            .expect("stackTrace should succeed");
        assert_eq!(res.status(), ureq::http::StatusCode::OK);
        let text = res.body_mut().read_to_string().expect("body readable");
        serde_json::from_str::<Value>(&text).expect("json")
    };
    assert_eq!(stack_resp["ok"], true);
    let frames = stack_resp["response"]["body"]["stackFrames"]
        .as_array()
        .expect("stackFrames should be array");
    assert!(
        frames
            .iter()
            .any(|frame| frame["source"]["path"] == script_path.to_string_lossy().to_string()),
        "expected stacktrace to include script path"
    );

    let stop_url = format!("{base_url}/v1/sessions/{session_id}/stop");
    let mut stop_res = agent
        .post(&stop_url)
        .header("Authorization", "Bearer testtoken")
        .send_empty()
        .expect("stop should succeed");
    assert_eq!(stop_res.status(), ureq::http::StatusCode::OK);
    let _ = stop_res
        .body_mut()
        .read_to_string()
        .expect("stop body readable");

    let _ = serve.kill();
    let _ = serve.wait();
}
