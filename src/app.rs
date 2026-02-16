mod config_ops;
mod dap_cli;
mod log_ops;
mod session_api;
mod spec_ops;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use launch_code::config::{LaunchRequest, load_launch_spec};
use launch_code::debug::resolve_debug_config;
use launch_code::model::{
    AppState, DebugConfig, DebugSessionMeta, LaunchMode, LaunchSpec, RuntimeKind, SessionRecord,
    SessionStatus, unix_timestamp_secs,
};
use launch_code::process::{
    is_process_alive, resume_process, run_shell_task, spawn_process, stop_process_with_options,
    suspend_process,
};
use launch_code::runtime::build_command;
use launch_code::runtime::python_executable;
use launch_code::state::StateStore;
use serde_json::json;
use uuid::Uuid;

use crate::cli::{
    Commands, DaemonArgs, DebugArgs, InspectArgs, LaunchArgs, ListArgs, ListStatusArg, LogsArgs,
    RestartArgs, ServeArgs, SessionIdArgs, StopArgs,
};
use crate::dap::DapRegistry;
use crate::error::AppError;
use crate::output;

pub(crate) use session_api::{
    api_debug_session, api_get_session, api_inspect_session, api_list_sessions,
    api_restart_session_with_options, api_resume_session, api_stop_session_with_options,
    api_suspend_session,
};

pub(crate) fn execute(store: &StateStore, command: Commands) -> Result<(), AppError> {
    match command {
        Commands::Start(args) => {
            let spec = spec_ops::build_launch_spec(&args, LaunchMode::Run, None)?;
            handle_start_spec(store, spec)
        }
        Commands::Debug(args) => handle_debug(store, &args),
        Commands::Launch(args) => handle_launch(store, &args),
        Commands::Attach(args) => handle_attach(store, &args),
        Commands::Inspect(args) => handle_inspect(store, &args),
        Commands::Logs(args) => handle_logs(store, &args),
        Commands::Stop(args) => handle_stop(store, &args),
        Commands::Restart(args) => handle_restart(store, &args),
        Commands::Suspend(args) => handle_suspend(store, &args),
        Commands::Resume(args) => handle_resume(store, &args),
        Commands::Status(args) => handle_status(store, &args),
        Commands::List(args) => handle_list(store, &args),
        Commands::Config(args) => config_ops::handle_config(store, &args),
        Commands::Daemon(args) => handle_daemon(store, &args),
        Commands::Serve(args) => handle_serve(store, &args),
        Commands::Dap(args) => dap_cli::handle_dap(store, &args),
    }
}

fn handle_serve(store: &StateStore, args: &ServeArgs) -> Result<(), AppError> {
    let token = resolve_serve_token(args)?;
    let max_body_bytes = args.max_body_bytes.clamp(1024, 16_777_216);
    crate::http_utils::set_http_max_json_body_bytes(max_body_bytes);
    let server =
        tiny_http::Server::http(&args.bind).map_err(|err| AppError::Http(err.to_string()))?;
    let url = format!("http://{}", server.server_addr());
    output::print_message(&format!("listening={url}"));
    std::io::stdout().flush()?;

    let worker_count = args.workers.clamp(1, 256);
    let queue_capacity = args.queue_capacity.min(4096);
    let serve_state = Arc::new(Mutex::new(DapRegistry::default()));
    let (sender, receiver) = mpsc::sync_channel::<tiny_http::Request>(queue_capacity);
    let receiver = Arc::new(Mutex::new(receiver));

    let mut workers = Vec::new();
    for _ in 0..worker_count {
        let store = store.clone();
        let token = token.clone();
        let serve_state = Arc::clone(&serve_state);
        let receiver = Arc::clone(&receiver);
        workers.push(thread::spawn(move || {
            loop {
                let request = {
                    let guard = match receiver.lock() {
                        Ok(value) => value,
                        Err(_) => break,
                    };
                    match guard.recv() {
                        Ok(request) => request,
                        Err(_) => break,
                    }
                };

                let mut request = request;
                let response = crate::http_api::response_for_request(
                    &store,
                    &token,
                    &serve_state,
                    &mut request,
                );
                let _ = request.respond(response);
            }
        }));
    }

    for request in server.incoming_requests() {
        match sender.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(request)) => {
                let method = request.method().as_str().to_string();
                let url = request.url().to_string();
                let response = crate::http_utils::http_json(
                    tiny_http::StatusCode(503),
                    json!({
                        "ok": false,
                        "error": "server_overloaded",
                        "message": "request queue is full",
                    }),
                );
                let response = match tiny_http::Header::from_bytes("Retry-After", "1") {
                    Ok(header) => response.with_header(header),
                    Err(_) => response,
                };
                let _ = request.respond(response);
                crate::http_api::observe_response(&method, &url, 503, Duration::from_millis(0));
            }
            Err(TrySendError::Disconnected(_)) => {
                break;
            }
        }
    }

    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }

    Ok(())
}

