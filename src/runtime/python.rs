use std::path::Path;

use crate::model::{LaunchMode, LaunchSpec};

use super::RuntimeError;

pub fn build(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    let mut command = vec![python_executable(spec)];

    match spec.mode {
        LaunchMode::Run => {
            command.push(spec.entry.clone());
            command.extend(spec.args.clone());
        }
        LaunchMode::Debug => {
            let debug = spec.debug.clone().unwrap_or_default();
            command.extend(["-m", "debugpy"].into_iter().map(str::to_string));
            command.push("--listen".to_string());
            command.push(format!("{}:{}", debug.host, debug.port));
            command.push("--configure-subProcess".to_string());
            command.push(if debug.subprocess {
                "true".to_string()
            } else {
                "false".to_string()
            });

            if debug.wait_for_client {
                command.push("--wait-for-client".to_string());
            }

            command.push(spec.entry.clone());
            command.extend(spec.args.clone());
        }
    }

    Ok(command)
}

pub fn python_executable(spec: &LaunchSpec) -> String {
    if let Some(explicit) = spec.env.get("PYTHON_BIN") {
        return explicit.clone();
    }

    let cwd = Path::new(&spec.cwd);
    for candidate in [
        ".venv/bin/python",
        "venv/bin/python",
        ".venv/Scripts/python.exe",
        "venv/Scripts/python.exe",
    ] {
        let path = cwd.join(candidate);
        if path.exists() {
            return path.to_string_lossy().to_string();
        }
    }

    "python".to_string()
}
