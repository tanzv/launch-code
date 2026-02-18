use std::env;
use std::path::PathBuf;

use launch_code::state::StateStore;
use serde_json::json;

use crate::cli::{LinkAddArgs, LinkArgs, LinkCommands, LinkNameArgs};
use crate::error::AppError;
use crate::link_registry::{LinkRecord, load_registry, normalize_link_path, update_registry};
use crate::output;

#[derive(Debug, Clone)]
struct LinkSummaryRow {
    name: String,
    path: String,
    project_name: Option<String>,
    project_repository: Option<String>,
    session_count: usize,
}

pub(super) fn handle_link(args: &LinkArgs) -> Result<(), AppError> {
    match &args.command {
        LinkCommands::List => handle_link_list(),
        LinkCommands::Show(args) => handle_link_show(args),
        LinkCommands::Add(args) => handle_link_add(args),
        LinkCommands::Remove(args) => handle_link_remove(args),
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
