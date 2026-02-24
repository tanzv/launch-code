use crate::model::{LaunchMode, LaunchSpec};

use super::RuntimeError;

const GO_TEST_ENTRY_PREFIX: &str = "test:";
const GO_ATTACH_ENTRY_PREFIX: &str = "attach:";

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
            let entry = spec.entry.trim();
            let (go_mode, target) = if let Some(raw) = entry.strip_prefix(GO_ATTACH_ENTRY_PREFIX) {
                ("attach", raw.trim())
            } else if let Some(raw) = entry.strip_prefix(GO_TEST_ENTRY_PREFIX) {
                ("test", raw.trim())
            } else {
                ("debug", entry)
            };
            if target.is_empty() {
                return Err(RuntimeError::InvalidEntry(
                    "go debug target is required".to_string(),
                ));
            }

            let mut command = vec!["dlv".to_string()];
            let debug = spec.debug.clone().unwrap_or_default();
            command.push(go_mode.to_string());
            command.push("--headless".to_string());
            command.push("--accept-multiclient".to_string());
            command.push("--api-version=2".to_string());
            command.push(format!("--listen={}:{}", debug.host, debug.port));
            if !debug.wait_for_client {
                command.push("--continue".to_string());
            }
            if go_mode == "attach" {
                let pid = target.parse::<u32>().map_err(|_| {
                    RuntimeError::InvalidEntry(
                        "go attach target must be a positive process id".to_string(),
                    )
                })?;
                if pid == 0 {
                    return Err(RuntimeError::InvalidEntry(
                        "go attach target must be a positive process id".to_string(),
                    ));
                }
                command.push(pid.to_string());
                return Ok(command);
            }

            command.push(target.to_string());
            if !spec.args.is_empty() {
                command.push("--".to_string());
                command.extend(spec.args.clone());
            }
            Ok(command)
        }
    }
}
