use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn link_add_list_show_remove_roundtrip() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace = home_root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace should exist");

    let mut add_cmd = cargo_bin_cmd!("launch-code");
    let add_output = add_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg("demo")
        .arg("--path")
        .arg(workspace.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(add_output.status.success(), "link add should succeed");
    let add_stdout = String::from_utf8(add_output.stdout).expect("stdout should be utf8");
    let add_doc: Value = serde_json::from_str(&add_stdout).expect("stdout should be valid json");
    assert_eq!(add_doc["ok"], true);
    assert_eq!(add_doc["link"]["name"], "demo");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("list")
        .output()
        .expect("link list should run");
    assert!(list_output.status.success(), "link list should succeed");
    let list_stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    let list_doc: Value = serde_json::from_str(&list_stdout).expect("stdout should be valid json");
    let items = list_doc["items"]
        .as_array()
        .expect("items should be an array");
    assert_eq!(items.len(), 1, "link list should include one item");
    assert_eq!(items[0]["name"], "demo");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("show")
        .arg("--name")
        .arg("demo")
        .output()
        .expect("link show should run");
    assert!(show_output.status.success(), "link show should succeed");
    let show_stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let show_doc: Value = serde_json::from_str(&show_stdout).expect("stdout should be valid json");
    assert_eq!(show_doc["link"]["name"], "demo");

    let mut remove_cmd = cargo_bin_cmd!("launch-code");
    let remove_output = remove_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("remove")
        .arg("--name")
        .arg("demo")
        .output()
        .expect("link remove should run");
    assert!(remove_output.status.success(), "link remove should succeed");

    let mut list_after_remove_cmd = cargo_bin_cmd!("launch-code");
    let list_after_remove_output = list_after_remove_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("list")
        .output()
        .expect("link list should run");
    assert!(
        list_after_remove_output.status.success(),
        "link list should succeed"
    );
    let list_after_remove_stdout =
        String::from_utf8(list_after_remove_output.stdout).expect("stdout should be utf8");
    let list_after_remove_doc: Value =
        serde_json::from_str(&list_after_remove_stdout).expect("stdout should be valid json");
    let items = list_after_remove_doc["items"]
        .as_array()
        .expect("items should be an array");
    assert!(items.is_empty(), "link list should be empty after removal");
}

#[test]
fn link_scope_routes_runtime_commands_to_linked_workspace() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace = home_root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace should exist");

    let mut add_cmd = cargo_bin_cmd!("launch-code");
    let add_output = add_cmd
        .env("HOME", home_root.path())
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg("demo")
        .arg("--path")
        .arg(workspace.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(add_output.status.success(), "link add should succeed");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("HOME", home_root.path())
        .arg("--link")
        .arg("demo")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("linked-project")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("HOME", home_root.path())
        .arg("--link")
        .arg("demo")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["project"]["name"], "linked-project");
}

#[test]
fn default_list_aggregates_sessions_across_registered_links() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    std::fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    std::fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    let script_name = format!("app-{}.py", Uuid::new_v4().simple());
    let script_path = workspace_a.join(&script_name);
    std::fs::write(
        &script_path,
        "import time\nwhile True:\n    time.sleep(1)\n",
    )
    .expect("script should be written");

    let mut start_cmd = cargo_bin_cmd!("launch-code");
    let start_output = start_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("start")
        .arg("--runtime")
        .arg("python")
        .arg("--entry")
        .arg(&script_name)
        .arg("--cwd")
        .arg(".")
        .output()
        .expect("start should run");
    assert!(start_output.status.success(), "start should succeed");
    let start_stdout = String::from_utf8(start_output.stdout).expect("stdout should be utf8");
    let session_id = start_stdout
        .split_whitespace()
        .find_map(|token| token.strip_prefix("session_id="))
        .expect("start output should include session_id")
        .to_string();

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--json")
        .arg("list")
        .output()
        .expect("list should run");
    assert!(list_output.status.success(), "list should succeed");
    let list_stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    let list_doc: Value = serde_json::from_str(&list_stdout).expect("stdout should be valid json");
    let items = list_doc["items"]
        .as_array()
        .expect("items should be an array");
    let row = items
        .iter()
        .find(|item| item["id"] == session_id)
        .expect("aggregated list should include session from another workspace");
    let link_name = row["link_name"]
        .as_str()
        .expect("aggregated list should include link_name");
    let expected_workspace_path = std::fs::canonicalize(&workspace_a)
        .expect("workspace path should be canonicalizable")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        row["link_path"], expected_workspace_path,
        "aggregated list should include linked workspace path"
    );

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("HOME", home_root.path())
        .arg("--link")
        .arg(link_name)
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("stop should run");
    assert!(stop_output.status.success(), "stop should succeed");
}

#[test]
fn link_registry_initial_write_sets_schema_version() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace = home_root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace should exist");

    let mut add_cmd = cargo_bin_cmd!("launch-code");
    let add_output = add_cmd
        .env("HOME", home_root.path())
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg("demo")
        .arg("--path")
        .arg(workspace.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(add_output.status.success(), "link add should succeed");

    let registry_path = home_root.path().join(".launch-code").join("links.json");
    let payload = std::fs::read_to_string(&registry_path).expect("links registry should exist");
    let doc: Value = serde_json::from_str(&payload).expect("registry should be valid json");
    assert_eq!(
        doc["schema_version"].as_u64(),
        Some(1),
        "newly persisted link registry should use schema version 1"
    );
}

#[test]
fn global_list_auto_registers_current_workspace_link() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace = home_root.path().join("workspace-current");
    std::fs::create_dir_all(&workspace).expect("workspace should exist");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace)
        .arg("--json")
        .arg("list")
        .output()
        .expect("list should run");
    assert!(list_output.status.success(), "list should succeed");

    let registry_path = home_root.path().join(".launch-code").join("links.json");
    let payload = std::fs::read_to_string(&registry_path).expect("links registry should exist");
    let doc: Value = serde_json::from_str(&payload).expect("registry should be valid json");
    let links = doc["links"].as_object().expect("links should be an object");
    assert_eq!(
        links.len(),
        1,
        "global list should bootstrap one link for the current workspace"
    );
    let expected_workspace_path = std::fs::canonicalize(&workspace)
        .expect("workspace path should be canonicalizable")
        .to_string_lossy()
        .to_string();
    let linked_path = links
        .values()
        .next()
        .and_then(|value| value.get("path"))
        .and_then(|value| value.as_str())
        .expect("linked path should exist");
    assert_eq!(
        linked_path, expected_workspace_path,
        "bootstrapped link should target current workspace"
    );
}
