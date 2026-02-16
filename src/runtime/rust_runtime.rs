use crate::model::LaunchSpec;

use super::RuntimeError;

pub fn build(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    let mut command = vec![
        "cargo".to_string(),
        "run".to_string(),
        "--bin".to_string(),
        spec.entry.clone(),
    ];

    if !spec.args.is_empty() {
        command.push("--".to_string());
        command.extend(spec.args.clone());
    }

    Ok(command)
}