fn resolve_serve_token(args: &ServeArgs) -> Result<String, AppError> {
    if let Some(token) = args.token.as_ref().map(|value| value.trim().to_string()) {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    if let Some(path) = &args.token_file {
        let raw = fs::read_to_string(path)?;
        let token = raw
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or_else(|| {
                AppError::Http(format!("token file is empty: {}", path.to_string_lossy()))
            })?;
        return Ok(token.to_string());
    }

    if let Ok(token) = std::env::var("LAUNCH_CODE_HTTP_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    Err(AppError::Http(
        "missing serve token: provide --token, --token-file, or LAUNCH_CODE_HTTP_TOKEN".to_string(),
    ))
}

fn handle_debug(store: &StateStore, args: &DebugArgs) -> Result<(), AppError> {
    let debug = DebugConfig {
        host: args.host.clone(),
        port: args.port,
        wait_for_client: args.wait_for_client,
        subprocess: args.subprocess,
    };
    let spec = spec_ops::build_launch_spec(&args.start, LaunchMode::Debug, Some(debug))?;
    handle_start_spec(store, spec)
}

fn handle_launch(store: &StateStore, args: &LaunchArgs) -> Result<(), AppError> {
    let request = LaunchRequest {
        name: args.name.clone(),
        mode: spec_ops::to_launch_mode(&args.mode),
        managed_override: args.managed.then_some(true),
        launch_file: args.launch_file.clone(),
    };
    let spec = load_launch_spec(store.root_path(), &request)?;
    handle_start_spec(store, spec)
}

fn handle_attach(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(&args.id)
        .ok_or_else(|| AppError::SessionNotFound(args.id.clone()))?;
    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;

    let doc = build_debug_session_doc(session, meta);
    output::print_json_doc(&doc);
    Ok(())
}

fn handle_inspect(store: &StateStore, args: &InspectArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let tail_lines = args.tail;
    let doc = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.clone()))?;

        reconcile_session(store, session, now)?;
        let pid = session.pid;
        let alive = pid.map(is_process_alive).unwrap_or(false);
        let command = build_command(&session.spec)?;
        let log_tail =
            log_ops::read_log_tail(session.log_path.as_deref(), tail_lines).unwrap_or_default();

        Ok(json!({
            "session": session.clone(),
            "process": {
                "pid": pid,
                "alive": alive,
                "command": command,
            },
            "log": {
                "tail_lines": tail_lines,
                "text": log_tail,
            }
        }))
    })?;

    output::print_json_doc(&doc);
    Ok(())
}

fn handle_logs(store: &StateStore, args: &LogsArgs) -> Result<(), AppError> {
    log_ops::handle_logs(store, args)
}

