use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use launch_code::model::{RuntimeKind, SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    CleanupArgs, CleanupStatusArg, ListArgs, ListFormatArg, ListStatusArg, RestartArgs, ResumeArgs,
    RunningArgs, SessionIdArgs, StopArgs, SuspendArgs,
};
use crate::error::AppError;
use crate::link_registry::load_registry;
use crate::output;

mod batch;

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

#[derive(Debug, Clone, Copy)]
struct ListRenderOptions {
    view: ListRenderView,
    no_trunc: bool,
    short_id_len: usize,
    no_headers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListRenderView {
    Wide,
    Compact,
    Id,
}

#[derive(Debug, Clone)]
struct GlobalCleanupRow {
    link_name: String,
    link_path: String,
    matched_count: usize,
    removed_count: usize,
    kept_count: usize,
    matched_session_ids: Vec<String>,
    removed_session_ids: Vec<String>,
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

    fn from_running_args(args: &RunningArgs) -> Self {
        Self {
            status_filter: Some(ListStatusArg::Running),
            runtime_filter: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
            name_filter: args
                .name_contains
                .as_ref()
                .map(|value| value.to_lowercase()),
        }
    }
}

pub(super) fn handle_stop(store: &StateStore, args: &StopArgs) -> Result<(), AppError> {
    batch::handle_stop(store, args)
}

pub(super) fn handle_restart(store: &StateStore, args: &RestartArgs) -> Result<(), AppError> {
    batch::handle_restart(store, args)
}

pub(super) fn handle_suspend(store: &StateStore, args: &SuspendArgs) -> Result<(), AppError> {
    batch::handle_suspend(store, args)
}

pub(super) fn handle_resume(store: &StateStore, args: &ResumeArgs) -> Result<(), AppError> {
    batch::handle_resume(store, args)
}

pub(super) fn handle_status(store: &StateStore, args: &SessionIdArgs) -> Result<(), AppError> {
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let session = super::api_get_session(store, session_id)?;
    let output = format_status_like_message(&session);
    print_session_command_output("status", &session, output);
    Ok(())
}

pub(super) fn handle_list(store: &StateStore, args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let render = list_render_options_from_list_args(args);
    let include_topology = should_include_topology(render);
    let rows = collect_rows_from_store(store, &filters, None, None, include_topology)?;
    print_list_rows(&rows, render);
    Ok(())
}

pub(super) fn handle_running(store: &StateStore, args: &RunningArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_running_args(args);
    let render = list_render_options_from_running_args(args);
    let include_topology = should_include_topology(render);
    let rows = collect_rows_from_store(store, &filters, None, None, include_topology)?;
    print_list_rows(&rows, render);
    Ok(())
}

pub(super) fn handle_list_global_default(args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let render = list_render_options_from_list_args(args);
    handle_list_global_with_filters(&filters, render)
}

pub(super) fn handle_running_global_default(args: &RunningArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_running_args(args);
    let render = list_render_options_from_running_args(args);
    handle_list_global_with_filters(&filters, render)
}

fn handle_list_global_with_filters(
    filters: &ListFilters,
    render: ListRenderOptions,
) -> Result<(), AppError> {
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();
    let include_topology = should_include_topology(render);

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }
        let store = StateStore::new(&item.path);
        let mut scoped_rows = match collect_rows_from_store(
            &store,
            filters,
            Some(item.name),
            Some(item.path),
            include_topology,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };
        rows.append(&mut scoped_rows);
    }

    cache_global_rows_session_routes(&rows);

    rows.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.link_name.cmp(&right.link_name))
    });

    print_list_rows(&rows, render);
    Ok(())
}

fn cache_global_rows_session_routes(rows: &[SessionListRow]) {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let Some(link_path) = row.link_path.as_ref() else {
            continue;
        };
        grouped
            .entry(link_path.clone())
            .or_default()
            .push(row.id.clone());
    }

    for (path, session_ids) in grouped {
        let _ = crate::session_lookup::upsert_sessions_for_path(session_ids, Path::new(&path));
    }
}

fn list_render_options_from_list_args(args: &ListArgs) -> ListRenderOptions {
    let view = if args.quiet {
        ListRenderView::Id
    } else if let Some(format) = args.format {
        list_render_view_from_format(format)
    } else if args.compact {
        ListRenderView::Compact
    } else {
        ListRenderView::Wide
    };

    ListRenderOptions {
        view,
        no_trunc: args.no_trunc,
        short_id_len: args.short_id_len,
        no_headers: args.no_headers,
    }
}

fn list_render_options_from_running_args(args: &RunningArgs) -> ListRenderOptions {
    let view = if args.quiet {
        ListRenderView::Id
    } else if let Some(format) = args.format {
        list_render_view_from_format(format)
    } else if args.wide {
        ListRenderView::Wide
    } else {
        ListRenderView::Compact
    };

    ListRenderOptions {
        view,
        no_trunc: args.no_trunc,
        short_id_len: args.short_id_len,
        no_headers: args.no_headers,
    }
}

