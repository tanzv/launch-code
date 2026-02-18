use std::collections::{BTreeSet, HashSet};

use launch_code::model::ProjectInfo;
use launch_code::state::StateStore;
use serde_json::json;

use super::{
    ProjectField, ProjectUpdate, api_get_project_info, api_unset_project_info_fields,
    api_update_project_info,
};
use crate::cli::{
    ProjectArgs, ProjectCommands, ProjectListArgs, ProjectListFieldArg, ProjectSetArgs,
    ProjectUnsetArgs, ProjectUnsetFieldArg,
};
use crate::error::AppError;
use crate::link_registry::load_registry;
use crate::output;

#[derive(Debug, Clone)]
struct ProjectFieldRow {
    field: &'static str,
    value: Option<String>,
    is_set: bool,
}

#[derive(Debug, Clone)]
struct GlobalProjectRow {
    link_name: String,
    link_path: String,
    project: ProjectInfo,
}

pub(super) fn handle_project(store: &StateStore, args: &ProjectArgs) -> Result<(), AppError> {
    match &args.command {
        ProjectCommands::Show => handle_project_show(store),
        ProjectCommands::List(args) => handle_project_list(store, args),
        ProjectCommands::Set(args) => handle_project_set(store, args),
        ProjectCommands::Unset(args) => handle_project_unset(store, args),
        ProjectCommands::Clear => handle_project_clear(store),
    }
}

pub(super) fn handle_project_show_global_default() -> Result<(), AppError> {
    let rows = collect_global_project_rows()?;

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "link": {
                        "name": row.link_name,
                        "path": row.link_path,
                    },
                    "project": row.project,
                })
            })
            .collect();
        let project = if rows.len() == 1 {
            Some(rows[0].project.clone())
        } else {
            None
        };
        output::print_json_doc(&json!({
            "ok": true,
            "scope": "global",
            "project": project,
            "items": items,
        }));
        return Ok(());
    }

    if rows.is_empty() {
        output::print_message("no project metadata");
        return Ok(());
    }

    if rows.len() == 1 {
        let mut lines = vec![
            "project metadata:".to_string(),
            format!("  link: {}", rows[0].link_name),
            format!("  path: {}", rows[0].link_path),
        ];
        let detail = format_project_show_text(&rows[0].project);
        lines.extend(
            detail
                .lines()
                .skip(1)
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );
        output::print_text_block(&format!("{}\n", lines.join("\n")));
        return Ok(());
    }

    let mut blocks = Vec::new();
    for row in rows {
        let detail = format_project_show_text(&row.project);
        let mut lines = vec![
            format!("project metadata (link={}):", row.link_name),
            format!("  path: {}", row.link_path),
        ];
        lines.extend(
            detail
                .lines()
                .skip(1)
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );
        blocks.push(lines.join("\n"));
    }
    output::print_text_block(&format!("{}\n", blocks.join("\n\n")));
    Ok(())
}

fn handle_project_show(store: &StateStore) -> Result<(), AppError> {
    let project = api_get_project_info(store)?;
    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "project": project,
        }));
        return Ok(());
    }

    let Some(project) = project.as_ref() else {
        output::print_message("no project metadata");
        return Ok(());
    };

    output::print_text_block(&format_project_show_text(project));
    Ok(())
}

fn handle_project_list(store: &StateStore, args: &ProjectListArgs) -> Result<(), AppError> {
    let project = api_get_project_info(store)?;
    let rows = build_project_field_rows(project.as_ref(), &args.fields, args.all);

    if output::is_json_mode() {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "field": row.field,
                    "value": row.value,
                    "is_set": row.is_set,
                })
            })
            .collect();
        output::print_json_doc(&json!({
            "ok": true,
            "project": project,
            "items": items,
        }));
        return Ok(());
    }

    if rows.is_empty() {
        output::print_message("no project metadata");
        return Ok(());
    }

    let lines = rows
        .iter()
        .map(format_project_list_row)
        .collect::<Vec<_>>()
        .join("\n");
    output::print_text_block(&format!("{lines}\n"));
    Ok(())
}

