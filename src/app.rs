mod config_ops;
mod dap_cli;
mod doctor_ops;
mod link_ops;
mod log_ops;
mod project_api;
mod project_ops;
mod serve_ops;
mod session_api;
mod session_cli;
mod spec_ops;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread;
use std::time::Duration;

use launch_code::config::{LaunchRequest, load_launch_spec};
use launch_code::debug::resolve_debug_config;
use launch_code::model::{
    AppState, DebugConfig, DebugSessionMeta, LaunchMode, LaunchSpec, RuntimeKind, SessionRecord,
    SessionStatus, unix_timestamp_secs,
};
use launch_code::process::{
    ProcessLogMode, is_process_alive, run_process_foreground, run_shell_task, spawn_process,
};
use launch_code::runtime::build_command;
use launch_code::runtime::python_executable;
use launch_code::state::StateStore;
use serde_json::json;
use uuid::Uuid;

use crate::cli::{
    CleanupArgs, Commands, DaemonArgs, DebugArgs, InspectArgs, LaunchArgs, ListArgs, LogsArgs,
    RestartArgs, ResumeArgs, RunningArgs, SessionIdArgs, StartArgs, StartLogModeArg, StopArgs,
    SuspendArgs,
};
use crate::error::AppError;
use crate::output;

pub(crate) use project_api::{
    ProjectField, ProjectUpdate, api_get_project_info, api_unset_project_info_fields,
    api_update_project_info,
};
pub(crate) use session_api::{
    api_cleanup_sessions, api_cleanup_sessions_with_options, api_debug_session, api_get_session,
    api_inspect_session, api_list_sessions, api_restart_session_with_options, api_resume_session,
    api_stop_session_with_options, api_suspend_session,
};

pub(crate) fn execute(store: &StateStore, command: Commands) -> Result<(), AppError> {
    match command {
        Commands::Start(args) => {
            let options = start_options_from_args(&args)?;
            let spec = spec_ops::build_launch_spec(&args, LaunchMode::Run, None)?;
            handle_start_spec(store, spec, options)
        }
        Commands::Debug(args) => handle_debug(store, &args),
        Commands::Launch(args) => handle_launch(store, &args),
        Commands::Attach(args) => handle_attach(store, &args),
        Commands::Inspect(args) => handle_inspect(store, &args),
        Commands::Logs(args) => handle_logs(store, &args),
        Commands::Stop(args) => session_cli::handle_stop(store, &args),
        Commands::Restart(args) => session_cli::handle_restart(store, &args),
        Commands::Suspend(args) => session_cli::handle_suspend(store, &args),
        Commands::Resume(args) => session_cli::handle_resume(store, &args),
        Commands::Status(args) => session_cli::handle_status(store, &args),
        Commands::List(args) => session_cli::handle_list(store, &args),
        Commands::Running(args) => session_cli::handle_running(store, &args),
        Commands::Cleanup(args) => session_cli::handle_cleanup(store, &args),
        Commands::Config(args) => config_ops::handle_config(store, &args),
        Commands::Project(args) => project_ops::handle_project(store, &args),
        Commands::Link(args) => link_ops::handle_link(&args),
        Commands::Daemon(args) => handle_daemon(store, &args),
        Commands::Serve(args) => serve_ops::handle_serve(store, &args),
        Commands::Dap(args) => dap_cli::handle_dap(store, &args),
        Commands::Doctor(args) => doctor_ops::handle_doctor(store, &args),
    }
}

pub(crate) fn execute_global_list(args: &ListArgs, workspace_root: &Path) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_list_global_default(args)
}

pub(crate) fn execute_global_running(
    workspace_root: &Path,
    args: &RunningArgs,
) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_running_global_default(args)
}

pub(crate) fn execute_global_cleanup(
    args: &CleanupArgs,
    workspace_root: &Path,
) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_cleanup_global_default(args)
}

pub(crate) fn execute_global_stop(args: &StopArgs, workspace_root: &Path) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_stop_global_default(args)
}

