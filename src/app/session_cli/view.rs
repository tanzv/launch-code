use std::collections::BTreeMap;

use launch_code::model::{SessionRecord, SessionStatus};
use serde_json::json;

use crate::cli::ListStatusArg;
use crate::output;

use super::{ListFilters, ListRenderOptions, ListRenderView, SessionListRow};

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

pub(super) fn should_include_topology(render: ListRenderOptions) -> bool {
    output::is_json_mode() || !matches!(render.view, ListRenderView::Compact)
}

pub(super) fn collect_rows_from_sessions(
    sessions: Vec<SessionRecord>,
    filters: &ListFilters,
    link_name: Option<String>,
    link_path: Option<String>,
    include_topology: bool,
) -> Vec<SessionListRow> {
    if sessions.is_empty() {
        return Vec::new();
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
                status: super::status_label(&session.status),
                runtime: super::super::spec_ops::runtime_label(&session.spec.runtime),
                mode: super::super::spec_ops::mode_label(&session.spec.mode),
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
        return rows;
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
        let parent_session_id =
            super::super::session_api::infer_parent_session_id(session, &session_map);
        let child_session_ids =
            super::super::session_api::collect_child_session_ids(session, &session_map);

        rows.push(SessionListRow {
            id: id.clone(),
            status: super::status_label(&session.status),
            runtime: super::super::spec_ops::runtime_label(&session.spec.runtime),
            mode: super::super::spec_ops::mode_label(&session.spec.mode),
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

    rows
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

pub(super) fn print_list_rows(rows: &[SessionListRow], render: ListRenderOptions) {
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

pub(super) fn matches_list_status(filter: &ListStatusArg, status: &SessionStatus) -> bool {
    matches!(
        (filter, status),
        (ListStatusArg::Running, SessionStatus::Running)
            | (ListStatusArg::Stopped, SessionStatus::Stopped)
            | (ListStatusArg::Suspended, SessionStatus::Suspended)
            | (ListStatusArg::Unknown, SessionStatus::Unknown)
    )
}
