use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
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
fn stop_auto_routes_session_id_across_links_in_global_scope() {
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

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("stop should run");
    assert!(
        stop_output.status.success(),
        "stop should auto-route by session id"
    );

    let mut status_cmd = cargo_bin_cmd!("launch-code");
    let status_output = status_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--json")
        .arg("status")
        .arg("--id")
        .arg(&session_id)
        .output()
        .expect("status should run");
    assert!(
        status_output.status.success(),
        "status should auto-route by session id"
    );
    let status_doc: Value =
        serde_json::from_slice(&status_output.stdout).expect("stdout should be valid json");
    let message = status_doc["message"]
        .as_str()
        .expect("status json should include message");
    assert!(
        message.contains("status=stopped"),
        "status message should report stopped session"
    );
}

#[test]
fn stop_auto_routes_using_cached_session_lookup_when_registry_missing_target_link() {
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

    let session_index_path = home_root
        .path()
        .join(".launch-code")
        .join("session-index.json");
    let session_index_payload =
        std::fs::read_to_string(&session_index_path).expect("session index should exist");
    let session_index_doc: Value =
        serde_json::from_str(&session_index_payload).expect("session index should be valid json");
    assert!(
        session_index_doc["sessions"].get(&session_id).is_some(),
        "session index should include the started session id"
    );

    let links_registry_path = home_root.path().join(".launch-code").join("links.json");
    std::fs::write(
        &links_registry_path,
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "links": {}
        }))
        .expect("links json"),
    )
    .expect("links registry should be rewritten");

    let mut stop_cmd = cargo_bin_cmd!("launch-code");
    let stop_output = stop_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("stop")
        .arg("--id")
        .arg(&session_id)
        .arg("--force")
        .output()
        .expect("stop should run");
    assert!(
        stop_output.status.success(),
        "stop should use cached session lookup for cross-link routing"
    );
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

#[test]
fn link_prune_removes_missing_and_temporary_empty_links() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_keep = home_root.path().join("workspace-keep");
    let workspace_missing = home_root.path().join("workspace-missing");
    let workspace_temp = home_root.path().join("workspace-temp-empty");
    std::fs::create_dir_all(&workspace_keep).expect("workspace keep should exist");
    std::fs::create_dir_all(&workspace_missing).expect("workspace missing should exist");
    std::fs::create_dir_all(&workspace_temp).expect("workspace temp should exist");

    for (name, path) in [
        ("keep", workspace_keep.as_path()),
        ("missing", workspace_missing.as_path()),
        ("temp-empty", workspace_temp.as_path()),
    ] {
        let mut add_cmd = cargo_bin_cmd!("launch-code");
        let add_output = add_cmd
            .env("HOME", home_root.path())
            .arg("link")
            .arg("add")
            .arg("--name")
            .arg(name)
            .arg("--path")
            .arg(path.to_string_lossy().to_string())
            .output()
            .expect("link add should run");
        assert!(add_output.status.success(), "link add should succeed");
    }

    let mut set_project_cmd = cargo_bin_cmd!("launch-code");
    let set_project_output = set_project_cmd
        .env("HOME", home_root.path())
        .arg("--link")
        .arg("keep")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("keep-project")
        .output()
        .expect("project set should run");
    assert!(
        set_project_output.status.success(),
        "project set should succeed"
    );

    std::fs::remove_dir_all(&workspace_missing).expect("workspace missing should be removed");

    let mut prune_dry_run_cmd = cargo_bin_cmd!("launch-code");
    let prune_dry_run_output = prune_dry_run_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("prune")
        .arg("--dry-run")
        .output()
        .expect("link prune dry run should execute");
    assert!(
        prune_dry_run_output.status.success(),
        "link prune dry run should succeed"
    );
    let prune_dry_run_doc: Value =
        serde_json::from_slice(&prune_dry_run_output.stdout).expect("stdout should be valid json");
    assert_eq!(prune_dry_run_doc["dry_run"], true);
    assert_eq!(prune_dry_run_doc["matched_count"], 2);
    assert_eq!(prune_dry_run_doc["removed_count"], 0);

    let mut prune_cmd = cargo_bin_cmd!("launch-code");
    let prune_output = prune_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("prune")
        .output()
        .expect("link prune should execute");
    assert!(prune_output.status.success(), "link prune should succeed");
    let prune_doc: Value =
        serde_json::from_slice(&prune_output.stdout).expect("stdout should be valid json");
    assert_eq!(prune_doc["dry_run"], false);
    assert_eq!(prune_doc["matched_count"], 2);
    assert_eq!(prune_doc["removed_count"], 2);

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("list")
        .output()
        .expect("link list should run");
    assert!(list_output.status.success(), "link list should succeed");
    let list_doc: Value = serde_json::from_slice(&list_output.stdout).expect("stdout json");
    let items = list_doc["items"].as_array().expect("items should be array");
    assert_eq!(items.len(), 1, "only one link should remain");
    assert_eq!(items[0]["name"], "keep");
}