pub(crate) fn execute_global_restart(
    args: &RestartArgs,
    workspace_root: &Path,
) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_restart_global_default(args)
}

pub(crate) fn execute_global_suspend(
    args: &SuspendArgs,
    workspace_root: &Path,
) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_suspend_global_default(args)
}

pub(crate) fn execute_global_resume(
    args: &ResumeArgs,
    workspace_root: &Path,
) -> Result<(), AppError> {
    crate::link_registry::ensure_link_for_workspace(workspace_root)?;
    session_cli::handle_resume_global_default(args)
}

pub(crate) fn execute_global_project_show() -> Result<(), AppError> {
    project_ops::handle_project_show_global_default()
}

pub(crate) fn resolve_global_store_for_session_id(
    session_id: &str,
    current_store: &StateStore,
) -> Result<Option<StateStore>, AppError> {
    let current_root = normalize_lookup_path(current_store.root_path());

    if let Some(cached_path) = crate::session_lookup::lookup_session_path(session_id)? {
        let cached_store = StateStore::new(cached_path);
        if normalize_lookup_path(cached_store.root_path()) != current_root {
            let cached_state = cached_store.load();
            if matches!(
                cached_state,
                Ok(ref state) if state.sessions.contains_key(session_id)
            ) {
                return Ok(Some(cached_store));
            }
        }
        let _ = crate::session_lookup::remove_session_mapping(session_id);
    }

    let registry = crate::link_registry::load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut prefix_candidates: Vec<(String, String, PathBuf)> = Vec::new();

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }

        let store = StateStore::new(&item.path);
        if normalize_lookup_path(store.root_path()) == current_root {
            continue;
        }

        let state = match store.load() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if state.sessions.contains_key(session_id) {
            let _ = crate::session_lookup::upsert_session_path(session_id, store.root_path());
            return Ok(Some(store));
        }

        let mut scoped_matches: Vec<String> = state
            .sessions
            .keys()
            .filter(|candidate| candidate.starts_with(session_id))
            .cloned()
            .collect();
        if scoped_matches.len() > 1 {
            scoped_matches.sort();
            let preview = scoped_matches
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<String>>()
                .join(",");
            return Err(AppError::SessionIdAmbiguous(format!(
                "{session_id}; matches={preview}"
            )));
        }
        if let Some(full_id) = scoped_matches.pop() {
            prefix_candidates.push((full_id, item.name, PathBuf::from(item.path)));
        }
    }

    if prefix_candidates.len() > 1 {
        prefix_candidates.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        let preview = prefix_candidates
            .iter()
            .take(5)
            .map(|(id, link, _)| format!("{id}@{link}"))
            .collect::<Vec<String>>()
            .join(",");
        return Err(AppError::SessionIdAmbiguous(format!(
            "{session_id}; matches={preview}"
        )));
    }
    if let Some((full_id, _, root_path)) = prefix_candidates.pop() {
        let store = StateStore::new(&root_path);
        let _ = crate::session_lookup::upsert_session_path(&full_id, store.root_path());
        return Ok(Some(store));
    }

    Ok(None)
}

fn normalize_lookup_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StartExecutionOptions {
    foreground: bool,
    tail: bool,
    log_mode: ProcessLogMode,
}

impl Default for StartExecutionOptions {
    fn default() -> Self {
        Self {
            foreground: false,
            tail: false,
            log_mode: ProcessLogMode::File,
        }
    }
}

fn start_options_from_args(args: &StartArgs) -> Result<StartExecutionOptions, AppError> {
    let log_mode = match args.log_mode {
        StartLogModeArg::File => ProcessLogMode::File,
        StartLogModeArg::Stdout => ProcessLogMode::Stdout,
        StartLogModeArg::Tee => ProcessLogMode::Tee,
    };
    let options = StartExecutionOptions {
        foreground: args.foreground,
        tail: args.tail,
        log_mode,
    };
    validate_start_options(options)?;
    Ok(options)
}

