use crate::model::{LaunchMode, LaunchSpec};

use super::RuntimeError;

pub fn build(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    match spec.mode {
        LaunchMode::Run => {
            let mut command = vec!["go".to_string()];
            command.push("run".to_string());
            command.push(spec.entry.clone());
            command.extend(spec.args.clone());
            Ok(command)
        }
        LaunchMode::Debug => {
            let mut command = vec!["dlv".to_string()];
            let debug = spec.debug.clone().unwrap_or_default();
            command.push("dap".to_string());
            command.push(format!("--listen={}:{}", debug.host, debug.port));
            command.push(spec.entry.clone());
            if !spec.args.is_empty() {
                command.push("--".to_string());
                command.extend(spec.args.clone());
            }
            Ok(command)
        }
    }
}
