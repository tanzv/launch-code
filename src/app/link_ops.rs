use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{LinkAddArgs, LinkArgs, LinkCommands, LinkNameArgs, LinkPruneArgs};
use crate::error::AppError;
use crate::link_registry::{LinkRecord, load_registry, normalize_link_path, update_registry};
use crate::output;

const AUTO_PRUNE_MIN_LINKS_DEFAULT: usize = 256;
const AUTO_PRUNE_MIN_MATCHED_DEFAULT: usize = 64;
const AUTO_PRUNE_RATIO_PERCENT_DEFAULT: usize = 30;
const AUTO_PRUNE_VERBOSE_ENV: &str = "LCODE_AUTO_PRUNE_VERBOSE";

#[derive(Debug, Clone)]
struct LinkSummaryRow {
    name: String,
    path: String,
    project_name: Option<String>,
    project_repository: Option<String>,
    session_count: usize,
}

#[derive(Debug, Clone)]
struct LinkPruneCandidate {
    name: String,
    path: String,
    reason: &'static str,
}

pub(super) fn auto_prune_stale_links_for_global_scan() -> Result<usize, AppError> {
    let registry = load_registry()?;
    let total = registry.links.len();
    let verbose = env_bool(AUTO_PRUNE_VERBOSE_ENV, false);
    let min_links = env_usize("LCODE_AUTO_PRUNE_MIN_LINKS", AUTO_PRUNE_MIN_LINKS_DEFAULT);
    if total < min_links {
        log_auto_prune(
            verbose,
            &format!("lcode_auto_prune skipped total_links={total} below_min_links={min_links}"),
        );
        return Ok(0);
    }

    let args = LinkPruneArgs {
        dry_run: false,
        missing_only: false,
        temp_only: false,
    };
    let candidates = collect_link_prune_candidates(&registry, &args);
    if candidates.is_empty() {
        log_auto_prune(verbose, "lcode_auto_prune skipped no_candidates");
        return Ok(0);
    }

    let min_matched = env_usize(
        "LCODE_AUTO_PRUNE_MIN_MATCHED",
        AUTO_PRUNE_MIN_MATCHED_DEFAULT,
    );
    let ratio_percent = env_usize(
        "LCODE_AUTO_PRUNE_RATIO_PERCENT",
        AUTO_PRUNE_RATIO_PERCENT_DEFAULT,
    );
    let ratio_hit = candidates.len().saturating_mul(100) >= total.saturating_mul(ratio_percent);
    let count_hit = candidates.len() >= min_matched;
    if !ratio_hit && !count_hit {
        log_auto_prune(
            verbose,
            &format!(
                "lcode_auto_prune skipped matched={} total={total} min_matched={min_matched} ratio_percent={ratio_percent}",
                candidates.len()
            ),
        );
        return Ok(0);
    }

    let removed_count = update_registry(|registry| {
        let mut removed = 0usize;
        for candidate in &candidates {
            if registry.remove(&candidate.name).is_some() {
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    })?;

    log_auto_prune(
        verbose,
        &format!(
            "lcode_auto_prune applied matched={} removed={removed_count} total={total}",
            candidates.len(),
        ),
    );

    Ok(removed_count)
}

pub(super) fn handle_link(args: &LinkArgs) -> Result<(), AppError> {
    match &args.command {
        LinkCommands::List => handle_link_list(),
        LinkCommands::Show(args) => handle_link_show(args),
        LinkCommands::Add(args) => handle_link_add(args),
        LinkCommands::Remove(args) => handle_link_remove(args),
        LinkCommands::Prune(args) => handle_link_prune(args),
    }
}

fn env_usize(name: &str, default_value: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn env_bool(name: &str, default_value: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default_value)
}

fn log_auto_prune(enabled: bool, message: &str) {
    if enabled {
        eprintln!("{message}");
    }
}

fn handle_link_list() -> Result<(), AppError> {
    let registry = load_registry()?;
    let items = registry
        .list()
        .iter()
        .map(build_link_summary_row)
        .collect::<Vec<_>>();

    if output::is_json_mode() {
        let payload_items: Vec<serde_json::Value> = items
            .iter()
            .map(|item| {
                json!({
                    "name": item.name,
                    "path": item.path,
                    "project_name": item.project_name,
                    "project_repository": item.project_repository,
                    "session_count": item.session_count,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "items": payload_items,
        }));
        return Ok(());
    }

    if items.is_empty() {
        output::print_message("no links");
        return Ok(());
    }

    let lines = items
        .iter()
        .map(|item| {
            format!(
                "{}\t{}\tproject={}\tsessions={}",
                item.name,
                item.path,
                item.project_name.as_deref().unwrap_or("none"),
                item.session_count
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    output::print_text_block(&format!("{lines}\n"));
    Ok(())
}

fn handle_link_show(args: &LinkNameArgs) -> Result<(), AppError> {
    let registry = load_registry()?;
    let item = registry
        .get(&args.name)
        .cloned()
        .ok_or_else(|| AppError::LinkNotFound(args.name.clone()))?;
    let summary = build_link_summary_row(&item);

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "link": item,
            "project_name": summary.project_name,
            "project_repository": summary.project_repository,
            "session_count": summary.session_count,
        }));
        return Ok(());
    }

    output::print_message(&format!(
        "name={} path={} project={} sessions={}",
        summary.name,
        summary.path,
        summary.project_name.as_deref().unwrap_or("none"),
        summary.session_count
    ));
    Ok(())
}

fn handle_link_add(args: &LinkAddArgs) -> Result<(), AppError> {
    let input_path = args
        .path
        .clone()
        .unwrap_or(env::current_dir().map_err(AppError::from)?);
    let normalized_path = normalize_link_path(&input_path)?;
    let normalized_display = normalized_path.to_string_lossy().to_string();

    let item = update_registry(|registry| {
        Ok(registry.upsert(args.name.clone(), normalized_display.clone()))
    })?;

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "message": "link_saved=true",
            "link": item,
        }));
        return Ok(());
    }

    output::print_message(&format!(
        "link_saved=true name={} path={}",
        args.name, normalized_display
    ));
    Ok(())
}

