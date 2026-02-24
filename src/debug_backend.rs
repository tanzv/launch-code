use crate::model::{DebugSessionMeta, RuntimeKind};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugBackendKind {
    PythonDebugpy,
    NodeInspector,
    GoDelve,
}

impl DebugBackendKind {
    pub fn for_runtime(runtime: &RuntimeKind) -> Option<Self> {
        match runtime {
            RuntimeKind::Python => Some(Self::PythonDebugpy),
            RuntimeKind::Node => Some(Self::NodeInspector),
            RuntimeKind::Rust => None,
            RuntimeKind::Go => Some(Self::GoDelve),
        }
    }

    pub fn requires_python_debugpy(self) -> bool {
        matches!(self, Self::PythonDebugpy)
    }

    pub fn supports_dap(self) -> bool {
        matches!(self, Self::PythonDebugpy | Self::GoDelve)
    }

    pub fn supports_dap_bootstrap(self) -> bool {
        matches!(self, Self::PythonDebugpy | Self::GoDelve)
    }

    pub fn reconnect_policy(self) -> &'static str {
        match self {
            Self::PythonDebugpy => "auto-retry",
            Self::NodeInspector => "manual-reconnect",
            Self::GoDelve => "auto-retry",
        }
    }

    pub fn adapter_kind(self) -> &'static str {
        match self {
            Self::PythonDebugpy => "python-debugpy",
            Self::NodeInspector => "node-inspector",
            Self::GoDelve => "go-delve",
        }
    }

    pub fn transport(self) -> &'static str {
        "tcp"
    }

    pub fn capabilities(self) -> &'static [&'static str] {
        match self {
            Self::PythonDebugpy => &[
                "vscode_attach",
                "dap",
                "dap_bootstrap",
                "dap_subprocess_adopt",
            ],
            Self::NodeInspector => &["vscode_attach", "inspector_attach", "dap_bridge"],
            Self::GoDelve => &["vscode_attach", "dap", "dap_bootstrap"],
        }
    }

    pub fn build_session_meta(
        self,
        host: String,
        requested_port: u16,
        active_port: u16,
        fallback_applied: bool,
    ) -> DebugSessionMeta {
        DebugSessionMeta {
            host,
            requested_port,
            active_port,
            fallback_applied,
            reconnect_policy: self.reconnect_policy().to_string(),
            adapter_kind: self.adapter_kind().to_string(),
            transport: self.transport().to_string(),
            capabilities: self
                .capabilities()
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }

    pub fn vscode_attach(self, session_name: &str, host: &str, port: u16) -> serde_json::Value {
        match self {
            Self::PythonDebugpy => json!({
                "name": format!("Attach ({session_name})"),
                "type": "python",
                "request": "attach",
                "connect": {
                    "host": host,
                    "port": port
                },
                "justMyCode": false,
                "pathMappings": [
                    {
                        "localRoot": "${workspaceFolder}",
                        "remoteRoot": "."
                    }
                ]
            }),
            Self::NodeInspector => json!({
                "name": format!("Attach ({session_name})"),
                "type": "pwa-node",
                "request": "attach",
                "address": host,
                "port": port,
                "restart": true,
                "localRoot": "${workspaceFolder}",
                "remoteRoot": "."
            }),
            Self::GoDelve => json!({
                "name": format!("Attach ({session_name})"),
                "type": "go",
                "request": "attach",
                "mode": "remote",
                "host": host,
                "port": port
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DebugBackendKind;
    use crate::model::RuntimeKind;

    #[test]
    fn runtime_to_backend_mapping_matches_current_support_matrix() {
        assert_eq!(
            DebugBackendKind::for_runtime(&RuntimeKind::Python),
            Some(DebugBackendKind::PythonDebugpy)
        );
        assert_eq!(
            DebugBackendKind::for_runtime(&RuntimeKind::Node),
            Some(DebugBackendKind::NodeInspector)
        );
        assert_eq!(DebugBackendKind::for_runtime(&RuntimeKind::Rust), None);
        assert_eq!(
            DebugBackendKind::for_runtime(&RuntimeKind::Go),
            Some(DebugBackendKind::GoDelve)
        );
    }

    #[test]
    fn dap_capability_matches_backend_matrix() {
        assert!(DebugBackendKind::PythonDebugpy.supports_dap());
        assert!(DebugBackendKind::PythonDebugpy.supports_dap_bootstrap());
        assert!(DebugBackendKind::GoDelve.supports_dap());
        assert!(DebugBackendKind::GoDelve.supports_dap_bootstrap());

        assert!(!DebugBackendKind::NodeInspector.supports_dap());
        assert!(!DebugBackendKind::NodeInspector.supports_dap_bootstrap());
    }

    #[test]
    fn backend_builds_session_meta_with_capabilities() {
        let meta = DebugBackendKind::NodeInspector.build_session_meta(
            "127.0.0.1".to_string(),
            9229,
            9229,
            false,
        );

        assert_eq!(meta.adapter_kind, "node-inspector");
        assert_eq!(meta.transport, "tcp");
        assert_eq!(meta.reconnect_policy, "manual-reconnect");
        assert_eq!(
            meta.capabilities,
            vec![
                "vscode_attach".to_string(),
                "inspector_attach".to_string(),
                "dap_bridge".to_string()
            ]
        );
    }
}
