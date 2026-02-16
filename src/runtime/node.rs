use crate::model::{LaunchMode, LaunchSpec};

use super::RuntimeError;

pub fn build(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    let mut command = vec!["node".to_string()];

    if matches!(spec.mode, LaunchMode::Debug) {
        command.push("--inspect-brk".to_string());
    }

    command.push(spec.entry.clone());
    command.extend(spec.args.clone());
    Ok(command)
}
