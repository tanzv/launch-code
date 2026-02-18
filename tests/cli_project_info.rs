use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn project_show_returns_null_when_metadata_not_set() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(output.status.success(), "project show should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("project show should output json");
    assert_eq!(doc["ok"], true);
    assert!(
        doc["project"].is_null(),
        "project show should return null when metadata does not exist"
    );
}

#[test]
fn project_set_show_unset_roundtrip_persists_state() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .arg("--description")
        .arg("IDE-like launch manager")
        .arg("--repository")
        .arg("https://example.com/org/launch-code")
        .arg("--language")
        .arg("rust")
        .arg("--language")
        .arg("python")
        .arg("--runtime")
        .arg("python")
        .arg("--runtime")
        .arg("node")
        .arg("--tool")
        .arg("debugpy")
        .arg("--tool")
        .arg("dap")
        .arg("--tag")
        .arg("cli")
        .arg("--tag")
        .arg("debug")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let set_stdout = String::from_utf8(set_output.stdout).expect("stdout should be utf8");
    assert!(
        set_stdout.contains("project_info_updated=true"),
        "project set should confirm update"
    );

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let show_stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let show_doc: Value =
        serde_json::from_str(&show_stdout).expect("project show should output json");
    assert_eq!(show_doc["ok"], true);
    assert_eq!(show_doc["project"]["name"], "launch-code");
    assert_eq!(
        show_doc["project"]["description"],
        "IDE-like launch manager"
    );
    assert_eq!(
        show_doc["project"]["repository"],
        "https://example.com/org/launch-code"
    );
    assert_eq!(show_doc["project"]["languages"][0], "rust");
    assert_eq!(show_doc["project"]["languages"][1], "python");
    assert_eq!(show_doc["project"]["runtimes"][0], "python");
    assert_eq!(show_doc["project"]["runtimes"][1], "node");
    assert_eq!(show_doc["project"]["tools"][0], "debugpy");
    assert_eq!(show_doc["project"]["tools"][1], "dap");
    assert_eq!(show_doc["project"]["tags"][0], "cli");
    assert_eq!(show_doc["project"]["tags"][1], "debug");

    let state_path = tmp.path().join(".launch-code").join("state.json");
    let state_payload = fs::read_to_string(state_path).expect("state file should exist");
    let state_doc: Value = serde_json::from_str(&state_payload).expect("state should be valid");
    assert_eq!(state_doc["project_info"]["name"], "launch-code");
    assert_eq!(
        state_doc["project_info"]["repository"],
        "https://example.com/org/launch-code"
    );

    let mut unset_cmd = cargo_bin_cmd!("launch-code");
    let unset_output = unset_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("unset")
        .arg("--field")
        .arg("tools")
        .arg("--field")
        .arg("tags")
        .output()
        .expect("project unset should run");
    assert!(
        unset_output.status.success(),
        "project unset should succeed"
    );

    let mut show_after_unset_cmd = cargo_bin_cmd!("launch-code");
    let show_after_unset_output = show_after_unset_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show after unset should run");
    assert!(
        show_after_unset_output.status.success(),
        "project show after unset should succeed"
    );
    let show_after_unset_stdout =
        String::from_utf8(show_after_unset_output.stdout).expect("stdout should be utf8");
    let show_after_unset_doc: Value =
        serde_json::from_str(&show_after_unset_stdout).expect("project show should output json");
    assert!(
        show_after_unset_doc["project"]["tools"].is_null(),
        "unset tools should clear tools field"
    );
    assert!(
        show_after_unset_doc["project"]["tags"].is_null(),
        "unset tags should clear tags field"
    );
}

#[test]
fn project_unset_all_clears_project_metadata() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut unset_cmd = cargo_bin_cmd!("launch-code");
    let unset_output = unset_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("unset")
        .arg("--field")
        .arg("all")
        .output()
        .expect("project unset all should run");
    assert!(
        unset_output.status.success(),
        "project unset all should succeed"
    );

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("project show should output json");
    assert!(
        doc["project"].is_null(),
        "project metadata should be null after unset all"
    );
}

#[test]
fn project_show_plain_text_uses_human_readable_layout() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .arg("--description")
        .arg("IDE-like launch manager")
        .arg("--language")
        .arg("rust")
        .arg("--language")
        .arg("python")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");

    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("name: launch-code"),
        "project show should print key-value output"
    );
    assert!(
        stdout.contains("description: IDE-like launch manager"),
        "project show should print the description field"
    );
    assert!(
        stdout.contains("languages: rust, python"),
        "project show should print list values in a compact form"
    );
}

#[test]
fn project_list_prints_compact_field_rows() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .arg("--runtime")
        .arg("python")
        .arg("--tool")
        .arg("debugpy")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("list")
        .output()
        .expect("project list should run");
    assert!(list_output.status.success(), "project list should succeed");

    let stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("name\tlaunch-code"),
        "project list should include compact rows"
    );
    assert!(
        stdout.contains("runtimes\tpython"),
        "project list should include runtime row"
    );
    assert!(
        stdout.contains("tools\tdebugpy"),
        "project list should include tool row"
    );
}