#[test]
fn global_list_auto_prunes_stale_links_when_thresholds_match() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_keep = home_root.path().join("workspace-keep");
    let workspace_missing = home_root.path().join("workspace-missing");
    std::fs::create_dir_all(&workspace_keep).expect("workspace keep should exist");
    std::fs::create_dir_all(&workspace_missing).expect("workspace missing should exist");

    for (name, path) in [
        ("keep", workspace_keep.as_path()),
        ("missing", workspace_missing.as_path()),
    ] {
        let mut add_cmd = cargo_bin_cmd!("launch-code");
        let add_output = add_cmd
            .env("HOME", home_root.path())
            .arg("link")
            .arg("add")
            .arg("--name")
            .arg(name)
            .arg("--path")
            .arg(path.to_string_lossy().to_string())
            .output()
            .expect("link add should run");
        assert!(add_output.status.success(), "link add should succeed");
    }

    let mut set_project_cmd = cargo_bin_cmd!("launch-code");
    let set_project_output = set_project_cmd
        .env("HOME", home_root.path())
        .arg("--link")
        .arg("keep")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("keep-project")
        .output()
        .expect("project set should run");
    assert!(
        set_project_output.status.success(),
        "project set should succeed"
    );

    std::fs::remove_dir_all(&workspace_missing).expect("workspace missing should be removed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .env("LCODE_AUTO_PRUNE_MIN_LINKS", "2")
        .env("LCODE_AUTO_PRUNE_MIN_MATCHED", "1")
        .env("LCODE_AUTO_PRUNE_RATIO_PERCENT", "1")
        .env("LCODE_AUTO_PRUNE_VERBOSE", "1")
        .current_dir(&workspace_keep)
        .arg("--json")
        .arg("list")
        .output()
        .expect("global list should run");
    assert!(list_output.status.success(), "global list should succeed");
    let list_stderr = String::from_utf8(list_output.stderr).expect("stderr should be utf8");
    assert!(
        list_stderr.contains("lcode_auto_prune applied"),
        "verbose auto prune should emit applied telemetry on stderr"
    );

    let mut link_list_cmd = cargo_bin_cmd!("launch-code");
    let link_list_output = link_list_cmd
        .env("HOME", home_root.path())
        .arg("--json")
        .arg("link")
        .arg("list")
        .output()
        .expect("link list should run");
    assert!(
        link_list_output.status.success(),
        "link list should succeed"
    );

    let link_list_doc: Value =
        serde_json::from_slice(&link_list_output.stdout).expect("stdout should be valid json");
    let items = link_list_doc["items"]
        .as_array()
        .expect("items should be an array");
    assert_eq!(items.len(), 1, "auto prune should keep only valid link");
    assert_eq!(items[0]["name"], "keep");
}
