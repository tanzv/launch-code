use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

fn wait_for_server_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    while std::time::Instant::now() < deadline {
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

#[test]
fn serve_exposes_health_endpoint() {
    let exe = assert_cmd::cargo::cargo_bin!("launch-code");

    let mut child = Command::new(exe)
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--token")
        .arg("testtoken")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve should start");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);

    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();

    let mut response = agent
        .get(&format!("{url}/v1/health"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("health request should succeed");

    assert_eq!(response.status(), ureq::http::StatusCode::OK);
    let body = response
        .body_mut()
        .read_to_string()
        .expect("body should be readable");
    assert!(body.contains("\"ok\":true"));

    let _ = child.kill();
    let _ = child.wait();
}