fn handle_project_set(store: &StateStore, args: &ProjectSetArgs) -> Result<(), AppError> {
    let update = ProjectUpdate {
        name: args.name.as_ref().map(|value| Some(value.clone())),
        description: args.description.as_ref().map(|value| Some(value.clone())),
        repository: args.repository.as_ref().map(|value| Some(value.clone())),
        languages: (!args.language.is_empty()).then_some(Some(args.language.clone())),
        runtimes: (!args.runtime.is_empty()).then_some(Some(args.runtime.clone())),
        tools: (!args.tool.is_empty()).then_some(Some(args.tool.clone())),
        tags: (!args.tag.is_empty()).then_some(Some(args.tag.clone())),
    };

    let fields = collect_set_fields(args);
    let project = api_update_project_info(store, &update)?;

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "message": "project_info_updated=true",
            "fields": fields,
            "project": project,
        }));
        return Ok(());
    }

    output::print_message(&format!(
        "project_info_updated=true fields={}",
        fields.join(",")
    ));
    Ok(())
}

fn handle_project_unset(store: &StateStore, args: &ProjectUnsetArgs) -> Result<(), AppError> {
    let fields: Vec<ProjectField> = args.fields.iter().map(map_unset_field).collect();
    let field_labels = collect_unset_fields(args);
    let project = api_unset_project_info_fields(store, &fields)?;

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "message": "project_info_unset=true",
            "fields": field_labels,
            "project": project,
        }));
        return Ok(());
    }

    output::print_message(&format!(
        "project_info_unset=true fields={}",
        field_labels.join(",")
    ));
    Ok(())
}

fn handle_project_clear(store: &StateStore) -> Result<(), AppError> {
    let project = api_unset_project_info_fields(store, &[ProjectField::All])?;

    if output::is_json_mode() {
        output::print_json_doc(&json!({
            "ok": true,
            "message": "project_info_cleared=true",
            "project": project,
        }));
        return Ok(());
    }

    output::print_message("project_info_cleared=true");
    Ok(())
}

fn map_unset_field(value: &ProjectUnsetFieldArg) -> ProjectField {
    match value {
        ProjectUnsetFieldArg::Name => ProjectField::Name,
        ProjectUnsetFieldArg::Description => ProjectField::Description,
        ProjectUnsetFieldArg::Repository => ProjectField::Repository,
        ProjectUnsetFieldArg::Languages => ProjectField::Languages,
        ProjectUnsetFieldArg::Runtimes => ProjectField::Runtimes,
        ProjectUnsetFieldArg::Tools => ProjectField::Tools,
        ProjectUnsetFieldArg::Tags => ProjectField::Tags,
        ProjectUnsetFieldArg::All => ProjectField::All,
    }
}

fn format_unset_field(value: &ProjectUnsetFieldArg) -> &'static str {
    match value {
        ProjectUnsetFieldArg::Name => "name",
        ProjectUnsetFieldArg::Description => "description",
        ProjectUnsetFieldArg::Repository => "repository",
        ProjectUnsetFieldArg::Languages => "languages",
        ProjectUnsetFieldArg::Runtimes => "runtimes",
        ProjectUnsetFieldArg::Tools => "tools",
        ProjectUnsetFieldArg::Tags => "tags",
        ProjectUnsetFieldArg::All => "all",
    }
}

fn collect_set_fields(args: &ProjectSetArgs) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if args.name.is_some() {
        fields.push("name");
    }
    if args.description.is_some() {
        fields.push("description");
    }
    if args.repository.is_some() {
        fields.push("repository");
    }
    if !args.language.is_empty() {
        fields.push("languages");
    }
    if !args.runtime.is_empty() {
        fields.push("runtimes");
    }
    if !args.tool.is_empty() {
        fields.push("tools");
    }
    if !args.tag.is_empty() {
        fields.push("tags");
    }
    fields
}

