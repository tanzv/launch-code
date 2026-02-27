use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, ExitStatus};

use serde_json::json;

use crate::cli::{BuildArgs, BuildPlatformArg, RuntimeArg};
use crate::error::AppError;
use crate::output;

const SUPPORTED_PLATFORM_TEXT: &str =
    "linux/amd64, linux/arm64, darwin/amd64, darwin/arm64, windows/amd64, windows/arm64";

#[derive(Debug, Clone)]
struct BuildPlan {
    runtime: String,
    cwd: String,
    command: Vec<String>,
    env: BTreeMap<String, String>,
    platform: Option<String>,
    dry_run: bool,
}

pub(super) fn handle_build(args: &BuildArgs) -> Result<(), AppError> {
    let mut env_map = BTreeMap::new();
    for env_file in &args.env_file {
        env_map.extend(super::spec_ops::parse_env_file_map(env_file)?);
    }
    env_map.extend(super::spec_ops::parse_env_map(&args.env)?);

    let plan = match args.runtime {
        RuntimeArg::Rust => build_plan_for_rust(args, env_map)?,
        RuntimeArg::Go => build_plan_for_go(args, env_map)?,
        RuntimeArg::Python | RuntimeArg::Node => {
            return Err(AppError::UnsupportedBuildRuntime(runtime_label(&args.runtime).to_string()))
        }
    };

    if plan.dry_run {
        print_build_plan(&plan, None);
        return Ok(());
    }

    let status = execute_build_plan(&plan)?;
    if !status.success() {
        return Err(AppError::BuildFailed(format!(
            "command exited with status {status}: {}",
            render_command(&plan.command)
        )));
    }
    print_build_plan(&plan, status.code());
    Ok(())
}

fn build_plan_for_rust(
    args: &BuildArgs,
    env_map: BTreeMap<String, String>,
) -> Result<BuildPlan, AppError> {
    validate_non_empty_runtime_entry(args.entry.as_deref())?;
    if args.goos.is_some() {
        return Err(invalid_build_options(
            "`--goos` is only supported for runtime go",
        ));
    }
    if args.goarch.is_some() {
        return Err(invalid_build_options(
            "`--goarch` is only supported for runtime go",
        ));
    }
    if args.cgo_enabled.is_some() {
        return Err(invalid_build_options(
            "`--cgo-enabled` is only supported for runtime go",
        ));
    }
    if args.output.is_some() {
        return Err(invalid_build_options(
            "`--output` is only supported for runtime go",
        ));
    }
    if args.platform.is_some() && args.target.is_some() {
        return Err(invalid_build_options(
            "choose either `--platform` or `--target` for runtime rust",
        ));
    }

    let mut command = vec!["cargo".to_string(), "build".to_string()];
    if args.release {
        command.push("--release".to_string());
    }
    if let Some(target_dir) = args.target_dir.as_deref() {
        command.push("--target-dir".to_string());
        command.push(target_dir.to_string());
    }

    let resolved_target = if let Some(target) = args.target.clone() {
        Some(target)
    } else if let Some(platform) = args.platform.as_ref() {
        Some(map_rust_target(platform)?)
    } else {
        None
    };

    if let Some(target) = resolved_target {
        command.push("--target".to_string());
        command.push(target);
    }

    if args.no_default_features {
        command.push("--no-default-features".to_string());
    }
    for feature in &args.features {
        command.push("--features".to_string());
        command.push(feature.clone());
    }
    if let Some(entry) = args.entry.as_deref() {
        command.push("--bin".to_string());
        command.push(entry.to_string());
    }
    command.extend(args.args.clone());

    Ok(BuildPlan {
        runtime: "rust".to_string(),
        cwd: args.cwd.clone(),
        command,
        env: env_map,
        platform: args.platform.as_ref().map(BuildPlatformArg::normalized),
        dry_run: args.dry_run,
    })
}

fn build_plan_for_go(
    args: &BuildArgs,
    mut env_map: BTreeMap<String, String>,
) -> Result<BuildPlan, AppError> {
    validate_non_empty_runtime_entry(args.entry.as_deref())?;
    if args.target.is_some() {
        return Err(invalid_build_options(
            "`--target` is only supported for runtime rust",
        ));
    }
    if args.target_dir.is_some() {
        return Err(invalid_build_options(
            "`--target-dir` is only supported for runtime rust",
        ));
    }
    if !args.features.is_empty() {
        return Err(invalid_build_options(
            "`--feature` is only supported for runtime rust",
        ));
    }
    if args.no_default_features {
        return Err(invalid_build_options(
            "`--no-default-features` is only supported for runtime rust",
        ));
    }
    if args.release {
        return Err(invalid_build_options(
            "`--release` is only supported for runtime rust",
        ));
    }

    let mut goos = sanitize_optional_flag(args.goos.as_deref(), "goos")?;
    let mut goarch = sanitize_optional_flag(args.goarch.as_deref(), "goarch")?;
    if let Some(platform) = args.platform.as_ref() {
        let (mapped_goos, mapped_goarch) = map_go_platform(platform)?;
        if goos.is_none() {
            goos = Some(mapped_goos.to_string());
        }
        if goarch.is_none() {
            goarch = Some(mapped_goarch.to_string());
        }
    }
    if let Some(value) = goos {
        env_map.insert("GOOS".to_string(), value);
    }
    if let Some(value) = goarch {
        env_map.insert("GOARCH".to_string(), value);
    }
    if let Some(cgo_enabled) = args.cgo_enabled {
        env_map.insert(
            "CGO_ENABLED".to_string(),
            if cgo_enabled {
                "1".to_string()
            } else {
                "0".to_string()
            },
        );
    }

    let mut command = vec!["go".to_string(), "build".to_string()];
    if let Some(output) = args.output.as_deref() {
        if output.trim().is_empty() {
            return Err(invalid_build_options("output must not be empty"));
        }
        command.push("-o".to_string());
        command.push(output.to_string());
    }
    command.push(args.entry.clone().unwrap_or_else(|| ".".to_string()));
    command.extend(args.args.clone());

    Ok(BuildPlan {
        runtime: "go".to_string(),
        cwd: args.cwd.clone(),
        command,
        env: env_map,
        platform: args.platform.as_ref().map(BuildPlatformArg::normalized),
        dry_run: args.dry_run,
    })
}

