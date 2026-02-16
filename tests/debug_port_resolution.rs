use std::net::TcpListener;

use launch_code::debug::resolve_debug_config;
use launch_code::model::DebugConfig;

#[test]
fn debug_port_falls_back_when_requested_port_is_busy() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test port should bind");
    let busy_port = listener
        .local_addr()
        .expect("listener should have local addr")
        .port();

    let resolved = resolve_debug_config(&DebugConfig {
        host: "127.0.0.1".to_string(),
        port: busy_port,
        wait_for_client: true,
        subprocess: true,
    })
    .expect("debug config should resolve");

    assert_ne!(resolved.config.port, busy_port);
    assert!(resolved.fallback_applied);
    assert_eq!(resolved.requested_port, busy_port);

    drop(listener);
}
