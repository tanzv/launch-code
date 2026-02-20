use crate::model::{LaunchMode, LaunchSpec};

use super::RuntimeError;

pub fn build(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    let mut command = vec!["node".to_string()];

    if matches!(spec.mode, LaunchMode::Debug) {
        let debug = spec.debug.clone().unwrap_or_default();
        let inspect_flag = if debug.wait_for_client {
            "--inspect-brk"
        } else {
            "--inspect"
        };
        command.push(format!("{inspect_flag}={}:{}", debug.host, debug.port));
    }

    command.push(spec.entry.clone());
    command.extend(spec.args.clone());
    Ok(command)
}