fn list_render_view_from_format(format: ListFormatArg) -> ListRenderView {
    match format {
        ListFormatArg::Table | ListFormatArg::Wide => ListRenderView::Wide,
        ListFormatArg::Compact => ListRenderView::Compact,
        ListFormatArg::Id => ListRenderView::Id,
    }
}

pub(super) fn handle_cleanup(store: &StateStore, args: &CleanupArgs) -> Result<(), AppError> {
    let statuses = resolve_cleanup_statuses(args);
    let result = super::api_cleanup_sessions_with_options(
        store,
        &statuses,
        args.older_than_secs,
        args.dry_run,
    )?;
    let matched_count = result.matched_session_ids.len();
    let removed_count = result.removed_session_ids.len();

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "dry_run": result.dry_run,
            "older_than_secs": args.older_than_secs,
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
    if let Some(older_than_secs) = args.older_than_secs {
        message.push_str(&format!(" older_than_secs={older_than_secs}"));
    }
    if !result.removed_session_ids.is_empty() {
        message.push_str(&format!(
            " removed_ids={}",
            result.removed_session_ids.join(",")
        ));
    }
    output::print_message(&message);
    Ok(())
}

pub(super) fn handle_cleanup_global_default(args: &CleanupArgs) -> Result<(), AppError> {
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let statuses = resolve_cleanup_statuses(args);
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();
    let mut matched_session_ids = Vec::new();
    let mut removed_session_ids = Vec::new();
    let mut kept_count = 0usize;

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }

        let store = StateStore::new(&item.path);
        let preloaded_state = match store.load() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if preloaded_state.sessions.is_empty() {
            continue;
        }

        let result = match super::api_cleanup_sessions_with_options(
            &store,
            &statuses,
            args.older_than_secs,
            args.dry_run,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };

        matched_session_ids.extend(result.matched_session_ids.iter().cloned());
        removed_session_ids.extend(result.removed_session_ids.iter().cloned());
        kept_count += result.kept_count;

        rows.push(GlobalCleanupRow {
            link_name: item.name,
            link_path: item.path,
            matched_count: result.matched_session_ids.len(),
            removed_count: result.removed_session_ids.len(),
            kept_count: result.kept_count,
            matched_session_ids: result.matched_session_ids,
            removed_session_ids: result.removed_session_ids,
        });
    }

    rows.sort_by(|left, right| {
        left.link_name
            .cmp(&right.link_name)
            .then_with(|| left.link_path.cmp(&right.link_path))
    });
    matched_session_ids.sort();
    matched_session_ids.dedup();
    removed_session_ids.sort();
    removed_session_ids.dedup();

    let matched_count = matched_session_ids.len();
    let removed_count = removed_session_ids.len();

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "link_name": row.link_name,
                    "link_path": row.link_path,
                    "matched_count": row.matched_count,
                    "removed_count": row.removed_count,
                    "kept_count": row.kept_count,
                    "matched_session_ids": row.matched_session_ids,
                    "removed_session_ids": row.removed_session_ids,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "scope": "global",
            "dry_run": args.dry_run,
            "older_than_secs": args.older_than_secs,
            "link_count": rows.len(),
            "matched_count": matched_count,
            "removed_count": removed_count,
            "kept_count": kept_count,
            "matched_session_ids": matched_session_ids,
            "removed_session_ids": removed_session_ids,
            "items": items,
        }));
        return Ok(());
    }

    let mut message = format!(
        "session_cleanup_scope=global session_cleanup_dry_run={} links={} matched={} removed={} kept={}",
        args.dry_run,
        rows.len(),
        matched_count,
        removed_count,
        kept_count
    );
    if let Some(older_than_secs) = args.older_than_secs {
        message.push_str(&format!(" older_than_secs={older_than_secs}"));
    }
    if !removed_session_ids.is_empty() {
        message.push_str(&format!(" removed_ids={}", removed_session_ids.join(",")));
    }
    output::print_message(&message);

    if !rows.is_empty() {
        let mut lines = Vec::with_capacity(rows.len() + 1);
        lines.push("LINK\tPATH\tMATCHED\tREMOVED\tKEPT".to_string());
        for row in &rows {
            lines.push(format!(
                "{}\t{}\t{}\t{}\t{}",
                row.link_name, row.link_path, row.matched_count, row.removed_count, row.kept_count
            ));
        }
        output::print_lines(&lines);
    }

    Ok(())
}

pub(super) fn handle_stop_global_default(args: &StopArgs) -> Result<(), AppError> {
    batch::handle_stop_global_default(args)
}

pub(super) fn handle_restart_global_default(args: &RestartArgs) -> Result<(), AppError> {
    batch::handle_restart_global_default(args)
}

pub(super) fn handle_suspend_global_default(args: &SuspendArgs) -> Result<(), AppError> {
    batch::handle_suspend_global_default(args)
}

