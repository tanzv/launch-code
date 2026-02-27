use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, ClearType};
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

#[derive(Debug, Clone)]
struct GlobalCleanupLinkErrorRow {
    link_name: String,
    link_path: String,
    error: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct GlobalListCollectStats {
    links: usize,
    skipped_links: usize,
    load_links_ms: u128,
    load_sessions_ms: u128,
    build_rows_ms: u128,
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
    if should_enable_interactive_list(
        args,
        output::is_stdout_terminal(),
        output::is_ps_alias_mode(),
    ) {
        return handle_list_interactive_local(store, args);
    }

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
    if should_enable_interactive_list(
        args,
        output::is_stdout_terminal(),
        output::is_ps_alias_mode(),
    ) {
        return handle_list_interactive_global(args);
    }

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
    let include_topology = view::should_include_topology(render);
    let (mut rows, collect_stats) = collect_global_rows(filters, include_topology)?;

    apply_list_order(&mut rows, order);

    let render_started_at = Instant::now();
    view::print_list_rows(&rows, render);
    let render_ms = render_started_at.elapsed().as_millis();
    output::print_trace(&format!(
        "trace_time command={command_label} scope=global links={} skipped_links={} load_links_ms={} load_sessions_ms={} build_rows_ms={} render_ms={} total_ms={} rows={}",
        collect_stats.links,
        collect_stats.skipped_links,
        collect_stats.load_links_ms,
        collect_stats.load_sessions_ms,
        collect_stats.build_rows_ms,
        render_ms,
        started_at.elapsed().as_millis(),
        rows.len()
    ));
    Ok(())
}

fn collect_global_rows(
    filters: &ListFilters,
    include_topology: bool,
) -> Result<(Vec<SessionListRow>, GlobalListCollectStats), AppError> {
    let _ = super::link_ops::auto_prune_stale_links_for_global_scan();
    let load_links_started_at = Instant::now();
    let registry = load_registry()?;
    let mut stats = GlobalListCollectStats {
        load_links_ms: load_links_started_at.elapsed().as_millis(),
        ..GlobalListCollectStats::default()
    };

    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();
    let mut scan_index = global_scan_index::GlobalListScanIndex::load_best_effort();

    for item in registry.list() {
        if !seen_paths.insert(item.path.clone()) {
            continue;
        }

        let store = StateStore::new(&item.path);
        let state_signature = global_scan_index::read_state_signature(&store.state_file_path());
        if state_signature.as_ref().is_some_and(|signature| {
            scan_index.should_skip_for_filters(&item.path, signature, filters)
        }) {
            stats.skipped_links = stats.skipped_links.saturating_add(1);
            continue;
        }

        let load_sessions_started_at = Instant::now();
        let sessions = match list_cache::load_sessions_for_listing(&store) {
            Ok(value) => value,
            Err(_) => continue,
        };
        stats.load_sessions_ms = stats
            .load_sessions_ms
            .saturating_add(load_sessions_started_at.elapsed().as_millis());

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
        stats.build_rows_ms = stats
            .build_rows_ms
            .saturating_add(build_rows_started_at.elapsed().as_millis());
        rows.append(&mut scoped_rows);
    }
    scan_index.persist_best_effort();

    stats.links = seen_paths.len();
    cache_global_rows_session_routes(&rows);
    Ok((rows, stats))
}

fn should_enable_interactive_list(
    args: &ListArgs,
    stdout_is_terminal: bool,
    ps_alias_mode: bool,
) -> bool {
    if output::is_json_mode() || args.watch_interval_ms.is_some() || args.quiet || args.no_interactive
    {
        return false;
    }
    if !stdout_is_terminal {
        return false;
    }
    args.interactive || ps_alias_mode
}

fn handle_list_interactive_local(store: &StateStore, args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let order = ListOrderOptions::from_list_args(args);
    let render = list_render_options_from_list_args(args);

    run_interactive_browser("scope=local", render, || {
        let mut rows = collect_rows_from_store(store, &filters, None, None, true)?;
        apply_list_order(&mut rows, order);
        Ok(rows)
    })
}

fn handle_list_interactive_global(args: &ListArgs) -> Result<(), AppError> {
    let filters = ListFilters::from_args(args);
    let order = ListOrderOptions::from_list_args(args);
    let render = list_render_options_from_list_args(args);

    run_interactive_browser("scope=global", render, || {
        let (mut rows, _stats) = collect_global_rows(&filters, true)?;
        apply_list_order(&mut rows, order);
        Ok(rows)
    })
}

fn run_interactive_browser<F>(
    scope_label: &str,
    render: ListRenderOptions,
    mut collect_rows: F,
) -> Result<(), AppError>
where
    F: FnMut() -> Result<Vec<SessionListRow>, AppError>,
{
    let _guard = InteractiveTerminalGuard::enter()?;
    let mut rows = collect_rows()?;
    let mut selected = 0usize;
    let mut show_details = false;
    let mut status_line = String::from("interactive mode");
    let refresh_interval = Duration::from_millis(1200);
    let mut next_refresh_at = Instant::now() + refresh_interval;

    loop {
        if selected >= rows.len() && !rows.is_empty() {
            selected = rows.len().saturating_sub(1);
        }
        render_interactive_frame(
            scope_label,
            &rows,
            selected,
            show_details,
            &status_line,
            render,
        )?;

        let now = Instant::now();
        let timeout = next_refresh_at.saturating_duration_since(now);
        if event::poll(timeout)? && let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('j') | KeyCode::Down => {
                    if !rows.is_empty() {
                        selected = (selected + 1).min(rows.len().saturating_sub(1));
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::Char('g') => {
                    selected = 0;
                }
                KeyCode::Char('G') => {
                    if !rows.is_empty() {
                        selected = rows.len().saturating_sub(1);
                    }
                }
                KeyCode::Enter => {
                    show_details = !show_details;
                }
                KeyCode::Char('r') => {
                    rows = collect_rows()?;
                    status_line = String::from("refreshed");
                    next_refresh_at = Instant::now() + refresh_interval;
                }
                _ => {}
            }
        }

        if Instant::now() >= next_refresh_at {
            rows = collect_rows()?;
            status_line = String::from("auto refreshed");
            next_refresh_at = Instant::now() + refresh_interval;
        }
    }
    Ok(())
}

fn render_interactive_frame(
    scope_label: &str,
    rows: &[SessionListRow],
    selected: usize,
    show_details: bool,
    status_line: &str,
    render: ListRenderOptions,
) -> Result<(), AppError> {
    let mut stdout = io::stdout();
    execute!(stdout, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All))?;

