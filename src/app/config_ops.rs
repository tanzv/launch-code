use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use launch_code::debug_backend::DebugBackendKind;
use launch_code::model::{DebugConfig, LaunchMode, LaunchSpec, RuntimeKind};
use launch_code::runtime::{build_command, python_executable};
use launch_code::state::StateStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cli::{
    ConfigArgs, ConfigCommands, ConfigExportArgs, ConfigImportArgs, ConfigNameArgs, ConfigRunArgs,
    ConfigSaveArgs, ConfigValidateArgs,
};
use crate::error::AppError;
use crate::output;

#[derive(Debug, Serialize, Deserialize)]
struct ProfileBundle {
    version: u32,
    profiles: BTreeMap<String, LaunchSpec>,
}

const PROFILE_BUNDLE_VERSION: u32 = 1;

pub(super) fn handle_config(store: &StateStore, args: &ConfigArgs) -> Result<(), AppError> {
    match &args.command {
        ConfigCommands::List => handle_config_list(store),
        ConfigCommands::Show(args) => handle_config_show(store, args),
        ConfigCommands::Save(args) => handle_config_save(store, args),
        ConfigCommands::Delete(args) => handle_config_delete(store, args),
        ConfigCommands::Run(args) => handle_config_run(store, args),
        ConfigCommands::Validate(args) => handle_config_validate(store, args),
        ConfigCommands::Export(args) => handle_config_export(store, args),
        ConfigCommands::Import(args) => handle_config_import(store, args),
    }
}

fn handle_config_list(store: &StateStore) -> Result<(), AppError> {
    let state = store.load()?;
    if output::is_json_mode() {
        let items = state
            .profiles
            .iter()
            .map(|(name, spec)| {
                json!({
                    "name": name,
                    "runtime": super::spec_ops::runtime_label(&spec.runtime),
                    "mode": super::spec_ops::mode_label(&spec.mode),
                    "entry": spec.entry,
                    "managed": spec.managed,
                })
            })
            .collect::<Vec<serde_json::Value>>();
        output::print_json_doc(&json!({
            "ok": true,
            "items": items,
        }));
        return Ok(());
    }

    if state.profiles.is_empty() {
        output::print_message("no profiles");
        return Ok(());
    }

    let lines = state
        .profiles
        .iter()
        .map(|(name, spec)| {
            format!(
                "{}\t{}\t{}\t{}\tmanaged={}",
                name,
                super::spec_ops::runtime_label(&spec.runtime),
                super::spec_ops::mode_label(&spec.mode),
                spec.entry,
                spec.managed
            )
        })
        .collect::<Vec<String>>();
    output::print_lines(&lines);
    Ok(())
}

fn handle_config_show(store: &StateStore, args: &ConfigNameArgs) -> Result<(), AppError> {
    let state = store.load()?;
    let spec = state
        .profiles
        .get(&args.name)
        .ok_or_else(|| AppError::ProfileNotFound(args.name.clone()))?;
    output::print_json_doc(&json!({
        "ok": true,
        "profile": spec,
    }));
    Ok(())
}

fn handle_config_save(store: &StateStore, args: &ConfigSaveArgs) -> Result<(), AppError> {
    let spec = build_profile_spec(args)?;
    let profile_name = args.name.clone();
    store.update::<_, _, AppError>(|state| {
        state.profiles.insert(profile_name.clone(), spec);
        Ok(())
    })?;
    output::print_message(&format!("profile={} saved=true", args.name));
    Ok(())
}

fn handle_config_delete(store: &StateStore, args: &ConfigNameArgs) -> Result<(), AppError> {
    let removed = store.update::<_, _, AppError>(|state| Ok(state.profiles.remove(&args.name)))?;
    if removed.is_none() {
        return Err(AppError::ProfileNotFound(args.name.clone()));
    }
    output::print_message(&format!("profile={} deleted=true", args.name));
    Ok(())
}

