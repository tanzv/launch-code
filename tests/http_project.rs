use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::tempdir;

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

fn read_json_body(response: &mut ureq::http::Response<ureq::Body>) -> Value {
    let text = response
        .body_mut()
        .read_to_string()
        .expect("response body should be readable");
    serde_json::from_str(&text).expect("response body should be json")
}

#[test]
fn serve_project_routes_support_crud_roundtrip() {
    let tmp = tempdir().expect("temp dir should exist");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
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

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into();

    let mut get_initial = agent
        .get(&format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .call()
        .expect("project get should succeed");
    assert_eq!(get_initial.status(), ureq::http::StatusCode::OK);
    let get_initial_doc = read_json_body(&mut get_initial);
    assert_eq!(get_initial_doc["ok"], true);
    assert!(get_initial_doc["project"].is_null());

    let payload = json!({
        "name": "launch-code",
        "description": "IDE-like launch manager",
        "repository": "https://example.com/org/launch-code",
        "languages": ["rust", "python"],
        "runtimes": ["python"],
        "tools": ["debugpy"],
        "tags": ["cli", "debug"]
    });
    let mut put_res = agent
        .put(&format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&payload).expect("payload should serialize"))
        .expect("project put should succeed");
    assert_eq!(put_res.status(), ureq::http::StatusCode::OK);
    let put_doc = read_json_body(&mut put_res);
    assert_eq!(put_doc["project"]["name"], "launch-code");
    assert_eq!(put_doc["project"]["languages"][1], "python");

    let patch_payload = json!({
        "tools": ["dap", "debugpy"],
        "tags": null
    });
    let mut patch_res = agent
        .patch(&format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .send(serde_json::to_string(&patch_payload).expect("payload should serialize"))
        .expect("project patch should succeed");
    assert_eq!(patch_res.status(), ureq::http::StatusCode::OK);
    let patch_doc = read_json_body(&mut patch_res);
    assert_eq!(patch_doc["project"]["tools"][0], "dap");
    assert!(patch_doc["project"]["tags"].is_null());

    let delete_fields_payload = json!({"fields": ["tools", "languages"]});
    let delete_fields_req = ureq::http::Request::builder()
        .method("DELETE")
        .uri(format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&delete_fields_payload).expect("payload should serialize"))
        .expect("delete request should build");
    let mut delete_fields_res = agent
        .run(delete_fields_req)
        .expect("project delete should succeed");
    assert_eq!(delete_fields_res.status(), ureq::http::StatusCode::OK);
    let delete_fields_doc = read_json_body(&mut delete_fields_res);
    assert!(delete_fields_doc["project"]["tools"].is_null());
    assert!(delete_fields_doc["project"]["languages"].is_null());

    let delete_all_req = ureq::http::Request::builder()
        .method("DELETE")
        .uri(format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .header("Content-Type", "application/json")
        .body("{}")
        .expect("delete request should build");
    let mut delete_all_res = agent
        .run(delete_all_req)
        .expect("project delete all should succeed");
    assert_eq!(delete_all_res.status(), ureq::http::StatusCode::OK);
    let delete_all_doc = read_json_body(&mut delete_all_res);
    assert!(delete_all_doc["project"].is_null());

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn serve_project_routes_validate_payload() {
    let tmp = tempdir().expect("temp dir should exist");

    let exe = assert_cmd::cargo::cargo_bin!("launch-code");
    let mut child = Command::new(exe)
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

    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let line = wait_for_server_line(&mut reader);
    let url = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("listening="))
        .expect("listening url should be printed")
        .to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut bad_put_res = agent
        .put(&format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .send("{\"languages\":[1]}")
        .expect("project bad put should complete");
    assert_eq!(bad_put_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_put_doc = read_json_body(&mut bad_put_res);
    assert_eq!(bad_put_doc["ok"], false);
    assert_eq!(bad_put_doc["error"], "bad_request");

    let mut empty_put_res = agent
        .put(&format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .send("{}")
        .expect("project empty put should complete");
    assert_eq!(empty_put_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let empty_put_doc = read_json_body(&mut empty_put_res);
    assert_eq!(empty_put_doc["ok"], false);
    assert_eq!(empty_put_doc["error"], "bad_request");

    let bad_delete_req = ureq::http::Request::builder()
        .method("DELETE")
        .uri(format!("{url}/v1/project"))
        .header("Authorization", "Bearer testtoken")
        .header("Content-Type", "application/json")
        .body("{\"fields\":[\"unknown\"]}")
        .expect("delete request should build");
    let mut bad_delete_res = agent
        .run(bad_delete_req)
        .expect("project bad delete should complete");
    assert_eq!(bad_delete_res.status(), ureq::http::StatusCode::BAD_REQUEST);
    let bad_delete_doc = read_json_body(&mut bad_delete_res);
    assert_eq!(bad_delete_doc["ok"], false);
    assert_eq!(bad_delete_doc["error"], "bad_request");

    let _ = child.kill();
    let _ = child.wait();
}