pub(super) fn handle_start_spec(store: &StateStore, mut spec: LaunchSpec) -> Result<(), AppError> {
    let session_id = Uuid::new_v4().simple().to_string();
    let log_path = default_log_path(store, &session_id);
    let debug_meta = prepare_debug_spec(&mut spec)?;
    run_prelaunch_task_if_any(&spec, &log_path)?;
    let pid = spawn_session_worker(store, &session_id, &spec, Some(log_path.clone()))?;
    let now = unix_timestamp_secs();

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

    let mut output = format!("session_id={session_id} pid={pid} status=running");
    if let Some(meta) = &debug_meta_output {
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

    output::print_message(&output);
    Ok(())
}

fn handle_stop(store: &StateStore, args: &StopArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let grace_timeout = Duration::from_millis(args.grace_timeout_ms);
    let (output, post_task) = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = find_session_mut(state, &session_id)?;
        if let Some(pid) = session.pid {
            stop_process_with_options(pid, args.force, grace_timeout)?;
        }

        let post_task = if let (Some(task), Some(log_path)) =
            (session.spec.poststop_task.clone(), session.log_path.clone())
        {
            Some((
                task,
                session.spec.cwd.clone(),
                session.spec.env.clone(),
                log_path,
            ))
        } else {
            None
        };

        session.pid = None;
        session.status = SessionStatus::Stopped;
        session.updated_at = now;
        Ok((
            format!("session_id={} status=stopped", session.id),
            post_task,
        ))
    })?;

    if let Some((task, cwd, env_map, log_path)) = post_task {
        run_shell_task(&task, Path::new(&cwd), &env_map, Path::new(&log_path))?;
    }

    output::print_message(&output);
    Ok(())
}

fn handle_restart(store: &StateStore, args: &RestartArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let force = args.force && !args.no_force;
    let grace_timeout = Duration::from_millis(args.grace_timeout_ms);
    let output = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = find_session_mut(state, &session_id)?;
        if let Some(pid) = session.pid {
            if is_process_alive(pid) {
                stop_process_with_options(pid, force, grace_timeout)?;
            }
        }

        let log_path = session
            .log_path
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_log_path(store, &session.id));
        let debug_meta = prepare_debug_spec(&mut session.spec)?;
        run_prelaunch_task_if_any(&session.spec, &log_path)?;
        let pid = spawn_session_worker(store, &session.id, &session.spec, Some(log_path.clone()))?;
        session.pid = Some(pid);
        session.status = SessionStatus::Running;
        session.restart_count = session.restart_count.saturating_add(1);
        session.updated_at = now;
        session.debug_meta = debug_meta;
        let mut output = format!(
            "session_id={} pid={} status=running restart_count={}",
            session.id, pid, session.restart_count
        );
        if let Some(meta) = &session.debug_meta {
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
        Ok(output)
    })?;
    output::print_message(&output);
    Ok(())
}

fn handle_suspend(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;

        suspend_process(pid)?;
        session.status = SessionStatus::Suspended;
        session.updated_at = now;
        let session_id = session.id.clone();
        Ok(format!("session_id={session_id} status=suspended"))
    })?;
    output::print_message(&output);
    Ok(())
}

fn handle_resume(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;

        resume_process(pid)?;
        session.status = SessionStatus::Running;
        session.updated_at = now;
        let session_id = session.id.clone();
        Ok(format!("session_id={session_id} status=running"))
    })?;
    output::print_message(&output);
    Ok(())
}

fn handle_status(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = find_session_mut(state, &session_id)?;
        reconcile_session(store, session, now)?;
        let pid_display = session
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());
        let status = status_label(&session.status);
        let mut output = format!(
            "session_id={} status={} pid={} restart_count={}",
            session.id, status, pid_display, session.restart_count
        );
        if let Some(meta) = &session.debug_meta {
            output.push_str(&format!(
                " debug_host={} debug_port={} requested_debug_port={} debug_fallback={} reconnect_policy={} debug_endpoint={}:{}",
                meta.host,
                meta.active_port,
                meta.requested_port,
                meta.fallback_applied,
                meta.reconnect_policy,
                meta.host,
                meta.active_port
            ));
        }

        Ok(output)
    })?;

    output::print_message(&output);
    Ok(())
}

