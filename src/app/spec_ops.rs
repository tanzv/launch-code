use std::collections::BTreeMap;
use std::path::Path;

use launch_code::envfile::{EnvFileError, parse_env_file_map as parse_shared_env_file_map};
use launch_code::model::{DebugConfig, LaunchMode, LaunchSpec, RuntimeKind};

use crate::cli::{LaunchModeArg, RuntimeArg, StartArgs};
use crate::error::AppError;

pub(super) fn build_launch_spec(
    args: &StartArgs,
    mode: LaunchMode,
    debug: Option<DebugConfig>,
) -> Result<LaunchSpec, AppError> {
    let runtime = to_runtime_kind(&args.runtime);
    let mut env = BTreeMap::new();
    for env_file in &args.env_file {
        env.extend(parse_env_file_map(env_file)?);
    }
    env.extend(parse_env_map(&args.env)?);
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| format!("{}-{}", runtime_label(&runtime), args.entry));

    Ok(LaunchSpec {
        name,
        runtime,
        entry: args.entry.clone(),
        args: args.args.clone(),
        cwd: args.cwd.clone(),
        env,
        env_remove: Vec::new(),
        managed: args.managed,
        mode,
        debug,
        prelaunch_task: None,
        poststop_task: None,
    })
}

pub(super) fn parse_env_map(items: &[String]) -> Result<BTreeMap<String, String>, AppError> {
    let mut env_map = BTreeMap::new();
    for item in items {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| AppError::InvalidEnvPair(item.clone()))?;
        env_map.insert(key.to_string(), value.to_string());
    }
    Ok(env_map)
}

pub(super) fn parse_env_file_map(path: &Path) -> Result<BTreeMap<String, String>, AppError> {
    parse_shared_env_file_map(path).map_err(|err| match err {
        EnvFileError::InvalidLine(line) => AppError::InvalidEnvFileLine(line),
        EnvFileError::Io(io) => AppError::Io(io),
    })
}

pub(super) fn to_runtime_kind(runtime: &RuntimeArg) -> RuntimeKind {
    match runtime {
        RuntimeArg::Python => RuntimeKind::Python,
        RuntimeArg::Node => RuntimeKind::Node,
        RuntimeArg::Rust => RuntimeKind::Rust,
        RuntimeArg::Go => RuntimeKind::Go,
    }
}

pub(super) fn to_launch_mode(mode: &LaunchModeArg) -> LaunchMode {
    match mode {
        LaunchModeArg::Run => LaunchMode::Run,
        LaunchModeArg::Debug => LaunchMode::Debug,
    }
}

pub(super) fn runtime_label(runtime: &RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::Python => "python",
        RuntimeKind::Node => "node",
        RuntimeKind::Rust => "rust",
        RuntimeKind::Go => "go",
    }
}

pub(super) fn mode_label(mode: &LaunchMode) -> &'static str {
    match mode {
        LaunchMode::Run => "run",
        LaunchMode::Debug => "debug",
    }
}
