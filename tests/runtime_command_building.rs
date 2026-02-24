use launch_code::model::{DebugConfig, LaunchMode, LaunchSpec, RuntimeKind};
use launch_code::runtime::build_command;

#[test]
fn python_run_command_is_built_from_entry_and_args() {
    let spec = LaunchSpec {
        name: "py-run".to_string(),
        runtime: RuntimeKind::Python,
        entry: "app.py".to_string(),
        args: vec!["--port".to_string(), "8000".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Run,
        debug: None,
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("python run command should build");
    assert_eq!(command, vec!["python", "app.py", "--port", "8000"]);
}

#[test]
fn python_debug_command_uses_debugpy_wait_for_client() {
    let spec = LaunchSpec {
        name: "py-debug".to_string(),
        runtime: RuntimeKind::Python,
        entry: "main.py".to_string(),
        args: vec!["--env".to_string(), "dev".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 5678,
            wait_for_client: true,
            subprocess: true,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("python debug command should build");
    assert_eq!(
        command,
        vec![
            "python",
            "-m",
            "debugpy",
            "--listen",
            "127.0.0.1:5678",
            "--configure-subProcess",
            "true",
            "--wait-for-client",
            "main.py",
            "--env",
            "dev"
        ]
    );
}

#[test]
fn python_debug_command_can_disable_debugpy_subprocess_injection() {
    let spec = LaunchSpec {
        name: "py-debug-no-subprocess".to_string(),
        runtime: RuntimeKind::Python,
        entry: "main.py".to_string(),
        args: vec![],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 9000,
            wait_for_client: false,
            subprocess: false,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("python debug command should build");
    assert_eq!(
        command,
        vec![
            "python",
            "-m",
            "debugpy",
            "--listen",
            "127.0.0.1:9000",
            "--configure-subProcess",
            "false",
            "main.py"
        ]
    );
}

#[test]
fn node_debug_command_uses_host_port_and_wait_flag() {
    let spec = LaunchSpec {
        name: "node-debug".to_string(),
        runtime: RuntimeKind::Node,
        entry: "app.js".to_string(),
        args: vec!["--env".to_string(), "dev".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 9229,
            wait_for_client: true,
            subprocess: true,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("node debug command should build");
    assert_eq!(
        command,
        vec![
            "node",
            "--inspect-brk=127.0.0.1:9229",
            "app.js",
            "--env",
            "dev"
        ]
    );
}

#[test]
fn node_debug_command_can_disable_wait_for_client() {
    let spec = LaunchSpec {
        name: "node-debug-no-wait".to_string(),
        runtime: RuntimeKind::Node,
        entry: "app.js".to_string(),
        args: vec![],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 9230,
            wait_for_client: false,
            subprocess: true,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("node debug command should build");
    assert_eq!(command, vec!["node", "--inspect=127.0.0.1:9230", "app.js"]);
}

#[test]
fn go_run_command_uses_go_run_with_entry_and_args() {
    let spec = LaunchSpec {
        name: "go-run".to_string(),
        runtime: RuntimeKind::Go,
        entry: "./cmd/demo".to_string(),
        args: vec!["--port".to_string(), "8080".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Run,
        debug: None,
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("go run command should build");
    assert_eq!(command, vec!["go", "run", "./cmd/demo", "--port", "8080"]);
}

#[test]
fn go_debug_command_uses_delve_headless_multiclient_listener() {
    let spec = LaunchSpec {
        name: "go-debug".to_string(),
        runtime: RuntimeKind::Go,
        entry: "./cmd/demo".to_string(),
        args: vec!["--port".to_string(), "8080".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 43000,
            wait_for_client: true,
            subprocess: false,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("go debug command should build");
    assert_eq!(
        command,
        vec![
            "dlv",
            "debug",
            "--headless",
            "--accept-multiclient",
            "--api-version=2",
            "--listen=127.0.0.1:43000",
            "./cmd/demo",
            "--",
            "--port",
            "8080"
        ]
    );
}

#[test]
fn go_debug_command_can_continue_without_waiting_for_client() {
    let spec = LaunchSpec {
        name: "go-debug-no-wait".to_string(),
        runtime: RuntimeKind::Go,
        entry: "./cmd/demo".to_string(),
        args: vec![],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 43001,
            wait_for_client: false,
            subprocess: false,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("go debug command should build");
    assert_eq!(
        command,
        vec![
            "dlv",
            "debug",
            "--headless",
            "--accept-multiclient",
            "--api-version=2",
            "--listen=127.0.0.1:43001",
            "--continue",
            "./cmd/demo",
        ]
    );
}

#[test]
fn go_debug_command_supports_test_mode_entry_prefix() {
    let spec = LaunchSpec {
        name: "go-test-debug".to_string(),
        runtime: RuntimeKind::Go,
        entry: "test:./pkg/service".to_string(),
        args: vec!["-test.run".to_string(), "TestServiceFlow".to_string()],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 43002,
            wait_for_client: true,
            subprocess: false,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("go test debug command should build");
    assert_eq!(
        command,
        vec![
            "dlv",
            "test",
            "--headless",
            "--accept-multiclient",
            "--api-version=2",
            "--listen=127.0.0.1:43002",
            "./pkg/service",
            "--",
            "-test.run",
            "TestServiceFlow"
        ]
    );
}

#[test]
fn go_debug_command_supports_attach_mode_entry_prefix() {
    let spec = LaunchSpec {
        name: "go-attach-debug".to_string(),
        runtime: RuntimeKind::Go,
        entry: "attach:34567".to_string(),
        args: vec![],
        cwd: ".".to_string(),
        env: Default::default(),
        env_remove: Vec::new(),
        managed: false,
        mode: LaunchMode::Debug,
        debug: Some(DebugConfig {
            host: "127.0.0.1".to_string(),
            port: 43003,
            wait_for_client: false,
            subprocess: false,
        }),
        prelaunch_task: None,
        poststop_task: None,
    };

    let command = build_command(&spec).expect("go attach debug command should build");
    assert_eq!(
        command,
        vec![
            "dlv",
            "attach",
            "--headless",
            "--accept-multiclient",
            "--api-version=2",
            "--listen=127.0.0.1:43003",
            "--continue",
            "34567"
        ]
    );
}