fn validate_start_options(options: StartExecutionOptions) -> Result<(), AppError> {
    if options.foreground && options.tail {
        return Err(AppError::InvalidStartOptions(
            "`--tail` cannot be used with `--foreground`.".to_string(),
        ));
    }

    if !options.foreground && !matches!(options.log_mode, ProcessLogMode::File) {
        return Err(AppError::InvalidStartOptions(
            "`--log-mode stdout|tee` requires `--foreground`.".to_string(),
        ));
    }

    Ok(())
}

fn handle_debug(store: &StateStore, args: &DebugArgs) -> Result<(), AppError> {
    let options = start_options_from_args(&args.start)?;
    let debug = DebugConfig {
        host: args.host.clone(),
        port: args.port,
        wait_for_client: args.wait_for_client,
        subprocess: args.subprocess,
    };
    let spec = spec_ops::build_launch_spec(&args.start, LaunchMode::Debug, Some(debug))?;
    handle_start_spec(store, spec, options)
}

fn handle_launch(store: &StateStore, args: &LaunchArgs) -> Result<(), AppError> {
    let request = LaunchRequest {
        name: args.name.clone(),
        mode: spec_ops::to_launch_mode(&args.mode),
        managed_override: args.managed.then_some(true),
        launch_file: args.launch_file.clone(),
    };
    let spec = load_launch_spec(store.root_path(), &request)?;
    handle_start_spec(store, spec, StartExecutionOptions::default())
}

fn handle_attach(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let session = api_get_session(store, session_id)?;
    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;

    let doc = build_debug_session_doc(&session, meta);
    output::print_json_doc(&doc);
    Ok(())
}

fn handle_inspect(store: &StateStore, args: &InspectArgs) -> Result<(), AppError> {
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let tail_lines = args.tail.min(log_ops::MAX_LOG_TAIL_LINES);
    let doc = api_inspect_session(store, session_id, tail_lines)?;
    output::print_json_doc(&doc);
    Ok(())
}

fn handle_logs(store: &StateStore, args: &LogsArgs) -> Result<(), AppError> {
    log_ops::handle_logs(store, args)
}

pub(super) fn handle_start_spec(
    store: &StateStore,
    mut spec: LaunchSpec,
    options: StartExecutionOptions,
) -> Result<(), AppError> {
    validate_start_options(options)?;
    let session_id = Uuid::new_v4().simple().to_string();
    let log_path = default_log_path(store, &session_id);
    let debug_meta = prepare_debug_spec(&mut spec)?;
    run_prelaunch_task_if_any(&spec, &log_path)?;
    let now = unix_timestamp_secs();

    if options.foreground {
        ensure_debug_runtime_ready(&spec)?;
        let command = build_command(&spec)?;
        let (pid, exit_code) = run_process_foreground(
            &command,
            Path::new(&spec.cwd),
            &spec.env,
            &log_path,
            options.log_mode,
        )?;

        let record = SessionRecord {
            id: session_id.clone(),
            spec,
            status: SessionStatus::Stopped,
            pid: None,
            supervisor_pid: None,
            log_path: Some(log_path.to_string_lossy().to_string()),
            debug_meta,
            created_at: now,
            updated_at: now,
            last_exit_code: exit_code,
            restart_count: 0,
        };
        let debug_meta_output = record.debug_meta.clone();
        store.update::<_, _, AppError>(|state| {
            state.sessions.insert(session_id.clone(), record);
            Ok(())
        })?;
        let _ = crate::session_lookup::upsert_session_path(&session_id, store.root_path());

        let mut output =
            format!("session_id={session_id} pid={pid} status=stopped mode=foreground");
        if let Some(code) = exit_code {
            output.push_str(&format!(" exit_code={code}"));
        }
        append_debug_meta_output(&mut output, debug_meta_output.as_ref());
        output::print_message(&output);
        return Ok(());
    }

    let pid = spawn_session_worker(store, &session_id, &spec, Some(log_path.clone()))?;
    let record = SessionRecord {
        id: session_id.clone(),
        spec,
        status: SessionStatus::Running,
        pid: Some(pid),
        supervisor_pid: None,
        log_path: Some(log_path.to_string_lossy().to_string()),
        debug_meta,
        created_at: now,
        updated_at: now,
        last_exit_code: None,
        restart_count: 0,
    };
    let debug_meta_output = record.debug_meta.clone();
    store.update::<_, _, AppError>(|state| {
        state.sessions.insert(session_id.clone(), record);
        Ok(())
    })?;
    let _ = crate::session_lookup::upsert_session_path(&session_id, store.root_path());

    let mut output = format!("session_id={session_id} pid={pid} status=running");
    append_debug_meta_output(&mut output, debug_meta_output.as_ref());
    output::print_message(&output);

    if options.tail {
        let follow_args = LogsArgs {
            id: Some(session_id),
            session_id: None,
            tail: 100,
            follow: true,
            poll_ms: 200,
            contains: Vec::new(),
            exclude: Vec::new(),
            regex: None,
            exclude_regex: None,
            ignore_case: false,
        };
        log_ops::handle_logs(store, &follow_args)?;
    }

    Ok(())
}

