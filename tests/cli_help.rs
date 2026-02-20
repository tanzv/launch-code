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
    assert!(
        stdout.contains("Manage workspace project metadata"),
        "top-level help should explain the project command"
    );
    assert!(
        stdout.contains("Manage global workspace links"),
        "top-level help should explain the link command"
    );
    assert!(
        stdout.contains("List only running sessions"),
        "top-level help should explain the running command"
    );
    assert!(
        stdout.contains("Run diagnostic checks for session lifecycle and debug channels"),
        "top-level help should explain the doctor command"
    );
    assert!(
        stdout.contains("Remove stale session records"),
        "top-level help should explain the cleanup command"
    );
    assert!(
        stdout.contains("--global"),
        "top-level help should expose global scope flag"
    );
    assert!(
        stdout.contains("--local"),
        "top-level help should expose local scope flag"
    );
    assert!(
        stdout.contains("--link"),
        "top-level help should expose link scope flag"
    );
}

#[test]
fn link_help_exposes_prune_subcommand() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("link")
        .arg("--help")
        .output()
        .expect("link help should run");
    assert!(output.status.success(), "link help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("prune"),
        "link help should expose prune subcommand"
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
    assert!(
        stdout.contains("Env merge order"),
        "start help should explain env merge precedence"
    );
    assert!(
        stdout.contains("--foreground"),
        "start help should expose --foreground"
    );
    assert!(stdout.contains("--tail"), "start help should expose --tail");
    assert!(
        stdout.contains("--log-mode"),
        "start help should expose --log-mode"
    );
    assert!(
        stdout.contains("stdout"),
        "start help should describe stdout log mode value"
    );
    assert!(
        stdout.contains("tee"),
        "start help should describe tee log mode value"
    );
}

#[test]
fn list_help_exposes_filter_flags() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("list")
        .arg("--help")
        .output()
        .expect("list help should run");
    assert!(output.status.success(), "list help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--status"),
        "list help should expose --status"
    );
    assert!(
        stdout.contains("--runtime"),
        "list help should expose --runtime"
    );
    assert!(
        stdout.contains("--name-contains"),
        "list help should expose --name-contains"
    );
}

#[test]
fn serve_help_exposes_worker_and_queue_options() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("serve")
        .arg("--help")
        .output()
        .expect("serve help should run");
    assert!(output.status.success(), "serve help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--workers"),
        "serve help should expose --workers"
    );
    assert!(
        stdout.contains("--token-file"),
        "serve help should expose --token-file"
    );
    assert!(
        stdout.contains("--queue-capacity"),
        "serve help should expose --queue-capacity"
    );
    assert!(
        stdout.contains("--max-body-bytes"),
        "serve help should expose --max-body-bytes"
    );
    assert!(
        stdout.contains("Set to 0 for direct handoff"),
        "serve help should explain queue-capacity=0 semantics"
    );
    assert!(
        stdout.contains("503"),
        "serve help should mention overload response behavior"
    );
    assert!(
        stdout.contains("Retry-After"),
        "serve help should mention retry hint header"
    );
}

#[test]
fn restart_help_exposes_force_and_grace_timeout_options() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("restart")
        .arg("--help")
        .output()
        .expect("restart help should run");
    assert!(output.status.success(), "restart help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--force"),
        "restart help should expose --force"
    );
    assert!(
        stdout.contains("--grace-timeout-ms"),
        "restart help should expose --grace-timeout-ms"
    );
    assert!(
        stdout.contains("--no-force"),
        "restart help should expose --no-force"
    );
    assert!(
        stdout.contains("--all"),
        "restart help should expose --all for batch mode"
    );
    assert!(
        stdout.contains("--dry-run"),
        "restart help should expose --dry-run for batch mode"
    );
    assert!(
        stdout.contains("--yes"),
        "restart help should expose --yes confirmation for global batch apply"
    );
    assert!(
        stdout.contains("--continue-on-error"),
        "restart help should expose batch continue-on-error control"
    );
    assert!(
        stdout.contains("--max-failures"),
        "restart help should expose batch max-failures control"
    );
}

