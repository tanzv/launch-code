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

#[derive(Debug, Clone, Copy)]
enum BatchAction {
    Stop,
    Restart,
    Suspend,
    Resume,
}

#[derive(Debug, Clone)]
struct BatchSelector {
    status: Option<ListStatusArg>,
    runtime: Option<RuntimeKind>,
    name_filter: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Clone, Copy)]
struct BatchExecutionControl {
    continue_on_error: bool,
    max_failures: usize,
}

#[derive(Debug, Clone)]
struct BatchSessionRow {
    id: String,
    status_before: &'static str,
    status_after: Option<&'static str>,
    link_name: Option<String>,
    link_path: Option<String>,
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct BatchLinkErrorRow {
    link_name: Option<String>,
    link_path: Option<String>,
    error: String,
}

#[derive(Debug, Clone)]
struct BatchExecutionResult {
    rows: Vec<BatchSessionRow>,
    selected_count: usize,
    processed_count: usize,
    link_errors: Vec<BatchLinkErrorRow>,
    stopped_early: bool,
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

impl BatchSelector {
    fn filters(&self) -> ListFilters {
        ListFilters {
            status_filter: self.status.clone(),
            runtime_filter: self.runtime.clone(),
            name_filter: self.name_filter.clone(),
        }
    }
}

impl BatchExecutionControl {
    fn should_stop_after_failure(&self, failure_count: usize) -> bool {
        if !self.continue_on_error {
            return true;
        }
        self.max_failures > 0 && failure_count >= self.max_failures
    }
}

pub(super) fn handle_stop(store: &StateStore, args: &StopArgs) -> Result<(), AppError> {
    if args.all {
        let selector = selector_from_stop_args(args);
        let control = control_from_stop_args(args);
        return handle_batch_control_local(
            store,
            BatchAction::Stop,
            selector,
            control,
            args.force,
            args.grace_timeout_ms,
        );
    }
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let session =
        super::api_stop_session_with_options(store, session_id, args.force, args.grace_timeout_ms)?;
    let output = format!("session_id={} status=stopped", session.id);
    print_session_command_output("stop", &session, output);
    Ok(())
}

pub(super) fn handle_restart(store: &StateStore, args: &RestartArgs) -> Result<(), AppError> {
    if args.all {
        let selector = selector_from_restart_args(args);
        let control = control_from_restart_args(args);
        let force = args.force && !args.no_force;
        return handle_batch_control_local(
            store,
            BatchAction::Restart,
            selector,
            control,
            force,
            args.grace_timeout_ms,
        );
    }
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let force = args.force && !args.no_force;
    let session =
        super::api_restart_session_with_options(store, session_id, force, args.grace_timeout_ms)?;
    let output = format_status_like_message(&session);
    print_session_command_output("restart", &session, output);
    Ok(())
}

pub(super) fn handle_suspend(store: &StateStore, args: &SuspendArgs) -> Result<(), AppError> {
    if args.all {
        let selector = selector_from_suspend_args(args);
        let control = control_from_suspend_args(args);
        return handle_batch_control_local(
            store,
            BatchAction::Suspend,
            selector,
            control,
            false,
            0,
        );
    }
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let session = super::api_suspend_session(store, session_id)?;
    let output = format!("session_id={} status=suspended", session.id);
    print_session_command_output("suspend", &session, output);
    Ok(())
}

pub(super) fn handle_resume(store: &StateStore, args: &ResumeArgs) -> Result<(), AppError> {
    if args.all {
        let selector = selector_from_resume_args(args);
        let control = control_from_resume_args(args);
        return handle_batch_control_local(store, BatchAction::Resume, selector, control, false, 0);
    }
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let session = super::api_resume_session(store, session_id)?;
    let output = format!("session_id={} status=running", session.id);
    print_session_command_output("resume", &session, output);
    Ok(())
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
    let rows = collect_rows_from_store(store, &filters, None, None)?;
    print_list_rows(&rows, render);
    Ok(())
}

pub(super) fn handle_running(store: &StateStore, args: &RunningArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_running_args(args);
    let render = list_render_options_from_running_args(args);
    let rows = collect_rows_from_store(store, &filters, None, None)?;
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

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }
        let store = StateStore::new(&item.path);
        let mut scoped_rows =
            match collect_rows_from_store(&store, filters, Some(item.name), Some(item.path)) {
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

        let result = match super::api_cleanup_sessions(&store, &statuses, args.dry_run) {
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
    handle_batch_control_global_stop(args)
}

pub(super) fn handle_restart_global_default(args: &RestartArgs) -> Result<(), AppError> {
    handle_batch_control_global_restart(args)
}

pub(super) fn handle_suspend_global_default(args: &SuspendArgs) -> Result<(), AppError> {
    handle_batch_control_global_suspend(args)
}

pub(super) fn handle_resume_global_default(args: &ResumeArgs) -> Result<(), AppError> {
    handle_batch_control_global_resume(args)
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

fn selector_from_stop_args(args: &StopArgs) -> BatchSelector {
    BatchSelector {
        status: args.status.clone(),
        runtime: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
        name_filter: args
            .name_contains
            .as_ref()
            .map(|value| value.to_lowercase()),
        dry_run: args.dry_run,
    }
}

fn selector_from_restart_args(args: &RestartArgs) -> BatchSelector {
    BatchSelector {
        status: args.status.clone(),
        runtime: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
        name_filter: args
            .name_contains
            .as_ref()
            .map(|value| value.to_lowercase()),
        dry_run: args.dry_run,
    }
}

fn selector_from_suspend_args(args: &SuspendArgs) -> BatchSelector {
    BatchSelector {
        status: args.status.clone(),
        runtime: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
        name_filter: args
            .name_contains
            .as_ref()
            .map(|value| value.to_lowercase()),
        dry_run: args.dry_run,
    }
}

fn selector_from_resume_args(args: &ResumeArgs) -> BatchSelector {
    BatchSelector {
        status: args.status.clone(),
        runtime: args.runtime.as_ref().map(super::spec_ops::to_runtime_kind),
        name_filter: args
            .name_contains
            .as_ref()
            .map(|value| value.to_lowercase()),
        dry_run: args.dry_run,
    }
}

fn control_from_stop_args(args: &StopArgs) -> BatchExecutionControl {
    BatchExecutionControl {
        continue_on_error: args.continue_on_error,
        max_failures: args.max_failures,
    }
}

fn control_from_restart_args(args: &RestartArgs) -> BatchExecutionControl {
    BatchExecutionControl {
        continue_on_error: args.continue_on_error,
        max_failures: args.max_failures,
    }
}

fn control_from_suspend_args(args: &SuspendArgs) -> BatchExecutionControl {
    BatchExecutionControl {
        continue_on_error: args.continue_on_error,
        max_failures: args.max_failures,
    }
}

fn control_from_resume_args(args: &ResumeArgs) -> BatchExecutionControl {
    BatchExecutionControl {
        continue_on_error: args.continue_on_error,
        max_failures: args.max_failures,
    }
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

fn handle_batch_control_local(
    store: &StateStore,
    action: BatchAction,
    selector: BatchSelector,
    control: BatchExecutionControl,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<(), AppError> {
    let filters = selector.filters();
    let mut failure_count = 0usize;
    let result = execute_batch_control_for_store(
        store,
        action,
        &filters,
        selector.dry_run,
        control,
        &mut failure_count,
        force,
        grace_timeout_ms,
        None,
        None,
    )?;
    print_batch_control_result(action, "local", selector.dry_run, control, result);
    Ok(())
}

fn handle_batch_control_global_stop(args: &StopArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("stop", args.dry_run, args.yes)?;
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_stop_args(args);
    let control = control_from_stop_args(args);
    let filters = selector.filters();
    let result = execute_batch_control_global(
        BatchAction::Stop,
        &filters,
        selector.dry_run,
        control,
        args.force,
        args.grace_timeout_ms,
    )?;
    print_batch_control_result(
        BatchAction::Stop,
        "global",
        selector.dry_run,
        control,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_restart(args: &RestartArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("restart", args.dry_run, args.yes)?;
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_restart_args(args);
    let control = control_from_restart_args(args);
    let filters = selector.filters();
    let force = args.force && !args.no_force;
    let result = execute_batch_control_global(
        BatchAction::Restart,
        &filters,
        selector.dry_run,
        control,
        force,
        args.grace_timeout_ms,
    )?;
    print_batch_control_result(
        BatchAction::Restart,
        "global",
        selector.dry_run,
        control,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_suspend(args: &SuspendArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("suspend", args.dry_run, args.yes)?;
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_suspend_args(args);
    let control = control_from_suspend_args(args);
    let filters = selector.filters();
    let result = execute_batch_control_global(
        BatchAction::Suspend,
        &filters,
        selector.dry_run,
        control,
        false,
        0,
    )?;
    print_batch_control_result(
        BatchAction::Suspend,
        "global",
        selector.dry_run,
        control,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_resume(args: &ResumeArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("resume", args.dry_run, args.yes)?;
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_resume_args(args);
    let control = control_from_resume_args(args);
    let filters = selector.filters();
    let result = execute_batch_control_global(
        BatchAction::Resume,
        &filters,
        selector.dry_run,
        control,
        false,
        0,
    )?;
    print_batch_control_result(
        BatchAction::Resume,
        "global",
        selector.dry_run,
        control,
        result,
    );
    Ok(())
}

fn ensure_global_batch_apply_confirmation(
    action_label: &str,
    dry_run: bool,
    confirmed: bool,
) -> Result<(), AppError> {
    if dry_run || confirmed {
        return Ok(());
    }
    Err(AppError::ConfirmationRequired(format!(
        "global `{action_label} --all` requires `--yes`; use `--dry-run` to preview."
    )))
}

fn execute_batch_control_global(
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<BatchExecutionResult, AppError> {
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut failure_count = 0usize;
    let mut rows = Vec::new();
    let mut link_errors = Vec::new();
    let mut selected_count = 0usize;
    let mut stopped_early = false;

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }
        let store = StateStore::new(&item.path);
        let link_name = item.name.clone();
        let link_path = item.path.clone();

        match execute_batch_control_for_store(
            &store,
            action,
            filters,
            dry_run,
            control,
            &mut failure_count,
            force,
            grace_timeout_ms,
            Some(link_name.clone()),
            Some(link_path.clone()),
        ) {
            Ok(mut scoped) => {
                selected_count = selected_count.saturating_add(scoped.selected_count);
                rows.append(&mut scoped.rows);
                if scoped.stopped_early {
                    stopped_early = true;
                    break;
                }
            }
            Err(err) => {
                failure_count = failure_count.saturating_add(1);
                link_errors.push(BatchLinkErrorRow {
                    link_name: Some(link_name),
                    link_path: Some(link_path),
                    error: err.to_string(),
                });
                if control.should_stop_after_failure(failure_count) {
                    stopped_early = true;
                    break;
                }
            }
        }
    }

    rows.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.link_name.cmp(&right.link_name))
    });
    link_errors.sort_by(|left, right| {
        left.link_name
            .cmp(&right.link_name)
            .then_with(|| left.link_path.cmp(&right.link_path))
    });

    let processed_count = rows.len();
    Ok(BatchExecutionResult {
        rows,
        selected_count,
        processed_count,
        link_errors,
        stopped_early,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_batch_control_for_store(
    store: &StateStore,
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    failure_count: &mut usize,
    force: bool,
    grace_timeout_ms: u64,
    link_name: Option<String>,
    link_path: Option<String>,
) -> Result<BatchExecutionResult, AppError> {
    let sessions = super::api_list_sessions(store)?;
    let mut matched_sessions: Vec<SessionRecord> = sessions
        .into_iter()
        .filter(|session| matches_batch_control_filters(action, filters, session))
        .collect();
    matched_sessions.sort_by(|left, right| left.id.cmp(&right.id));
    let total_matched = matched_sessions.len();

    let mut rows = Vec::with_capacity(matched_sessions.len());
    for session in matched_sessions {
        let status_before = status_label(&session.status);

        if dry_run {
            rows.push(BatchSessionRow {
                id: session.id,
                status_before,
                status_after: Some(status_before),
                link_name: link_name.clone(),
                link_path: link_path.clone(),
                ok: true,
                error: None,
            });
            continue;
        }

        let result = match action {
            BatchAction::Stop => {
                super::api_stop_session_with_options(store, &session.id, force, grace_timeout_ms)
            }
            BatchAction::Restart => {
                super::api_restart_session_with_options(store, &session.id, force, grace_timeout_ms)
            }
            BatchAction::Suspend => super::api_suspend_session(store, &session.id),
            BatchAction::Resume => super::api_resume_session(store, &session.id),
        };

        match result {
            Ok(updated) => rows.push(BatchSessionRow {
                id: updated.id,
                status_before,
                status_after: Some(status_label(&updated.status)),
                link_name: link_name.clone(),
                link_path: link_path.clone(),
                ok: true,
                error: None,
            }),
            Err(err) => {
                rows.push(BatchSessionRow {
                    id: session.id,
                    status_before,
                    status_after: None,
                    link_name: link_name.clone(),
                    link_path: link_path.clone(),
                    ok: false,
                    error: Some(err.to_string()),
                });
                *failure_count = failure_count.saturating_add(1);
                if control.should_stop_after_failure(*failure_count) {
                    break;
                }
            }
        }
    }

    let stopped_early = rows.len() < total_matched;
    let processed_count = rows.len();
    Ok(BatchExecutionResult {
        rows,
        selected_count: total_matched,
        processed_count,
        link_errors: Vec::new(),
        stopped_early,
    })
}

fn matches_batch_control_filters(
    action: BatchAction,
    filters: &ListFilters,
    session: &SessionRecord,
) -> bool {
    let status_match = if let Some(status_filter) = &filters.status_filter {
        matches_list_status(status_filter, &session.status)
    } else {
        default_batch_status_match(action, &session.status)
    };
    if !status_match {
        return false;
    }
    if let Some(runtime_filter) = &filters.runtime_filter {
        if session.spec.runtime != *runtime_filter {
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

fn default_batch_status_match(action: BatchAction, status: &SessionStatus) -> bool {
    match action {
        BatchAction::Stop => !matches!(status, SessionStatus::Stopped),
        BatchAction::Restart => matches!(status, SessionStatus::Running | SessionStatus::Suspended),
        BatchAction::Suspend => matches!(status, SessionStatus::Running),
        BatchAction::Resume => matches!(status, SessionStatus::Suspended),
    }
}

fn print_batch_control_result(
    action: BatchAction,
    scope: &str,
    dry_run: bool,
    control: BatchExecutionControl,
    result: BatchExecutionResult,
) {
    let BatchExecutionResult {
        rows,
        selected_count,
        processed_count,
        link_errors,
        stopped_early,
    } = result;
    let success_count = rows.iter().filter(|row| row.ok).count();
    let session_failed_count = rows.len().saturating_sub(success_count);
    let link_error_count = link_errors.len();
    let failed_count = session_failed_count.saturating_add(link_error_count);
    let action_label = match action {
        BatchAction::Stop => "stop",
        BatchAction::Restart => "restart",
        BatchAction::Suspend => "suspend",
        BatchAction::Resume => "resume",
    };

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "status_before": row.status_before,
                    "status_after": row.status_after,
                    "link_name": row.link_name,
                    "link_path": row.link_path,
                    "ok": row.ok,
                    "error": row.error,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "action": action_label,
            "scope": scope,
            "all": true,
            "dry_run": dry_run,
            "continue_on_error": control.continue_on_error,
            "max_failures": control.max_failures,
            "stopped_early": stopped_early,
            "matched_count": selected_count,
            "processed_count": processed_count,
            "success_count": success_count,
            "session_failed_count": session_failed_count,
            "link_error_count": link_error_count,
            "failed_count": failed_count,
            "link_errors": link_errors.iter().map(|row| {
                json!({
                    "link_name": row.link_name,
                    "link_path": row.link_path,
                    "error": row.error,
                })
            }).collect::<Vec<_>>(),
            "items": items,
        }));
        return;
    }

    output::print_message(&format!(
        "session_batch_action={action_label} scope={scope} dry_run={dry_run} continue_on_error={} max_failures={} stopped_early={} matched={} processed={} success={success_count} failed={failed_count} link_errors={link_error_count}",
        control.continue_on_error,
        control.max_failures,
        stopped_early,
        selected_count,
        processed_count,
    ));

    if rows.is_empty() && link_errors.is_empty() {
        return;
    }

    if !rows.is_empty() {
        let mut lines = Vec::with_capacity(rows.len() + 1);
        lines.push("ID\tSTATUS_BEFORE\tSTATUS_AFTER\tRESULT\tLINK\tERROR".to_string());
        for row in &rows {
            let status_after = row.status_after.unwrap_or("-");
            let result = if row.ok { "ok" } else { "failed" };
            let link = row.link_name.clone().unwrap_or_else(|| "-".to_string());
            let error = row.error.clone().unwrap_or_else(|| "-".to_string());
            lines.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                row.id, row.status_before, status_after, result, link, error
            ));
        }
        output::print_lines(&lines);
    }

    if !link_errors.is_empty() {
        let mut lines = Vec::with_capacity(link_errors.len() + 1);
        lines.push("LINK\tPATH\tERROR".to_string());
        for row in &link_errors {
            let link = row.link_name.clone().unwrap_or_else(|| "-".to_string());
            let path = row.link_path.clone().unwrap_or_else(|| "-".to_string());
            lines.push(format!("{}\t{}\t{}", link, path, row.error));
        }
        output::print_lines(&lines);
    }
}
