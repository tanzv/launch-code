use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use launch_code::model::{RuntimeKind, SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{
    CleanupArgs, CleanupStatusArg, ListArgs, ListFormatArg, ListSortArg, ListStatusArg,
    RestartArgs, ResumeArgs, RunningArgs, SessionIdArgs, StopArgs, SuspendArgs,
};
use crate::error::AppError;
use crate::link_registry::load_registry;
use crate::output;

mod batch;
mod global_scan_index;
mod list_cache;
mod view;

#[derive(Debug, Clone)]
struct SessionListRow {
    id: String,
    status: &'static str,
    runtime: &'static str,
    mode: &'static str,
    updated_at: u64,
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
struct ListOrderOptions {
    sort: Option<ListSortArg>,
    limit: Option<usize>,
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

impl ListOrderOptions {
    fn from_list_args(args: &ListArgs) -> Self {
        Self {
            sort: args.sort,
            limit: args.limit,
        }
    }

    fn from_running_args(args: &RunningArgs) -> Self {
        Self {
            sort: args.sort,
            limit: args.limit,
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
    if let Some(interval_ms) = args.watch_interval_ms {
        let max_cycles = args.watch_count.unwrap_or(usize::MAX);
        let mut cycle = 0usize;
        loop {
            cycle = cycle.saturating_add(1);
            execute_list_once(store, args)?;
            if cycle >= max_cycles {
                break;
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }
        return Ok(());
    }

    execute_list_once(store, args)
}

fn execute_list_once(store: &StateStore, args: &ListArgs) -> Result<(), AppError> {
    let started_at = Instant::now();
    let filters = ListFilters::from_args(args);
    let order = ListOrderOptions::from_list_args(args);
    let render = list_render_options_from_list_args(args);
    let include_topology = view::should_include_topology(render);
    let collect_started_at = Instant::now();
    let mut rows = collect_rows_from_store(store, &filters, None, None, include_topology)?;
    apply_list_order(&mut rows, order);
    let collect_rows_ms = collect_started_at.elapsed().as_millis();
    let render_started_at = Instant::now();
    view::print_list_rows(&rows, render);
    let render_ms = render_started_at.elapsed().as_millis();
    output::print_trace(&format!(
        "trace_time command=list scope=local collect_rows_ms={collect_rows_ms} render_ms={render_ms} total_ms={} rows={}",
        started_at.elapsed().as_millis(),
        rows.len()
    ));
    Ok(())
}

pub(super) fn handle_running(store: &StateStore, args: &RunningArgs) -> Result<(), AppError> {
    if let Some(interval_ms) = args.watch_interval_ms {
        let max_cycles = args.watch_count.unwrap_or(usize::MAX);
        let mut cycle = 0usize;
        loop {
            cycle = cycle.saturating_add(1);
            execute_running_once(store, args)?;
            if cycle >= max_cycles {
                break;
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }
        return Ok(());
    }

    execute_running_once(store, args)
}

fn execute_running_once(store: &StateStore, args: &RunningArgs) -> Result<(), AppError> {
    let started_at = Instant::now();
    let filters = ListFilters::from_running_args(args);
    let order = ListOrderOptions::from_running_args(args);
    let render = list_render_options_from_running_args(args);
    let include_topology = view::should_include_topology(render);
    let collect_started_at = Instant::now();
    let mut rows = collect_rows_from_store(store, &filters, None, None, include_topology)?;
    apply_list_order(&mut rows, order);
    let collect_rows_ms = collect_started_at.elapsed().as_millis();
    let render_started_at = Instant::now();
    view::print_list_rows(&rows, render);
    let render_ms = render_started_at.elapsed().as_millis();
    output::print_trace(&format!(
        "trace_time command=running scope=local collect_rows_ms={collect_rows_ms} render_ms={render_ms} total_ms={} rows={}",
        started_at.elapsed().as_millis(),
        rows.len()
    ));
    Ok(())
}

pub(super) fn handle_list_global_default(args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let order = ListOrderOptions::from_list_args(args);
    let render = list_render_options_from_list_args(args);
    if let Some(interval_ms) = args.watch_interval_ms {
        let max_cycles = args.watch_count.unwrap_or(usize::MAX);
        let mut cycle = 0usize;
        loop {
            cycle = cycle.saturating_add(1);
            handle_list_global_with_filters("list", &filters, render, order)?;
            if cycle >= max_cycles {
                break;
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }
        return Ok(());
    }
    handle_list_global_with_filters("list", &filters, render, order)
}

pub(super) fn handle_running_global_default(args: &RunningArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_running_args(args);
    let order = ListOrderOptions::from_running_args(args);
    let render = list_render_options_from_running_args(args);
    if let Some(interval_ms) = args.watch_interval_ms {
        let max_cycles = args.watch_count.unwrap_or(usize::MAX);
        let mut cycle = 0usize;
        loop {
            cycle = cycle.saturating_add(1);
            handle_list_global_with_filters("running", &filters, render, order)?;
            if cycle >= max_cycles {
                break;
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }
        return Ok(());
    }
    handle_list_global_with_filters("running", &filters, render, order)
}

fn handle_list_global_with_filters(
    command_label: &str,
    filters: &ListFilters,
    render: ListRenderOptions,
    order: ListOrderOptions,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let load_links_started_at = Instant::now();
    let registry = load_registry()?;
    let load_links_ms = load_links_started_at.elapsed().as_millis();
    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();
    let include_topology = view::should_include_topology(render);
    let mut scan_index = global_scan_index::GlobalListScanIndex::load_best_effort();
    let mut skipped_links = 0usize;
    let mut load_sessions_ms = 0u128;
    let mut build_rows_ms = 0u128;

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }

        let store = StateStore::new(&item.path);
        let state_signature = global_scan_index::read_state_signature(&store.state_file_path());
        if state_signature.as_ref().is_some_and(|signature| {
            scan_index.should_skip_for_filters(&item.path, signature, filters)
        }) {
            skipped_links = skipped_links.saturating_add(1);
            continue;
        }

        let load_sessions_started_at = Instant::now();
        let sessions = match list_cache::load_sessions_for_listing(&store) {
            Ok(value) => value,
            Err(_) => continue,
        };
        load_sessions_ms =
            load_sessions_ms.saturating_add(load_sessions_started_at.elapsed().as_millis());

        if let Some(signature) = state_signature {
            scan_index.update_link_summary(&item.path, signature, &sessions);
        }

        let build_rows_started_at = Instant::now();
        let mut scoped_rows = view::collect_rows_from_sessions(
            sessions,
            filters,
            Some(item.name),
            Some(item.path),
            include_topology,
        );
        build_rows_ms = build_rows_ms.saturating_add(build_rows_started_at.elapsed().as_millis());
        rows.append(&mut scoped_rows);
    }
    scan_index.persist_best_effort();

    cache_global_rows_session_routes(&rows);

    apply_list_order(&mut rows, order);

    let render_started_at = Instant::now();
    view::print_list_rows(&rows, render);
    let render_ms = render_started_at.elapsed().as_millis();
    output::print_trace(&format!(
        "trace_time command={command_label} scope=global links={} skipped_links={skipped_links} load_links_ms={load_links_ms} load_sessions_ms={load_sessions_ms} build_rows_ms={build_rows_ms} render_ms={render_ms} total_ms={} rows={}",
        seen_paths.len(),
        started_at.elapsed().as_millis(),
        rows.len()
    ));
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

fn apply_list_order(rows: &mut Vec<SessionListRow>, order: ListOrderOptions) {
    let sort = order.sort.unwrap_or(ListSortArg::Id);
    match sort {
        ListSortArg::Id => rows.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
        ListSortArg::Name => rows.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
        ListSortArg::Runtime => rows.sort_by(|left, right| {
            left.runtime
                .cmp(right.runtime)
                .then_with(|| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
        ListSortArg::Status => rows.sort_by(|left, right| {
            status_sort_rank(left.status)
                .cmp(&status_sort_rank(right.status))
                .then_with(|| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
        ListSortArg::Updated => rows.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
        ListSortArg::Restarts => rows.sort_by(|left, right| {
            right
                .restart_count
                .cmp(&left.restart_count)
                .then_with(|| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.link_name.cmp(&right.link_name))
        }),
    }

    if let Some(limit) = order.limit {
        rows.truncate(limit);
    }
}

fn status_sort_rank(status: &str) -> u8 {
    match status {
        "running" => 0,
        "suspended" => 1,
        "stopped" => 2,
        "unknown" => 3,
        _ => u8::MAX,
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

fn matches_list_status(filter: &ListStatusArg, status: &SessionStatus) -> bool {
    view::matches_list_status(filter, status)
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
            " debug_adapter={} debug_transport={} debug_host={} debug_port={} requested_debug_port={} debug_fallback={} reconnect_policy={} debug_endpoint={}:{}",
            meta.adapter_kind,
            meta.transport,
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

fn collect_rows_from_store(
    store: &StateStore,
    filters: &ListFilters,
    link_name: Option<String>,
    link_path: Option<String>,
    include_topology: bool,
) -> Result<Vec<SessionListRow>, AppError> {
    let sessions = list_cache::load_sessions_for_listing(store)?;
    Ok(view::collect_rows_from_sessions(
        sessions,
        filters,
        link_name,
        link_path,
        include_topology,
    ))
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
