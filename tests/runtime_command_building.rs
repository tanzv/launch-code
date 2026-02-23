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
