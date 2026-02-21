use std::collections::BTreeMap;

use serde_json::json;

use crate::cli::BatchSortArg;
use crate::output;

use super::{
    BatchAction, BatchExecutionControl, BatchExecutionPlan, BatchExecutionResult, BatchSessionRow,
};

pub(super) fn print_batch_control_result(
    action: BatchAction,
    scope: &str,
    dry_run: bool,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
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
    let action_label = batch_action_label(action);
    let summary = build_batch_summary_doc(&rows);

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "runtime": row.runtime,
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
            "sort": batch_sort_label(plan.sort),
            "limit": plan.limit,
            "jobs": plan.jobs,
            "summary": plan.summary,
            "stopped_early": stopped_early,
            "matched_count": selected_count,
            "processed_count": processed_count,
            "success_count": success_count,
            "session_failed_count": session_failed_count,
            "link_error_count": link_error_count,
            "failed_count": failed_count,
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
            "summary_doc": summary,
            "items": items,
        }));
        return;
    }

    output::print_message(&format!(
        "session_batch_action={action_label} scope={scope} dry_run={dry_run} continue_on_error={} max_failures={} sort={} limit={} jobs={} stopped_early={} matched={} processed={} success={success_count} failed={failed_count} link_errors={link_error_count}",
        control.continue_on_error,
        control.max_failures,
        batch_sort_label(plan.sort),
        plan.limit
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        plan.jobs,
        stopped_early,
        selected_count,
        processed_count,
    ));

    if rows.is_empty() && link_errors.is_empty() {
        return;
    }

    if !rows.is_empty() {
        let mut lines = Vec::with_capacity(rows.len() + 1);
        lines.push("ID\tRUNTIME\tSTATUS_BEFORE\tSTATUS_AFTER\tRESULT\tLINK\tERROR".to_string());
        for row in &rows {
            let status_after = row.status_after.unwrap_or("-");
            let result = if row.ok { "ok" } else { "failed" };
            let link = row.link_name.clone().unwrap_or_else(|| "-".to_string());
            let error = row.error.clone().unwrap_or_else(|| "-".to_string());
            lines.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.id, row.runtime, row.status_before, status_after, result, link, error
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

    if plan.summary {
        let mut lines = Vec::new();
        lines.push("SUMMARY_KEY\tCOUNT".to_string());
        for (key, count) in build_batch_summary_lines(&rows) {
            lines.push(format!("{key}\t{count}"));
        }
        output::print_lines(&lines);
    }
}

pub(super) fn print_multi_target_control_result(action: BatchAction, rows: Vec<BatchSessionRow>) {
    let action_label = batch_action_label(action);
    let success_count = rows.iter().filter(|row| row.ok).count();
    let failed_count = rows.len().saturating_sub(success_count);

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "runtime": row.runtime,
                    "status_after": row.status_after,
                    "ok": row.ok,
                    "error": row.error,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "action": action_label,
            "all": false,
            "target_count": rows.len(),
            "processed_count": rows.len(),
            "success_count": success_count,
            "failed_count": failed_count,
            "items": items,
        }));
        return;
    }

    output::print_message(&format!(
        "session_multi_action={action_label} targets={} success={success_count} failed={failed_count}",
        rows.len()
    ));

    if rows.is_empty() {
        return;
    }

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push("ID\tRUNTIME\tSTATUS_AFTER\tRESULT\tERROR".to_string());
    for row in rows {
        let status_after = row.status_after.unwrap_or("-");
        let result = if row.ok { "ok" } else { "failed" };
        let error = row.error.unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            row.id, row.runtime, status_after, result, error
        ));
    }
    output::print_lines(&lines);
}

fn batch_sort_label(sort: BatchSortArg) -> &'static str {
    match sort {
        BatchSortArg::Id => "id",
        BatchSortArg::Name => "name",
        BatchSortArg::Status => "status",
        BatchSortArg::Runtime => "runtime",
    }
}

fn build_batch_summary_lines(rows: &[BatchSessionRow]) -> Vec<(String, usize)> {
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_runtime: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_link: BTreeMap<String, usize> = BTreeMap::new();

    for row in rows {
        *by_status.entry(row.status_before.to_string()).or_default() += 1;
        *by_runtime.entry(row.runtime.to_string()).or_default() += 1;
        *by_link
            .entry(row.link_name.clone().unwrap_or_else(|| "local".to_string()))
            .or_default() += 1;
    }

    let mut lines = Vec::new();
    for (status, count) in by_status {
        lines.push((format!("status:{status}"), count));
    }
    for (runtime, count) in by_runtime {
        lines.push((format!("runtime:{runtime}"), count));
    }
    for (link, count) in by_link {
        lines.push((format!("link:{link}"), count));
    }
    lines
}

fn build_batch_summary_doc(rows: &[BatchSessionRow]) -> serde_json::Value {
    let lines = build_batch_summary_lines(rows);
    let items: Vec<serde_json::Value> = lines
        .into_iter()
        .map(|(key, count)| json!({ "key": key, "count": count }))
        .collect();
    json!({ "items": items })
}

fn batch_action_label(action: BatchAction) -> &'static str {
    match action {
        BatchAction::Stop => "stop",
        BatchAction::Restart => "restart",
        BatchAction::Suspend => "suspend",
        BatchAction::Resume => "resume",
    }
}