#[test]
fn stop_help_exposes_batch_mode_flags() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("stop")
        .arg("--help")
        .output()
        .expect("stop help should run");
    assert!(output.status.success(), "stop help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("--id"), "stop help should expose --id");
    assert!(stdout.contains("--all"), "stop help should expose --all");
    assert!(
        stdout.contains("--name-contains"),
        "stop help should expose batch name filter"
    );
    assert!(
        stdout.contains("--dry-run"),
        "stop help should expose batch dry-run"
    );
    assert!(
        stdout.contains("--yes"),
        "stop help should expose --yes confirmation for global batch apply"
    );
    assert!(
        stdout.contains("--continue-on-error"),
        "stop help should expose batch continue-on-error control"
    );
    assert!(
        stdout.contains("--max-failures"),
        "stop help should expose batch max-failures control"
    );
}

#[test]
fn suspend_resume_help_exposes_batch_mode_flags() {
    let mut suspend_cmd = cargo_bin_cmd!("launch-code");
    let suspend_output = suspend_cmd
        .arg("suspend")
        .arg("--help")
        .output()
        .expect("suspend help should run");
    assert!(
        suspend_output.status.success(),
        "suspend help should succeed"
    );
    let suspend_stdout = String::from_utf8(suspend_output.stdout).expect("stdout should be utf8");
    assert!(
        suspend_stdout.contains("--all"),
        "suspend help should expose --all"
    );
    assert!(
        suspend_stdout.contains("--dry-run"),
        "suspend help should expose --dry-run"
    );
    assert!(
        suspend_stdout.contains("--yes"),
        "suspend help should expose --yes confirmation"
    );
    assert!(
        suspend_stdout.contains("--continue-on-error"),
        "suspend help should expose batch continue-on-error control"
    );
    assert!(
        suspend_stdout.contains("--max-failures"),
        "suspend help should expose batch max-failures control"
    );

    let mut resume_cmd = cargo_bin_cmd!("launch-code");
    let resume_output = resume_cmd
        .arg("resume")
        .arg("--help")
        .output()
        .expect("resume help should run");
    assert!(resume_output.status.success(), "resume help should succeed");
    let resume_stdout = String::from_utf8(resume_output.stdout).expect("stdout should be utf8");
    assert!(
        resume_stdout.contains("--all"),
        "resume help should expose --all"
    );
    assert!(
        resume_stdout.contains("--dry-run"),
        "resume help should expose --dry-run"
    );
    assert!(
        resume_stdout.contains("--yes"),
        "resume help should expose --yes confirmation"
    );
    assert!(
        resume_stdout.contains("--continue-on-error"),
        "resume help should expose batch continue-on-error control"
    );
    assert!(
        resume_stdout.contains("--max-failures"),
        "resume help should expose batch max-failures control"
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
fn project_help_lists_metadata_subcommands() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("project")
        .arg("--help")
        .output()
        .expect("project help should run");
    assert!(output.status.success(), "project help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("show"),
        "project help should include show subcommand"
    );
    assert!(
        stdout.contains("list"),
        "project help should include list subcommand"
    );
    assert!(
        stdout.contains("set"),
        "project help should include set subcommand"
    );
    assert!(
        stdout.contains("unset"),
        "project help should include unset subcommand"
    );
    assert!(
        stdout.contains("clear"),
        "project help should include clear subcommand"
    );
}

#[test]
fn link_help_lists_link_subcommands() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("link")
        .arg("--help")
        .output()
        .expect("link help should run");
    assert!(output.status.success(), "link help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("list"),
        "link help should include list subcommand"
    );
    assert!(
        stdout.contains("show"),
        "link help should include show subcommand"
    );
    assert!(
        stdout.contains("add"),
        "link help should include add subcommand"
    );
    assert!(
        stdout.contains("remove"),
        "link help should include remove subcommand"
    );
}

#[test]
fn cleanup_help_exposes_status_and_dry_run_flags() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("cleanup")
        .arg("--help")
        .output()
        .expect("cleanup help should run");
    assert!(output.status.success(), "cleanup help should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("--status"),
        "cleanup help should expose --status"
    );
    assert!(
        stdout.contains("--dry-run"),
        "cleanup help should expose --dry-run"
    );
    assert!(
        stdout.contains("stopped"),
        "cleanup help should list stopped status value"
    );
    assert!(
        stdout.contains("unknown"),
        "cleanup help should list unknown status value"
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
    assert!(
        stdout.contains("saved profile env"),
        "config run help should explain env precedence"
    );
    assert!(
        stdout.contains("lcode config run --name"),
        "config run help should include command examples"
    );
}
