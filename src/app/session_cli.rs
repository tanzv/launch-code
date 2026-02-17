use launch_code::model::SessionStatus;
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{ListArgs, ListStatusArg, RestartArgs, SessionIdArgs, StopArgs};
use crate::error::AppError;
use crate::output;

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

pub(super) fn handle_stop(store: &StateStore, args: &StopArgs) -> Result<(), AppError> {
    let session =
        super::api_stop_session_with_options(store, &args.id, args.force, args.grace_timeout_ms)?;
    let output = format!("session_id={} status=stopped", session.id);
    output::print_message(&output);
    Ok(())
}

pub(super) fn handle_restart(store: &StateStore, args: &RestartArgs) -> Result<(), AppError> {
    let force = args.force && !args.no_force;
    let session =
        super::api_restart_session_with_options(store, &args.id, force, args.grace_timeout_ms)?;
    let pid = session
        .pid
        .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;
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
    output::print_message(&output);
    Ok(())
}

pub(super) fn handle_suspend(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = launch_code::model::unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;

        launch_code::process::suspend_process(pid)?;
        session.status = SessionStatus::Suspended;
        session.updated_at = now;
        let session_id = session.id.clone();
        Ok(format!("session_id={session_id} status=suspended"))
    })?;
    output::print_message(&output);
    Ok(())
}

pub(super) fn handle_resume(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = launch_code::model::unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;

        launch_code::process::resume_process(pid)?;
        session.status = SessionStatus::Running;
        session.updated_at = now;
        let session_id = session.id.clone();
        Ok(format!("session_id={session_id} status=running"))
    })?;
    output::print_message(&output);
    Ok(())
}

pub(super) fn handle_status(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let session_id = args.id.clone();
    let output = store.update::<_, _, AppError>(|state| {
        let now = launch_code::model::unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        super::reconcile_session(store, session, now)?;
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

pub(super) fn handle_list(store: &StateStore, args: &ListArgs) -> Result<(), AppError> {
    let runtime_filter = args.runtime.as_ref().map(super::spec_ops::to_runtime_kind);
    let name_filter = args
        .name_contains
        .as_ref()
        .map(|value| value.to_lowercase());
    let status_filter = args.status.clone();

    let rows = store.update::<_, _, AppError>(|state| {
        if state.sessions.is_empty() {
            return Ok(Vec::<SessionListRow>::new());
        }

        let now = launch_code::model::unix_timestamp_secs();
        let mut rows = Vec::new();
        let ids: Vec<String> = state.sessions.keys().cloned().collect();

        for id in ids {
            let session = state
                .sessions
                .get_mut(&id)
                .ok_or_else(|| AppError::SessionNotFound(id.clone()))?;
            super::reconcile_session(store, session, now)?;

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
                runtime: super::spec_ops::runtime_label(&session.spec.runtime),
                mode: super::spec_ops::mode_label(&session.spec.mode),
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