fn handle_config_run(store: &StateStore, args: &ConfigRunArgs) -> Result<(), AppError> {
    let state = store.load()?;
    let mut spec = state
        .profiles
        .get(&args.name)
        .cloned()
        .ok_or_else(|| AppError::ProfileNotFound(args.name.clone()))?;

    if let Some(mode) = &args.mode {
        spec.mode = super::spec_ops::to_launch_mode(mode);
        if matches!(spec.mode, LaunchMode::Debug) {
            if spec.debug.is_none() {
                spec.debug = Some(DebugConfig::default());
            }
        } else {
            spec.debug = None;
        }
    }

    if args.managed {
        spec.managed = true;
    }

    if args.clear_args {
        spec.args.clear();
    }

    if !args.args.is_empty() {
        spec.args.extend(args.args.clone());
    }

    if args.clear_env {
        spec.env.clear();
        spec.env_remove.clear();
    }

    for env_file in &args.env_file {
        let env_map = super::spec_ops::parse_env_file_map(env_file)?;
        remove_env_remove_keys(&mut spec.env_remove, env_map.keys());
        spec.env.extend(env_map);
    }

    if !args.env.is_empty() {
        let overrides = super::spec_ops::parse_env_map(&args.env)?;
        remove_env_remove_keys(&mut spec.env_remove, overrides.keys());
        spec.env.extend(overrides);
    }

    super::handle_start_spec(store, spec, super::StartExecutionOptions::default())
}

fn handle_config_validate(store: &StateStore, args: &ConfigValidateArgs) -> Result<(), AppError> {
    if args.all {
        let state = store.load()?;
        let mut validated_profiles = 0usize;
        let mut items = Vec::<serde_json::Value>::new();

        for (name, spec) in &state.profiles {
            match validate_profile_spec(spec) {
                Ok(checks) => {
                    validated_profiles += 1;
                    if output::is_json_mode() {
                        items.push(json!({
                            "name": name,
                            "valid": true,
                            "checks": checks,
                        }));
                    }
                }
                Err(AppError::ProfileValidationFailed(message)) => {
                    return Err(AppError::ProfileValidationFailed(format!(
                        "profile `{name}`: {message}"
                    )));
                }
                Err(other) => return Err(other),
            }
        }

        if output::is_json_mode() {
            output::print_json_doc(&json!({
                "ok": true,
                "all": true,
                "validated_profiles": validated_profiles,
                "items": items,
            }));
        } else {
            output::print_message(&format!(
                "validated_profiles={validated_profiles} valid=true"
            ));
        }
        return Ok(());
    }

    let profile_name = args
        .name
        .as_ref()
        .ok_or_else(|| AppError::ProfileValidationFailed("missing profile name".to_string()))?;
    let state = store.load()?;
    let spec = state
        .profiles
        .get(profile_name)
        .cloned()
        .ok_or_else(|| AppError::ProfileNotFound(profile_name.clone()))?;

    let checks = validate_profile_spec(&spec)?;
    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "profile": profile_name,
            "valid": true,
            "checks": checks,
        }));
    } else {
        output::print_message(&format!("profile={profile_name} valid=true"));
    }
    Ok(())
}

fn handle_config_export(store: &StateStore, args: &ConfigExportArgs) -> Result<(), AppError> {
    let state = store.load()?;
    if let Some(parent) = args
        .file
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let bundle = ProfileBundle {
        version: PROFILE_BUNDLE_VERSION,
        profiles: state.profiles,
    };
    let payload = serde_json::to_string_pretty(&bundle)?;
    fs::write(&args.file, payload)?;
    output::print_message(&format!(
        "profiles_exported={} file={}",
        bundle.profiles.len(),
        args.file.display()
    ));
    Ok(())
}

fn handle_config_import(store: &StateStore, args: &ConfigImportArgs) -> Result<(), AppError> {
    let payload = fs::read_to_string(&args.file)?;
    let bundle: ProfileBundle = serde_json::from_str(&payload)?;
    if bundle.version != PROFILE_BUNDLE_VERSION {
        return Err(AppError::ProfileBundleVersionUnsupported(bundle.version));
    }

    let imported = bundle.profiles.len();
    let profiles = bundle.profiles;
    store.update::<_, _, AppError>(|state| {
        if args.replace {
            state.profiles.clear();
        }
        for (name, spec) in profiles {
            state.profiles.insert(name, spec);
        }
        Ok(())
    })?;
    output::print_message(&format!(
        "profiles_imported={} replace={} file={}",
        imported,
        args.replace,
        args.file.display()
    ));
    Ok(())
}

fn build_profile_spec(args: &ConfigSaveArgs) -> Result<LaunchSpec, AppError> {
    let runtime = super::spec_ops::to_runtime_kind(&args.runtime);
    let mode = super::spec_ops::to_launch_mode(&args.mode);
    let debug = if matches!(mode, LaunchMode::Debug) {
        Some(DebugConfig {
            host: args.host.clone(),
            port: args.port,
            wait_for_client: args.wait_for_client,
            subprocess: args.subprocess,
        })
    } else {
        None
    };

    Ok(LaunchSpec {
        name: args.name.clone(),
        runtime,
        entry: args.entry.clone(),
        args: args.args.clone(),
        cwd: args.cwd.clone(),
        env: super::spec_ops::parse_env_map(&args.env)?,
        env_remove: Vec::new(),
        managed: args.managed,
        mode,
        debug,
        prelaunch_task: args.prelaunch_task.clone(),
        poststop_task: args.poststop_task.clone(),
    })
}