#[test]
fn project_clear_clears_project_metadata() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut clear_cmd = cargo_bin_cmd!("launch-code");
    let clear_output = clear_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("clear")
        .output()
        .expect("project clear should run");
    assert!(
        clear_output.status.success(),
        "project clear should succeed"
    );
    let clear_stdout = String::from_utf8(clear_output.stdout).expect("stdout should be utf8");
    let clear_doc: Value = serde_json::from_str(&clear_stdout).expect("stdout should be json");
    assert_eq!(clear_doc["ok"], true);
    assert_eq!(clear_doc["message"], "project_info_cleared=true");
    assert!(
        clear_doc["project"].is_null(),
        "project clear should return null project"
    );

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let show_stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let show_doc: Value = serde_json::from_str(&show_stdout).expect("stdout should be json");
    assert!(show_doc["project"].is_null());
}

#[test]
fn project_list_supports_field_filter_and_all_flag() {
    let tmp = tempdir().expect("temp dir should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("launch-code")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("LAUNCH_CODE_HOME", tmp.path())
        .arg("project")
        .arg("list")
        .arg("--field")
        .arg("name")
        .arg("--field")
        .arg("repository")
        .arg("--all")
        .output()
        .expect("project list should run");
    assert!(list_output.status.success(), "project list should succeed");

    let stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("name\tlaunch-code"),
        "project list should include requested name field"
    );
    assert!(
        stdout.contains("repository\tnull"),
        "project list should include empty requested fields with --all"
    );
}

#[test]
fn project_commands_support_link_scope_via_registry() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    let mut link_cmd = cargo_bin_cmd!("launch-code");
    let link_output = link_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("link")
        .arg("add")
        .arg("--name")
        .arg("demo")
        .arg("--path")
        .arg(workspace_a.to_string_lossy().to_string())
        .output()
        .expect("link add should run");
    assert!(link_output.status.success(), "link add should succeed");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
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
        .current_dir(&workspace_b)
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
fn project_commands_default_to_global_link_scope() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("local-default-project")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(
        doc["scope"], "global",
        "project show should use global aggregation by default"
    );
    assert!(
        doc["project"]["name"] == "local-default-project",
        "single global project metadata should be lifted into project field"
    );
    let items = doc["items"].as_array().expect("items should be an array");
    let expected_workspace_path = fs::canonicalize(&workspace_a)
        .expect("workspace path should be canonicalizable")
        .to_string_lossy()
        .to_string();
    assert!(
        items.iter().any(|item| {
            item["project"]["name"] == "local-default-project"
                && item["link"]["path"] == expected_workspace_path
        }),
        "global aggregation should include workspace project metadata"
    );
}

#[test]
fn global_flag_uses_same_default_link_scope_behavior() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--global")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("global-flag-project")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--global")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert!(
        doc["project"]["name"] == "global-flag-project",
        "global flag should keep runtime in the same workspace link scope"
    );
}

#[test]
fn project_commands_support_local_scope_override() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");

    let mut set_cmd = cargo_bin_cmd!("launch-code");
    let set_output = set_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--local")
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("local-project")
        .output()
        .expect("project set should run");
    assert!(set_output.status.success(), "project set should succeed");

    let mut show_cmd = cargo_bin_cmd!("launch-code");
    let show_output = show_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--local")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(show_output.status.success(), "project show should succeed");
    let stdout = String::from_utf8(show_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert!(
        doc["project"].is_null(),
        "local scope should not share metadata across workspaces"
    );
}

#[test]
fn project_list_all_links_aggregates_metadata_rows() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    let workspace_c = home_root.path().join("workspace-c");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");
    fs::create_dir_all(&workspace_c).expect("workspace c should exist");

    let mut set_a_cmd = cargo_bin_cmd!("launch-code");
    let set_a_output = set_a_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("project-a")
        .output()
        .expect("project set should run");
    assert!(set_a_output.status.success(), "project set should succeed");

    let mut set_b_cmd = cargo_bin_cmd!("launch-code");
    let set_b_output = set_b_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("project-b")
        .output()
        .expect("project set should run");
    assert!(set_b_output.status.success(), "project set should succeed");

    let mut list_cmd = cargo_bin_cmd!("launch-code");
    let list_output = list_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_c)
        .arg("--json")
        .arg("project")
        .arg("list")
        .arg("--all-links")
        .arg("--field")
        .arg("name")
        .output()
        .expect("project list should run");
    assert!(list_output.status.success(), "project list should succeed");

    let stdout = String::from_utf8(list_output.stdout).expect("stdout should be utf8");
    let doc: Value = serde_json::from_str(&stdout).expect("stdout should be valid json");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["scope"], "global");
    let items = doc["items"].as_array().expect("items should be an array");
    assert!(
        items.iter().any(|item| {
            item["project"]["name"] == "project-a"
                && item["fields"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field["field"] == "name"))
        }),
        "global list should include project-a rows"
    );
    assert!(
        items.iter().any(|item| {
            item["project"]["name"] == "project-b"
                && item["fields"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field["field"] == "name"))
        }),
        "global list should include project-b rows"
    );
}

