use std::collections::{BTreeMap, BTreeSet};

use launch_code::model::{RuntimeKind, SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    CleanupArgs, CleanupStatusArg, ListArgs, ListStatusArg, RestartArgs, SessionIdArgs, StopArgs,
};
use crate::error::AppError;
use crate::link_registry::load_registry;
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
    parent_session_id: Option<String>,
    child_session_ids: Vec<String>,
    link_name: Option<String>,
    link_path: Option<String>,
}

#[derive(Debug, Clone)]
struct ListFilters {
    status_filter: Option<ListStatusArg>,
    runtime_filter: Option<RuntimeKind>,
    name_filter: Option<String>,
}

impl ListFilters {
    fn from_args(args: &ListArgs) -> Self {
        Self {
            status_filter: args.status.clone(),
            runtime_filter: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
            name_filter: args
                .name_contains
                .as_ref()
                .map(|value| value.to_lowercase()),
        }
    }
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
    let filters = ListFilters::from_args(args);
    let rows = collect_rows_from_store(store, &filters, None, None)?;
    print_list_rows(&rows);
    Ok(())
}

pub(super) fn handle_list_global_default(args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }
        let store = StateStore::new(&item.path);
        let mut scoped_rows =
            collect_rows_from_store(&store, &filters, Some(item.name), Some(item.path))?;
        rows.append(&mut scoped_rows);
    }

    rows.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.link_name.cmp(&right.link_name))
    });

    print_list_rows(&rows);
    Ok(())
}

pub(super) fn handle_cleanup(store: &StateStore, args: &CleanupArgs) -> Result<(), AppError> {
    let statuses = resolve_cleanup_statuses(args);
    let result = super::api_cleanup_sessions(store, &statuses, args.dry_run)?;
    let matched_count = result.matched_session_ids.len();
    let removed_count = result.removed_session_ids.len();

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "dry_run": result.dry_run,
            "matched_count": matched_count,
            "removed_count": removed_count,
            "kept_count": result.kept_count,
            "matched_session_ids": result.matched_session_ids,
            "removed_session_ids": result.removed_session_ids,
        }));
        return Ok(());
    }

    let mut message = format!(
        "session_cleanup_dry_run={} matched={} removed={} kept={}",
        result.dry_run, matched_count, removed_count, result.kept_count
    );
    if !result.removed_session_ids.is_empty() {
        message.push_str(&format!(
            " removed_ids={}",
            result.removed_session_ids.join(",")
        ));
    }
    output::print_message(&message);
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
        .unwrap_or_else(|| "-".to_string());
    let debug_display = row
        .debug_endpoint
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let parent_display = row
        .parent_session_id
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let link_display = row.link_name.clone().unwrap_or_else(|| "-".to_string());

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        row.id,
        row.status,
        row.runtime,
        row.mode,
        pid_display,
        row.restart_count,
        row.name,
        row.entry,
        debug_display,
        row.child_session_ids.len(),
        parent_display,
        link_display,
    )
}

fn collect_rows_from_store(
    store: &StateStore,
    filters: &ListFilters,
    link_name: Option<String>,
    link_path: Option<String>,
) -> Result<Vec<SessionListRow>, AppError> {
    let sessions = super::api_list_sessions(store)?;
    if sessions.is_empty() {
        return Ok(Vec::new());
    }

    let session_map: BTreeMap<String, SessionRecord> = sessions
        .into_iter()
        .map(|session| (session.id.clone(), session))
        .collect();
    let mut rows = Vec::new();

    for (id, session) in &session_map {
        if !matches_list_filters(filters, session) {
            continue;
        }

        let debug_endpoint = session
            .debug_meta
            .as_ref()
            .map(|meta| format!("{}:{}", meta.host, meta.active_port));
        let parent_session_id = super::session_api::infer_parent_session_id(session, &session_map);
        let child_session_ids =
            super::session_api::collect_child_session_ids(session, &session_map);

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
            parent_session_id,
            child_session_ids,
            link_name: link_name.clone(),
            link_path: link_path.clone(),
        });
    }

    Ok(rows)
}

fn matches_list_filters(filters: &ListFilters, session: &SessionRecord) -> bool {
    if let Some(status) = &filters.status_filter {
        if !matches_list_status(status, &session.status) {
            return false;
        }
    }
    if let Some(runtime) = &filters.runtime_filter {
        if &session.spec.runtime != runtime {
            return false;
        }
    }
    if let Some(name_filter) = &filters.name_filter {
        if !session.spec.name.to_lowercase().contains(name_filter) {
            return false;
        }
    }
    true
}

fn print_list_rows(rows: &[SessionListRow]) {
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
                    "parent_session_id": row.parent_session_id,
                    "child_session_ids": row.child_session_ids,
                    "child_session_count": row.child_session_ids.len(),
                    "link_name": row.link_name,
                    "link_path": row.link_path,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "items": items,
        }));
        return;
    }

    let lines: Vec<String> = rows.iter().map(format_session_list_row).collect();
    if lines.is_empty() {
        output::print_lines(&lines);
        return;
    }

    let mut all_lines = Vec::with_capacity(lines.len() + 1);
    all_lines.push(
        "ID\tSTATUS\tRUNTIME\tMODE\tPID\tRESTARTS\tNAME\tENTRY\tDEBUG\tCHILDREN\tPARENT\tLINK"
            .to_string(),
    );
    all_lines.extend(lines);
    output::print_lines(&all_lines);
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

fn resolve_cleanup_statuses(args: &CleanupArgs) -> Vec<SessionStatus> {
    if args.status.is_empty() {
        return vec![SessionStatus::Stopped, SessionStatus::Unknown];
    }

    args.status
        .iter()
        .map(|value| match value {
            CleanupStatusArg::Stopped => SessionStatus::Stopped,
            CleanupStatusArg::Unknown => SessionStatus::Unknown,
        })
        .collect()
}