fn handle_link_remove(args: &LinkNameArgs) -> Result<(), AppError> {
    let removed = update_registry(|registry| {
        registry
            .remove(&args.name)
            .ok_or_else(|| AppError::LinkNotFound(args.name.clone()))
    })?;

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "message": "link_removed=true",
            "link": removed,
        }));
        return Ok(());
    }

    output::print_message(&format!(
        "link_removed=true name={} path={}",
        removed.name, removed.path
    ));
    Ok(())
}

fn handle_link_prune(args: &LinkPruneArgs) -> Result<(), AppError> {
    let registry = load_registry()?;
    let candidates = collect_link_prune_candidates(&registry, args);
    let matched_count = candidates.len();

    if args.dry_run {
        return print_link_prune_result(true, matched_count, 0, &candidates);
    }

    let removed = update_registry(|registry| {
        let mut removed_items = Vec::new();
        for candidate in &candidates {
            if let Some(item) = registry.remove(&candidate.name) {
                removed_items.push(item);
            }
        }
        Ok(removed_items)
    })?;

    print_link_prune_result(false, matched_count, removed.len(), &candidates)
}

fn print_link_prune_result(
    dry_run: bool,
    matched_count: usize,
    removed_count: usize,
    candidates: &[LinkPruneCandidate],
) -> Result<(), AppError> {
    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = candidates
            .iter()
            .map(|candidate| {
                json!({
                    "name": candidate.name,
                    "path": candidate.path,
                    "reason": candidate.reason,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "dry_run": dry_run,
            "matched_count": matched_count,
            "removed_count": removed_count,
            "items": items,
        }));
        return Ok(());
    }

    let mut lines = vec![format!(
        "link_prune_dry_run={} matched={} removed={}",
        dry_run, matched_count, removed_count
    )];
    for candidate in candidates {
        lines.push(format!(
            "name={} reason={} path={}",
            candidate.name, candidate.reason, candidate.path
        ));
    }
    output::print_text_block(&format!("{}\n", lines.join("\n")));
    Ok(())
}

fn collect_link_prune_candidates(
    registry: &crate::link_registry::LinkRegistry,
    args: &LinkPruneArgs,
) -> Vec<LinkPruneCandidate> {
    let mut candidates = Vec::new();

    for item in registry.list() {
        let path = PathBuf::from(&item.path);
        let path_exists = path.exists();
        let temporary_path = is_temporary_link_path(&path);
        let state_empty = path_exists && link_state_is_empty(&item.path);
        let missing_match = !path_exists;
        let temp_match = temporary_path && state_empty;

        let (reason, matched) = if args.missing_only {
            ("missing_path", missing_match)
        } else if args.temp_only {
            ("temporary_empty_path", temp_match)
        } else if missing_match {
            ("missing_path", true)
        } else if temp_match {
            ("temporary_empty_path", true)
        } else {
            ("", false)
        };

        if matched {
            candidates.push(LinkPruneCandidate {
                name: item.name,
                path: item.path,
                reason,
            });
        }
    }

    candidates
}

fn is_temporary_link_path(path: &Path) -> bool {
    let temp_root = normalize_path_for_match(&env::temp_dir());
    let target = normalize_path_for_match(path);
    if target.starts_with(&temp_root) {
        return true;
    }

    let target_text = target.to_string_lossy();
    target_text.starts_with("/tmp/")
        || target_text.starts_with("/private/tmp/")
        || (target_text.contains("/var/folders/") && target_text.contains("/T/"))
}

fn normalize_path_for_match(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn link_state_is_empty(path: &str) -> bool {
    let store = StateStore::new(PathBuf::from(path));
    match store.load() {
        Ok(state) => state.sessions.is_empty() && state.project_info.is_none(),
        Err(_) => false,
    }
}

fn build_link_summary_row(record: &LinkRecord) -> LinkSummaryRow {
    let mut summary = LinkSummaryRow {
        name: record.name.clone(),
        path: record.path.clone(),
        project_name: None,
        project_repository: None,
        session_count: 0,
    };

    let store = StateStore::new(PathBuf::from(&record.path));
    if let Ok(state) = store.load() {
        summary.session_count = state.sessions.len();
        if let Some(project) = state.project_info {
            summary.project_name = project.name;
            summary.project_repository = project.repository;
        }
    }

    summary
}