fn handle_daemon(store: &StateStore, args: &DaemonArgs) -> Result<(), AppError> {
    if args.once {
        let restarted = reconcile_all_sessions(store)?;
        output::print_message(&format!("reconciled=true restarted={restarted}"));
        return Ok(());
    }

    loop {
        let restarted = reconcile_all_sessions(store)?;
        output::print_message(&format!("reconciled=true restarted={restarted}"));
        thread::sleep(Duration::from_millis(args.interval_ms));
    }
}

fn reconcile_all_sessions(store: &StateStore) -> Result<usize, AppError> {
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let mut restarted = 0usize;

        let ids: Vec<String> = state.sessions.keys().cloned().collect();
        for id in ids {
            let session = state
                .sessions
                .get_mut(&id)
                .ok_or_else(|| AppError::SessionNotFound(id.clone()))?;

            let before = session.restart_count;
            reconcile_session(store, session, now)?;
            if session.restart_count > before {
                restarted += (session.restart_count - before) as usize;
            }
        }

        Ok(restarted)
    })
}

fn reconcile_session(
    store: &StateStore,
    session: &mut SessionRecord,
    now: u64,
) -> Result<bool, AppError> {
    let pid_alive = session.pid.map(is_process_alive).unwrap_or(false);

    if pid_alive {
        return Ok(false);
    }

    let should_restart = session.spec.managed
        && matches!(
            session.status,
            SessionStatus::Running | SessionStatus::Unknown
        );

    if should_restart {
        let log_path = session
            .log_path
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_log_path(store, &session.id));
        let debug_meta = prepare_debug_spec(&mut session.spec)?;
        run_prelaunch_task_if_any(&session.spec, &log_path)?;
        let pid = spawn_session_worker(store, &session.id, &session.spec, Some(log_path))?;
        session.pid = Some(pid);
        session.status = SessionStatus::Running;
        session.updated_at = now;
        session.restart_count += 1;
        session.debug_meta = debug_meta;
        return Ok(true);
    }

    let mut dirty = false;
    if session.pid.is_some() {
        session.pid = None;
        dirty = true;
    }

    if !matches!(session.status, SessionStatus::Stopped) {
        session.status = SessionStatus::Stopped;
        dirty = true;
    }

    if dirty {
        session.updated_at = now;
    }

    Ok(dirty)
}

fn spawn_session_worker(
    store: &StateStore,
    session_id: &str,
    spec: &LaunchSpec,
    existing_log_path: Option<PathBuf>,
) -> Result<u32, AppError> {
    ensure_debug_runtime_ready(spec)?;
    let command = build_command(spec)?;
    let log_path = existing_log_path.unwrap_or_else(|| default_log_path(store, session_id));

    spawn_process(&command, Path::new(&spec.cwd), &spec.env, &log_path).map_err(AppError::from)
}