fn map_rust_target(platform: &BuildPlatformArg) -> Result<String, AppError> {
    let target = match (platform.os.as_str(), platform.arch.as_str()) {
        ("linux", "amd64") => "x86_64-unknown-linux-gnu",
        ("linux", "arm64") => "aarch64-unknown-linux-gnu",
        ("darwin", "amd64") => "x86_64-apple-darwin",
        ("darwin", "arm64") => "aarch64-apple-darwin",
        ("windows", "amd64") => "x86_64-pc-windows-msvc",
        ("windows", "arm64") => "aarch64-pc-windows-msvc",
        _ => {
            return Err(invalid_build_options(format!(
                "unsupported platform `{}`; supported values: {SUPPORTED_PLATFORM_TEXT}",
                platform.normalized()
            )));
        }
    };
    Ok(target.to_string())
}

fn map_go_platform(platform: &BuildPlatformArg) -> Result<(&'static str, &'static str), AppError> {
    let mapped = match (platform.os.as_str(), platform.arch.as_str()) {
        ("linux", "amd64") => ("linux", "amd64"),
        ("linux", "arm64") => ("linux", "arm64"),
        ("darwin", "amd64") => ("darwin", "amd64"),
        ("darwin", "arm64") => ("darwin", "arm64"),
        ("windows", "amd64") => ("windows", "amd64"),
        ("windows", "arm64") => ("windows", "arm64"),
        _ => {
            return Err(invalid_build_options(format!(
                "unsupported platform `{}`; supported values: {SUPPORTED_PLATFORM_TEXT}",
                platform.normalized()
            )));
        }
    };
    Ok(mapped)
}

fn sanitize_optional_flag(raw: Option<&str>, flag_name: &str) -> Result<Option<String>, AppError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid_build_options(format!("`--{flag_name}` must not be empty")));
    }
    Ok(Some(trimmed.to_ascii_lowercase()))
}

fn validate_non_empty_runtime_entry(entry: Option<&str>) -> Result<(), AppError> {
    if entry.is_some_and(|value| value.trim().is_empty()) {
        return Err(invalid_build_options("entry must not be empty"));
    }
    Ok(())
}

fn execute_build_plan(plan: &BuildPlan) -> Result<ExitStatus, AppError> {
    let (program, args) = plan.command.split_first().ok_or_else(|| {
        AppError::BuildFailed("internal error: resolved build command is empty".to_string())
    })?;
    let mut command = Command::new(program);
    command.args(args);
    command.current_dir(Path::new(&plan.cwd));
    for (key, value) in &plan.env {
        command.env(key, value);
    }
    Ok(command.status()?)
}

fn render_command(command: &[String]) -> String {
    command
        .iter()
        .map(|value| {
            if value.contains(' ') {
                format!("{value:?}")
            } else {
                value.clone()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn print_build_plan(plan: &BuildPlan, exit_code: Option<i32>) {
    if output::is_json_mode() {
        let mut payload = json!({
            "ok": true,
            "runtime": &plan.runtime,
            "cwd": &plan.cwd,
            "platform": &plan.platform,
            "command": &plan.command,
            "env": &plan.env,
            "dry_run": plan.dry_run,
            "executed": !plan.dry_run,
        });
        if let Some(code) = exit_code && let Some(doc) = payload.as_object_mut() {
            doc.insert("exit_code".to_string(), json!(code));
        }
        output::print_json_doc(&payload);
        return;
    }

    println!("runtime={}", plan.runtime);
    println!("cwd={}", plan.cwd);
    if let Some(platform) = &plan.platform {
        println!("platform={platform}");
    }
    if plan.env.is_empty() {
        println!("env=-");
    } else {
        let env_line = plan
            .env
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<String>>()
            .join(" ");
        println!("env={env_line}");
    }
    println!("command={}", render_command(&plan.command));
    if plan.dry_run {
        println!("dry_run=true executed=false");
    } else if let Some(code) = exit_code {
        println!("dry_run=false executed=true exit_code={code}");
    } else {
        println!("dry_run=false executed=true");
    }
}

fn runtime_label(runtime: &RuntimeArg) -> &'static str {
    match runtime {
        RuntimeArg::Python => "python",
        RuntimeArg::Node => "node",
        RuntimeArg::Rust => "rust",
        RuntimeArg::Go => "go",
    }
}

fn invalid_build_options(message: impl Into<String>) -> AppError {
    AppError::InvalidBuildOptions(message.into())
}