    writeln!(
        stdout,
        "lcode ps --interactive ({scope_label})  keys: ↑/k ↓/j select  Enter details  r refresh  q quit"
    )?;
    writeln!(stdout, "status: {status_line}")?;
    writeln!(stdout)?;

    if rows.is_empty() {
        writeln!(stdout, "no sessions")?;
        writeln!(
            stdout,
            "hint: use `lcode running` for active sessions, `lcode list --format id` for raw ids, and `lcode link list` to check global links."
        )?;
        stdout.flush()?;
        return Ok(());
    }

    let (_width, height) = terminal::size()?;
    let reserved_detail_lines = if show_details { 11usize } else { 0usize };
    let reserved_header_lines = 5usize;
    let list_capacity = usize::from(height)
        .saturating_sub(reserved_header_lines + reserved_detail_lines)
        .max(1);
    let window = list_capacity.min(rows.len());
    let start = selected
        .saturating_sub(window / 2)
        .min(rows.len().saturating_sub(window));
    let end = start.saturating_add(window);

    writeln!(stdout, " ID\tSTATUS\tRUNTIME\tMODE\tPID\tNAME\tDEBUG\tLINK")?;
    for (idx, row) in rows.iter().enumerate().take(end).skip(start) {
        let marker = if idx == selected { ">" } else { " " };
        writeln!(
            stdout,
            "{marker} {}",
            format_interactive_row(row, render.no_trunc, render.short_id_len)
        )?;
    }

    if show_details {
        let row = &rows[selected];
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
        let children_display = if row.child_session_ids.is_empty() {
            "-".to_string()
        } else {
            row.child_session_ids.join(",")
        };

        writeln!(stdout)?;
        writeln!(stdout, "-- selected session ------------------------------------------------")?;
        writeln!(stdout, "id: {}", row.id)?;
        writeln!(stdout, "status: {}  runtime: {}  mode: {}", row.status, row.runtime, row.mode)?;
        writeln!(
            stdout,
            "pid: {pid_display}  restarts: {}  debug: {debug_display}",
            row.restart_count
        )?;
        writeln!(stdout, "name: {}", row.name)?;
        writeln!(stdout, "entry: {}", row.entry)?;
        writeln!(stdout, "link: {link_display}  parent: {parent_display}")?;
        writeln!(stdout, "children: {children_display}")?;
    }

    stdout.flush()?;
    Ok(())
}

fn format_interactive_row(row: &SessionListRow, no_trunc: bool, short_id_len: usize) -> String {
    let id = if no_trunc {
        row.id.clone()
    } else {
        row.id.chars().take(short_id_len).collect()
    };
    let pid = row
        .pid
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let name = truncate_interactive_field(&row.name, 36, no_trunc);
    let debug = truncate_interactive_field(row.debug_endpoint.as_deref().unwrap_or("-"), 24, no_trunc);
    let link = truncate_interactive_field(row.link_name.as_deref().unwrap_or("-"), 20, no_trunc);

    format!(
        "{id}\t{}\t{}\t{}\t{pid}\t{name}\t{debug}\t{link}",
        row.status, row.runtime, row.mode
    )
}