fn ensure_debug_runtime_ready(spec: &LaunchSpec) -> Result<(), AppError> {
    if !matches!(spec.mode, LaunchMode::Debug) {
        return Ok(());
    }

    if !matches!(spec.runtime, RuntimeKind::Python) {
        return Ok(());
    }

    let interpreter = python_executable(spec);
    let status = ProcessCommand::new(interpreter)
        .arg("-c")
        .arg("import debugpy")
        .current_dir(&spec.cwd)
        .envs(spec.env.iter())
        .output()?
        .status;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::PythonDebugpyUnavailable)
    }
}

fn prepare_debug_spec(spec: &mut LaunchSpec) -> Result<Option<DebugSessionMeta>, AppError> {
    if !matches!(spec.mode, LaunchMode::Debug) {
        return Ok(None);
    }
    if !matches!(spec.runtime, RuntimeKind::Python | RuntimeKind::Node) {
        return Err(AppError::UnsupportedDebugRuntime(
            spec_ops::runtime_label(&spec.runtime).to_string(),
        ));
    }

    let debug = spec.debug.clone().unwrap_or_default();
    let resolved = resolve_debug_config(&debug)?;
    spec.debug = Some(resolved.config.clone());

    Ok(Some(DebugSessionMeta {
        host: resolved.config.host.clone(),
        requested_port: resolved.requested_port,
        active_port: resolved.config.port,
        fallback_applied: resolved.fallback_applied,
        reconnect_policy: "auto-retry".to_string(),
    }))
}

fn run_prelaunch_task_if_any(spec: &LaunchSpec, log_path: &Path) -> Result<(), AppError> {
    if let Some(task) = &spec.prelaunch_task {
        run_shell_task(task, Path::new(&spec.cwd), &spec.env, log_path)?;
    }
    Ok(())
}

fn default_log_path(store: &StateStore, session_id: &str) -> PathBuf {
    store
        .state_dir_path()
        .join("logs")
        .join(format!("{session_id}.log"))
}

fn append_debug_meta_output(output: &mut String, meta: Option<&DebugSessionMeta>) {
    if let Some(meta) = meta {
        output.push_str(&format!(
            " debug_host={} debug_port={} requested_debug_port={} debug_fallback={} debug_endpoint={}:{}",
            meta.host,
            meta.active_port,
            meta.requested_port,
            meta.fallback_applied,
            meta.host,
            meta.active_port
        ));
    }
}

fn find_session_mut<'a>(
    state: &'a mut AppState,
    session_id: &str,
) -> Result<&'a mut SessionRecord, AppError> {
    state
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))
}

fn build_debug_session_doc(session: &SessionRecord, meta: &DebugSessionMeta) -> serde_json::Value {
    let mut doc = json!({
        "ok": true,
        "session_id": session.id,
        "runtime": spec_ops::runtime_label(&session.spec.runtime),
        "debug_meta": meta,
        "endpoint": format!("{}:{}", meta.host, meta.active_port),
    });

    if matches!(session.spec.runtime, RuntimeKind::Python) {
        let attach = json!({
            "name": format!("Attach ({})", session.spec.name),
            "type": "python",
            "request": "attach",
            "connect": {
                "host": meta.host,
                "port": meta.active_port
            },
            "justMyCode": false,
            "pathMappings": [
                {
                    "localRoot": "${workspaceFolder}",
                    "remoteRoot": "."
                }
            ]
        });
        if let Some(obj) = doc.as_object_mut() {
            obj.insert("attach_vscode".to_string(), attach);
        }
    }
    if matches!(session.spec.runtime, RuntimeKind::Node) {
        let attach = json!({
            "name": format!("Attach ({})", session.spec.name),
            "type": "pwa-node",
            "request": "attach",
            "address": meta.host,
            "port": meta.active_port,
            "restart": true,
            "localRoot": "${workspaceFolder}",
            "remoteRoot": "."
        });
        if let Some(obj) = doc.as_object_mut() {
            obj.insert("attach_vscode".to_string(), attach);
        }
    }

    doc
}