fn validate_profile_spec(spec: &LaunchSpec) -> Result<Vec<String>, AppError> {
    let mut checks = Vec::new();

    let cwd = Path::new(&spec.cwd);
    if !cwd.exists() {
        return Err(AppError::ProfileValidationFailed(format!(
            "cwd not found: {}",
            spec.cwd
        )));
    }
    if !cwd.is_dir() {
        return Err(AppError::ProfileValidationFailed(format!(
            "cwd is not a directory: {}",
            spec.cwd
        )));
    }
    checks.push(format!("cwd_exists={}", spec.cwd));

    match spec.runtime {
        RuntimeKind::Rust => {
            let cargo_manifest = cwd.join("Cargo.toml");
            if !cargo_manifest.exists() {
                return Err(AppError::ProfileValidationFailed(format!(
                    "Cargo.toml not found for rust runtime: {}",
                    cargo_manifest.display()
                )));
            }
            checks.push(format!(
                "cargo_manifest_exists={}",
                cargo_manifest.display()
            ));
        }
        RuntimeKind::Python | RuntimeKind::Node | RuntimeKind::Go => {
            let entry_path = if Path::new(&spec.entry).is_absolute() {
                PathBuf::from(&spec.entry)
            } else {
                cwd.join(&spec.entry)
            };
            if !entry_path.exists() {
                return Err(AppError::ProfileValidationFailed(format!(
                    "entry not found: {}",
                    entry_path.display()
                )));
            }
            checks.push(format!("entry_exists={}", entry_path.display()));
        }
    }

    build_command(spec)
        .map_err(|err| AppError::ProfileValidationFailed(format!("build command failed: {err}")))?;
    checks.push("command_buildable=true".to_string());

    if matches!(spec.mode, LaunchMode::Debug) {
        let Some(backend) = DebugBackendKind::for_runtime(&spec.runtime) else {
            return Err(AppError::ProfileValidationFailed(format!(
                "debug mode currently supports python, node, and go runtimes only; found {}",
                super::spec_ops::runtime_label(&spec.runtime)
            )));
        };

        if spec.debug.is_none() {
            return Err(AppError::ProfileValidationFailed(
                "debug mode requires debug config".to_string(),
            ));
        }
        checks.push("debug_config_present=true".to_string());

        if backend.requires_python_debugpy() {
            let interpreter = python_executable(spec);
            let mut cmd = ProcessCommand::new(&interpreter);
            cmd.arg("-c").arg("import debugpy").current_dir(cwd);
            for key in &spec.env_remove {
                cmd.env_remove(key);
            }
            let status = cmd
                .envs(spec.env.iter())
                .output()
                .map_err(|err| {
                    AppError::ProfileValidationFailed(format!(
                        "python debugpy check failed for interpreter `{interpreter}`: {err}"
                    ))
                })?
                .status;
            if !status.success() {
                return Err(AppError::ProfileValidationFailed(format!(
                    "debugpy unavailable for interpreter `{interpreter}`"
                )));
            }
            checks.push(format!("python_debugpy_ready={interpreter}"));
        }
        if matches!(backend, DebugBackendKind::GoDelve) {
            let mut cmd = ProcessCommand::new("dlv");
            cmd.arg("version").current_dir(cwd);
            for key in &spec.env_remove {
                cmd.env_remove(key);
            }
            let status = cmd
                .envs(spec.env.iter())
                .output()
                .map_err(|err| {
                    AppError::ProfileValidationFailed(format!("dlv version check failed: {err}"))
                })?
                .status;
            if !status.success() {
                return Err(AppError::ProfileValidationFailed(
                    "dlv unavailable in PATH for go debug runtime".to_string(),
                ));
            }
            checks.push("go_dlv_ready=true".to_string());
        }
    }

    Ok(checks)
}

fn remove_env_remove_keys<'a>(
    env_remove: &mut Vec<String>,
    keys: impl Iterator<Item = &'a String>,
) {
    for key in keys {
        env_remove.retain(|item| item != key);
    }
}