fn truncate_interactive_field(value: &str, max_chars: usize, no_trunc: bool) -> String {
    if no_trunc || value.chars().count() <= max_chars {
        return value.to_string();
    }
    let prefix: String = value.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{prefix}...")
}

struct InteractiveTerminalGuard;

impl InteractiveTerminalGuard {
    fn enter() -> Result<Self, AppError> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for InteractiveTerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
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

fn list_render_options_from_list_args(args: &ListArgs) -> ListRenderOptions {
    let view = resolve_list_render_view(args, output::is_stdout_terminal());

    ListRenderOptions {
        view,
        no_trunc: args.no_trunc,
        short_id_len: args.short_id_len,
        no_headers: args.no_headers,
    }
}

fn resolve_list_render_view(args: &ListArgs, stdout_is_terminal: bool) -> ListRenderView {
    if args.quiet {
        ListRenderView::Id
    } else if let Some(format) = args.format {
        list_render_view_from_format(format)
    } else if args.compact || stdout_is_terminal {
        ListRenderView::Compact
    } else {
        ListRenderView::Wide
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
    let mut link_errors = Vec::new();
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
            Err(err) => {
                link_errors.push(GlobalCleanupLinkErrorRow {
                    link_name: item.name.clone(),
                    link_path: item.path.clone(),
                    error: err.to_string(),
                });
                continue;
            }
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
            Err(err) => {
                link_errors.push(GlobalCleanupLinkErrorRow {
                    link_name: item.name.clone(),
                    link_path: item.path.clone(),
                    error: err.to_string(),
                });
                continue;
            }
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
    link_errors.sort_by(|left, right| {
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
            "link_error_count": link_errors.len(),
            "matched_count": matched_count,
            "removed_count": removed_count,
            "kept_count": kept_count,
            "matched_session_ids": matched_session_ids,
            "removed_session_ids": removed_session_ids,
            "link_errors": link_errors
                .iter()
                .map(|row| {
                    json!({
                        "link_name": row.link_name,
                        "link_path": row.link_path,
                        "error": row.error,
                    })
                })
                .collect::<Vec<_>>(),
            "items": items,
        }));
        return Ok(());
    }

    let mut message = format!(
        "session_cleanup_scope=global session_cleanup_dry_run={} links={} matched={} removed={} kept={} link_errors={}",
        args.dry_run,
        rows.len(),
        matched_count,
        removed_count,
        kept_count,
        link_errors.len()
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

    if !link_errors.is_empty() {
        let mut lines = Vec::with_capacity(link_errors.len() + 1);
        lines.push("LINK\tPATH\tERROR".to_string());
        for row in &link_errors {
            lines.push(format!(
                "{}\t{}\t{}",
                row.link_name, row.link_path, row.error
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

#[cfg(test)]
mod tests {
    use super::*;

    fn build_list_args() -> ListArgs {
        ListArgs {
            status: None,
            runtime: None,
            name_contains: None,
            format: None,
            compact: false,
            quiet: false,
            no_trunc: false,
            short_id_len: 12,
            no_headers: false,
            interactive: false,
            no_interactive: false,
            watch_interval_ms: None,
            watch_count: None,
            sort: None,
            limit: None,
        }
    }

    #[test]
    fn list_default_view_prefers_compact_on_tty() {
        let args = build_list_args();
        assert_eq!(
            resolve_list_render_view(&args, true),
            ListRenderView::Compact
        );
    }

    #[test]
    fn list_default_view_keeps_wide_for_non_tty() {
        let args = build_list_args();
        assert_eq!(resolve_list_render_view(&args, false), ListRenderView::Wide);
    }

    #[test]
    fn list_default_view_respects_quiet_compact_and_explicit_format() {
        let mut quiet_args = build_list_args();
        quiet_args.quiet = true;
        assert_eq!(
            resolve_list_render_view(&quiet_args, true),
            ListRenderView::Id
        );

        let mut compact_args = build_list_args();
        compact_args.compact = true;
        assert_eq!(
            resolve_list_render_view(&compact_args, false),
            ListRenderView::Compact
        );

        let mut formatted_args = build_list_args();
        formatted_args.format = Some(ListFormatArg::Wide);
        assert_eq!(
            resolve_list_render_view(&formatted_args, true),
            ListRenderView::Wide
        );
    }

    #[test]
    fn interactive_mode_defaults_to_ps_alias_on_tty() {
        let args = build_list_args();
        assert!(should_enable_interactive_list(&args, true, true));
        assert!(!should_enable_interactive_list(&args, false, true));
        assert!(!should_enable_interactive_list(&args, true, false));
    }

    #[test]
    fn interactive_mode_respects_force_on_and_off_flags() {
        let mut interactive_args = build_list_args();
        interactive_args.interactive = true;
        assert!(should_enable_interactive_list(&interactive_args, true, false));

        let mut disabled_args = build_list_args();
        disabled_args.no_interactive = true;
        assert!(!should_enable_interactive_list(&disabled_args, true, true));
    }
}