fn handle_list(store: &StateStore, args: &ListArgs) -> Result<(), AppError> {
    let runtime_filter = args.runtime.as_ref().map(spec_ops::to_runtime_kind);
    let name_filter = args
        .name_contains
        .as_ref()
        .map(|value| value.to_lowercase());
    let status_filter = args.status.clone();

    let rows = store.update::<_, _, AppError>(|state| {
        if state.sessions.is_empty() {
            return Ok(Vec::<SessionListRow>::new());
        }

        let now = unix_timestamp_secs();
        let mut rows = Vec::new();
        let ids: Vec<String> = state.sessions.keys().cloned().collect();

        for id in ids {
            let session = state
                .sessions
                .get_mut(&id)
                .ok_or_else(|| AppError::SessionNotFound(id.clone()))?;
            reconcile_session(store, session, now)?;

            if let Some(status) = &status_filter {
                if !matches_list_status(status, &session.status) {
                    continue;
                }
            }
            if let Some(runtime) = &runtime_filter {
                if &session.spec.runtime != runtime {
                    continue;
                }
            }
            if let Some(filter) = &name_filter {
                if !session.spec.name.to_lowercase().contains(filter) {
                    continue;
                }
            }

            let debug_endpoint = session
                .debug_meta
                .as_ref()
                .map(|meta| format!("{}:{}", meta.host, meta.active_port));

            rows.push(SessionListRow {
                id: id.clone(),
                status: status_label(&session.status),
                runtime: spec_ops::runtime_label(&session.spec.runtime),
                mode: spec_ops::mode_label(&session.spec.mode),
                pid: session.pid,
                restart_count: session.restart_count,
                name: session.spec.name.clone(),
                entry: session.spec.entry.clone(),
                debug_endpoint,
            });
        }

        Ok(rows)
    })?;

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "status": row.status,
                    "runtime": row.runtime,
                    "mode": row.mode,
                    "pid": row.pid,
                    "restart_count": row.restart_count,
                    "name": row.name,
                    "entry": row.entry,
                    "debug_endpoint": row.debug_endpoint,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "items": items,
        }));
        return Ok(());
    }

    let lines: Vec<String> = rows.iter().map(format_session_list_row).collect();
    output::print_lines(&lines);
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

fn find_session_mut<'a>(
    state: &'a mut AppState,
    session_id: &str,
) -> Result<&'a mut SessionRecord, AppError> {
    state
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))
}

fn status_label(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Stopped => "stopped",
        SessionStatus::Suspended => "suspended",
        SessionStatus::Unknown => "unknown",
    }
}

fn format_session_list_row(row: &SessionListRow) -> String {
    let pid_display = row
        .pid
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    let mut line = format!(
        "{}\t{}\t{}\t{}\tpid={}\trestarts={}\tname={}\tentry={}",
        row.id,
        row.status,
        row.runtime,
        row.mode,
        pid_display,
        row.restart_count,
        row.name,
        row.entry,
    );
    if let Some(endpoint) = &row.debug_endpoint {
        line.push_str(&format!(" debug_endpoint={endpoint}"));
    }
    line
}

fn matches_list_status(filter: &ListStatusArg, status: &SessionStatus) -> bool {
    matches!(
        (filter, status),
        (ListStatusArg::Running, SessionStatus::Running)
            | (ListStatusArg::Stopped, SessionStatus::Stopped)
            | (ListStatusArg::Suspended, SessionStatus::Suspended)
            | (ListStatusArg::Unknown, SessionStatus::Unknown)
    )
}

#[derive(Debug, Clone)]
struct SessionListRow {
    id: String,
    status: &'static str,
    runtime: &'static str,
    mode: &'static str,
    pid: Option<u32>,
    restart_count: u32,
    name: String,
    entry: String,
    debug_endpoint: Option<String>,
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

    doc
}
