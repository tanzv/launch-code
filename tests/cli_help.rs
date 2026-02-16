use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn top_level_help_includes_command_descriptions() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd.arg("--help").output().expect("help should run");
    assert!(output.status.success(), "help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("Start a new run session"),
        "top-level help should explain the start command"
    );
    assert!(
        stdout.contains("Run a session in debug mode"),
        "top-level help should explain the debug command"
    );
    assert!(
        stdout.contains("Expose an HTTP control plane"),
        "top-level help should explain the serve command"
    );
    assert!(
        stdout.contains("Manage saved run/debug profiles"),
        "top-level help should explain the config command"
    );
}

#[test]
fn dap_help_includes_expression_command_descriptions() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("--help")
        .output()
        .expect("dap help should run");
    assert!(output.status.success(), "dap help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("Configure exception breakpoints"),
        "dap help should explain exception-breakpoints"
    );
    assert!(
        stdout.contains("Evaluate an expression"),
        "dap help should explain evaluate"
    );
    assert!(
        stdout.contains("Set a variable value"),
        "dap help should explain set-variable"
    );
    assert!(
        stdout.contains("Adopt a debugpy child-process attach event"),
        "dap help should explain adopt-subprocess"
    );
}

#[test]
fn evaluate_help_explains_key_arguments() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("evaluate")
        .arg("--help")
        .output()
        .expect("evaluate help should run");
    assert!(output.status.success(), "evaluate help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("Expression to evaluate"),
        "evaluate help should explain --expression"
    );
    assert!(
        stdout.contains("Optional stack frame id"),
        "evaluate help should explain --frame-id"
    );
    assert!(
        stdout.contains("Evaluation context"),
        "evaluate help should explain --context"
    );
}

#[test]
fn logs_help_explains_filter_arguments() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("logs")
        .arg("--help")
        .output()
        .expect("logs help should run");
    assert!(output.status.success(), "logs help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("Substring filter"),
        "logs help should explain --contains"
    );
    assert!(
        stdout.contains("Case-insensitive matching"),
        "logs help should explain --ignore-case"
    );
    assert!(
        stdout.contains("Exclude filter"),
        "logs help should explain --exclude"
    );
    assert!(
        stdout.contains("Regular expression"),
        "logs help should explain --regex"
    );
    assert!(
        stdout.contains("Exclude regular expression"),
        "logs help should explain --exclude-regex"
    );
}

#[test]
fn start_help_exposes_env_file_flag() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("start")
        .arg("--help")
        .output()
        .expect("start help should run");
    assert!(output.status.success(), "start help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--env-file"),
        "start help should expose --env-file"
    );
    assert!(
        stdout.contains("Repeatable"),
        "start help should describe repeatable env files"
    );
}

#[test]
fn config_help_lists_profile_subcommands() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("config")
        .arg("--help")
        .output()
        .expect("config help should run");
    assert!(output.status.success(), "config help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("save"),
        "config help should include save subcommand"
    );
    assert!(
        stdout.contains("run"),
        "config help should include run subcommand"
    );
    assert!(
        stdout.contains("delete"),
        "config help should include delete subcommand"
    );
    assert!(
        stdout.contains("validate"),
        "config help should include validate subcommand"
    );
    assert!(
        stdout.contains("export"),
        "config help should include export subcommand"
    );
    assert!(
        stdout.contains("import"),
        "config help should include import subcommand"
    );
}

#[test]
fn config_validate_help_exposes_all_flag() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("config")
        .arg("validate")
        .arg("--help")
        .output()
        .expect("config validate help should run");
    assert!(
        output.status.success(),
        "config validate help should succeed"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--all"),
        "validate help should expose --all flag"
    );
}

#[test]
fn config_run_help_exposes_runtime_override_flags() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("config")
        .arg("run")
        .arg("--help")
        .output()
        .expect("config run help should run");
    assert!(output.status.success(), "config run help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--arg"),
        "config run help should expose --arg"
    );
    assert!(
        stdout.contains("--env"),
        "config run help should expose --env"
    );
    assert!(
        stdout.contains("--clear-args"),
        "config run help should expose --clear-args"
    );
    assert!(
        stdout.contains("--clear-env"),
        "config run help should expose --clear-env"
    );
    assert!(
        stdout.contains("--env-file"),
        "config run help should expose --env-file"
    );
    assert!(
        stdout.contains("Repeatable"),
        "config run help should describe repeatable env files"
    );
}
