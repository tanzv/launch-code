mod go;
mod node;
mod python;
mod rust_runtime;

use thiserror::Error;

use crate::model::{LaunchSpec, RuntimeKind};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("launch entry is required")]
    MissingEntry,
    #[error("invalid launch entry: {0}")]
    InvalidEntry(String),
}

pub fn build_command(spec: &LaunchSpec) -> Result<Vec<String>, RuntimeError> {
    if spec.entry.trim().is_empty() {
        return Err(RuntimeError::MissingEntry);
    }

    match spec.runtime {
        RuntimeKind::Python => python::build(spec),
        RuntimeKind::Node => node::build(spec),
        RuntimeKind::Rust => rust_runtime::build(spec),
        RuntimeKind::Go => go::build(spec),
    }
}

pub fn python_executable(spec: &LaunchSpec) -> String {
    python::python_executable(spec)
}