#[test]
fn project_set_unset_clear_support_all_links_batch_mode() {
    let home_root = tempdir().expect("temp dir should exist");
    let workspace_a = home_root.path().join("workspace-a");
    let workspace_b = home_root.path().join("workspace-b");
    let workspace_c = home_root.path().join("workspace-c");
    fs::create_dir_all(&workspace_a).expect("workspace a should exist");
    fs::create_dir_all(&workspace_b).expect("workspace b should exist");
    fs::create_dir_all(&workspace_c).expect("workspace c should exist");

    let mut set_a_cmd = cargo_bin_cmd!("launch-code");
    let set_a_output = set_a_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("project-a")
        .output()
        .expect("project set should run");
    assert!(set_a_output.status.success(), "project set should succeed");

    let mut set_b_cmd = cargo_bin_cmd!("launch-code");
    let set_b_output = set_b_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("project")
        .arg("set")
        .arg("--name")
        .arg("project-b")
        .output()
        .expect("project set should run");
    assert!(set_b_output.status.success(), "project set should succeed");

    let mut global_set_cmd = cargo_bin_cmd!("launch-code");
    let global_set_output = global_set_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_c)
        .arg("--json")
        .arg("project")
        .arg("set")
        .arg("--all-links")
        .arg("--tag")
        .arg("shared")
        .output()
        .expect("project set should run");
    assert!(
        global_set_output.status.success(),
        "global project set should succeed"
    );
    let set_doc: Value =
        serde_json::from_slice(&global_set_output.stdout).expect("stdout should be valid json");
    assert_eq!(set_doc["scope"], "global");

    let mut show_a_cmd = cargo_bin_cmd!("launch-code");
    let show_a_output = show_a_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--local")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(
        show_a_output.status.success(),
        "project show should succeed"
    );
    let show_a_doc: Value =
        serde_json::from_slice(&show_a_output.stdout).expect("stdout should be valid json");
    assert_eq!(show_a_doc["project"]["tags"][0], "shared");

    let mut show_b_cmd = cargo_bin_cmd!("launch-code");
    let show_b_output = show_b_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--local")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(
        show_b_output.status.success(),
        "project show should succeed"
    );
    let show_b_doc: Value =
        serde_json::from_slice(&show_b_output.stdout).expect("stdout should be valid json");
    assert_eq!(show_b_doc["project"]["tags"][0], "shared");

    let mut global_unset_cmd = cargo_bin_cmd!("launch-code");
    let global_unset_output = global_unset_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_c)
        .arg("--json")
        .arg("project")
        .arg("unset")
        .arg("--all-links")
        .arg("--field")
        .arg("tags")
        .output()
        .expect("project unset should run");
    assert!(
        global_unset_output.status.success(),
        "global project unset should succeed"
    );

    let mut show_after_unset_cmd = cargo_bin_cmd!("launch-code");
    let show_after_unset_output = show_after_unset_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_a)
        .arg("--local")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(
        show_after_unset_output.status.success(),
        "project show should succeed"
    );
    let show_after_unset_doc: Value =
        serde_json::from_slice(&show_after_unset_output.stdout).expect("stdout should be json");
    assert!(
        show_after_unset_doc["project"]["tags"].is_null(),
        "global unset should clear tag field on each linked workspace"
    );

    let mut global_clear_cmd = cargo_bin_cmd!("launch-code");
    let global_clear_output = global_clear_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_c)
        .arg("--json")
        .arg("project")
        .arg("clear")
        .arg("--all-links")
        .output()
        .expect("project clear should run");
    assert!(
        global_clear_output.status.success(),
        "global project clear should succeed"
    );
    let clear_doc: Value =
        serde_json::from_slice(&global_clear_output.stdout).expect("stdout should be valid json");
    assert_eq!(clear_doc["scope"], "global");

    let mut show_after_clear_cmd = cargo_bin_cmd!("launch-code");
    let show_after_clear_output = show_after_clear_cmd
        .env("HOME", home_root.path())
        .current_dir(&workspace_b)
        .arg("--local")
        .arg("--json")
        .arg("project")
        .arg("show")
        .output()
        .expect("project show should run");
    assert!(
        show_after_clear_output.status.success(),
        "project show should succeed"
    );
    let show_after_clear_doc: Value =
        serde_json::from_slice(&show_after_clear_output.stdout).expect("stdout should be json");
    assert!(
        show_after_clear_doc["project"].is_null(),
        "global clear should clear metadata on each linked workspace"
    );
}