pub(super) fn handle_resume_global_default(args: &ResumeArgs) -> Result<(), AppError> {
    batch::handle_resume_global_default(args)
}

fn status_label(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Stopped => "stopped",
        SessionStatus::Suspended => "suspended",
        SessionStatus::Unknown => "unknown",
    }
}

fn format_status_like_message(session: &SessionRecord) -> String {
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
    output
}

fn print_session_command_output(action: &str, session: &SessionRecord, message: String) {
    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "action": action,
            "message": message,
            "session": build_session_command_doc(session),
        }));
        return;
    }
    output::print_message(&message);
}

fn build_session_command_doc(session: &SessionRecord) -> serde_json::Value {
    let debug_endpoint = session
        .debug_meta
        .as_ref()
        .map(|meta| format!("{}:{}", meta.host, meta.active_port));
    json!({
        "id": session.id.clone(),
        "status": status_label(&session.status),
        "runtime": super::spec_ops::runtime_label(&session.spec.runtime),
        "mode": super::spec_ops::mode_label(&session.spec.mode),
        "pid": session.pid,
        "restart_count": session.restart_count,
        "name": session.spec.name.clone(),
        "entry": session.spec.entry.clone(),
        "debug_endpoint": debug_endpoint,
        "debug_meta": session.debug_meta.clone(),
    })
}

fn format_session_list_row_wide(row: &SessionListRow) -> String {
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

fn format_session_list_row_compact(
    row: &SessionListRow,
    no_trunc: bool,
    short_id_len: usize,
) -> String {
    let pid_display = row
        .pid
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let debug_display =
        truncate_compact_field(row.debug_endpoint.as_deref().unwrap_or("-"), 24, no_trunc);
    let link_display =
        truncate_compact_field(row.link_name.as_deref().unwrap_or("-"), 20, no_trunc);
    let id_display = abbreviate_session_id(&row.id, no_trunc, short_id_len);
    let name_display = truncate_compact_field(&row.name, 32, no_trunc);

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        id_display,
        row.status,
        row.runtime,
        row.mode,
        pid_display,
        name_display,
        debug_display,
        link_display,
    )
}

fn abbreviate_session_id(value: &str, no_trunc: bool, short_id_len: usize) -> String {
    if no_trunc {
        return value.to_string();
    }
    value.chars().take(short_id_len).collect()
}

fn truncate_compact_field(value: &str, max_chars: usize, no_trunc: bool) -> String {
    if no_trunc {
        return value.to_string();
    }

    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let prefix: String = value.chars().take(max_chars - 3).collect();
    format!("{prefix}...")
}

fn should_include_topology(render: ListRenderOptions) -> bool {
    output::is_json_mode() || !matches!(render.view, ListRenderView::Compact)
}

fn collect_rows_from_store(
    store: &StateStore,
    filters: &ListFilters,
    link_name: Option<String>,
    link_path: Option<String>,
    include_topology: bool,
) -> Result<Vec<SessionListRow>, AppError> {
    let sessions = super::api_list_sessions(store)?;
    if sessions.is_empty() {
        return Ok(Vec::new());
    }

    if !include_topology {
        let mut rows = Vec::new();
        for session in sessions {
            if !matches_list_filters(filters, &session) {
                continue;
            }

            let debug_endpoint = session
                .debug_meta
                .as_ref()
                .map(|meta| format!("{}:{}", meta.host, meta.active_port));

            rows.push(SessionListRow {
                id: session.id,
                status: status_label(&session.status),
                runtime: super::spec_ops::runtime_label(&session.spec.runtime),
                mode: super::spec_ops::mode_label(&session.spec.mode),
                pid: session.pid,
                restart_count: session.restart_count,
                name: session.spec.name,
                entry: session.spec.entry,
                debug_endpoint,
                parent_session_id: None,
                child_session_ids: Vec::new(),
                link_name: link_name.clone(),
                link_path: link_path.clone(),
            });
        }
        return Ok(rows);
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

fn print_list_rows(rows: &[SessionListRow], render: ListRenderOptions) {
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

    if matches!(render.view, ListRenderView::Id) {
        let lines: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        output::print_lines(&lines);
        return;
    }

    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            if matches!(render.view, ListRenderView::Compact) {
                format_session_list_row_compact(row, render.no_trunc, render.short_id_len)
            } else {
                format_session_list_row_wide(row)
            }
        })
        .collect();
    if lines.is_empty() {
        output::print_lines(&lines);
        return;
    }

    let header = if matches!(render.view, ListRenderView::Compact) {
        "ID\tSTATUS\tRUNTIME\tMODE\tPID\tNAME\tDEBUG\tLINK"
    } else {
        "ID\tSTATUS\tRUNTIME\tMODE\tPID\tRESTARTS\tNAME\tENTRY\tDEBUG\tCHILDREN\tPARENT\tLINK"
    };
    let mut all_lines = Vec::with_capacity(lines.len() + usize::from(!render.no_headers));
    if !render.no_headers {
        all_lines.push(header.to_string());
    }
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
