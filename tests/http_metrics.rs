use std::io::BufRead;
use std::io::BufReader;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

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
            && line.contains("listening=")
        {
            return line;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("server did not print a listening line in time");
}

#[test]
fn serve_exposes_metrics_for_request_outcomes() {
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

    let auth_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let noerr_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();
    let unauth_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut health_res = auth_agent
        .get(&format!("{url}/v1/health"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("health should succeed");
    assert_eq!(health_res.status(), ureq::http::StatusCode::OK);
    let _ = health_res
        .body_mut()
        .read_to_string()
        .expect("body readable");

    let mut not_found_res = noerr_agent
        .get(&format!("{url}/v1/not-found"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("not-found request should complete");
    assert_eq!(not_found_res.status(), ureq::http::StatusCode::NOT_FOUND);
    let _ = not_found_res
        .body_mut()
        .read_to_string()
        .expect("body readable");

    let mut unauthorized_res = unauth_agent
        .get(&format!("{url}/v1/health"))
        .call()
        .expect("unauthorized request should complete");
    assert_eq!(
        unauthorized_res.status(),
        ureq::http::StatusCode::UNAUTHORIZED
    );
    let _ = unauthorized_res
        .body_mut()
        .read_to_string()
        .expect("body readable");

    let mut metrics_res = auth_agent
        .get(&format!("{url}/v1/metrics"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("metrics should succeed");
    assert_eq!(metrics_res.status(), ureq::http::StatusCode::OK);
    let text = metrics_res
        .body_mut()
        .read_to_string()
        .expect("body readable");
    let doc: Value = serde_json::from_str(&text).expect("metrics should be json");
    assert_eq!(doc["ok"], true);

    let requests_total = doc["metrics"]["requests_total"]
        .as_u64()
        .expect("requests_total should be u64");
    let responses_2xx = doc["metrics"]["responses"]["2xx"]
        .as_u64()
        .expect("2xx should be u64");
    let responses_4xx = doc["metrics"]["responses"]["4xx"]
        .as_u64()
        .expect("4xx should be u64");
    let responses_401 = doc["metrics"]["responses"]["401"]
        .as_u64()
        .expect("401 should be u64");
    let responses_404 = doc["metrics"]["responses"]["404"]
        .as_u64()
        .expect("404 should be u64");
    let average_ms = doc["metrics"]["latency"]["average_ms"]
        .as_f64()
        .expect("average_ms should be f64");

    assert!(
        requests_total >= 3,
        "metrics should include executed requests"
    );
    assert!(responses_2xx >= 1, "2xx should include health request");
    assert!(
        responses_4xx >= 2,
        "4xx should include unauthorized and not-found"
    );
    assert!(
        responses_401 >= 1,
        "401 should include unauthorized request"
    );
    assert!(responses_404 >= 1, "404 should include not-found request");
    assert!(average_ms >= 0.0, "average latency should be non-negative");

    let _ = child.kill();
    let _ = child.wait();
}