fn collect_unset_fields(args: &ProjectUnsetArgs) -> Vec<&'static str> {
    let mut seen = HashSet::new();
    let mut fields = Vec::new();
    for field in &args.fields {
        let label = format_unset_field(field);
        if seen.insert(label) {
            fields.push(label);
        }
    }
    fields
}

fn format_project_show_text(project: &ProjectInfo) -> String {
    let rows = build_project_field_rows(Some(project), &[], true);
    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push("project metadata:".to_string());
    for row in rows {
        let value = row.value.unwrap_or_else(|| "null".to_string());
        lines.push(format!("  {}: {}", row.field, value));
    }
    format!("{}\n", lines.join("\n"))
}

fn format_project_list_row(row: &ProjectFieldRow) -> String {
    let value = row.value.clone().unwrap_or_else(|| "null".to_string());
    format!("{}\t{}", row.field, value)
}

fn build_project_field_rows(
    project: Option<&ProjectInfo>,
    requested_fields: &[ProjectListFieldArg],
    include_empty: bool,
) -> Vec<ProjectFieldRow> {
    resolve_list_fields(requested_fields)
        .into_iter()
        .filter_map(|field| {
            let value = project.and_then(|value| get_project_field_value(value, field));
            let is_set = value.is_some();
            if include_empty || is_set {
                Some(ProjectFieldRow {
                    field: project_list_field_name(field),
                    value,
                    is_set,
                })
            } else {
                None
            }
        })
        .collect()
}

fn resolve_list_fields(requested_fields: &[ProjectListFieldArg]) -> Vec<ProjectListFieldArg> {
    let defaults = [
        ProjectListFieldArg::Name,
        ProjectListFieldArg::Description,
        ProjectListFieldArg::Repository,
        ProjectListFieldArg::Languages,
        ProjectListFieldArg::Runtimes,
        ProjectListFieldArg::Tools,
        ProjectListFieldArg::Tags,
    ];

    if requested_fields.is_empty() {
        return defaults.to_vec();
    }

    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for field in requested_fields {
        if seen.insert(*field) {
            output.push(*field);
        }
    }
    output
}

fn project_list_field_name(field: ProjectListFieldArg) -> &'static str {
    match field {
        ProjectListFieldArg::Name => "name",
        ProjectListFieldArg::Description => "description",
        ProjectListFieldArg::Repository => "repository",
        ProjectListFieldArg::Languages => "languages",
        ProjectListFieldArg::Runtimes => "runtimes",
        ProjectListFieldArg::Tools => "tools",
        ProjectListFieldArg::Tags => "tags",
    }
}

fn get_project_field_value(project: &ProjectInfo, field: ProjectListFieldArg) -> Option<String> {
    match field {
        ProjectListFieldArg::Name => project.name.clone(),
        ProjectListFieldArg::Description => project.description.clone(),
        ProjectListFieldArg::Repository => project.repository.clone(),
        ProjectListFieldArg::Languages => join_string_list(project.languages.as_deref()),
        ProjectListFieldArg::Runtimes => join_string_list(project.runtimes.as_deref()),
        ProjectListFieldArg::Tools => join_string_list(project.tools.as_deref()),
        ProjectListFieldArg::Tags => join_string_list(project.tags.as_deref()),
    }
}

fn join_string_list(values: Option<&[String]>) -> Option<String> {
    let values = values?;
    if values.is_empty() {
        return None;
    }
    Some(values.join(", "))
}

fn collect_global_project_rows() -> Result<Vec<GlobalProjectRow>, AppError> {
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let mut rows = Vec::new();

    for link in registry.list() {
        if !seen_paths.insert(link.path.clone()) {
            continue;
        }
        let store = StateStore::new(&link.path);
        let Some(project) = api_get_project_info(&store)? else {
            continue;
        };
        rows.push(GlobalProjectRow {
            link_name: link.name,
            link_path: link.path,
            project,
        });
    }

    rows.sort_by(|left, right| left.link_name.cmp(&right.link_name));
    Ok(rows)
}
