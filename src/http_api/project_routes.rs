use launch_code::state::StateStore;
use serde_json::json;

use crate::app::{
    ProjectField, ProjectUpdate, api_get_project_info, api_unset_project_info_fields,
    api_update_project_info,
};
use crate::http_utils::{http_json, http_json_body_error, http_read_json_object_body};

pub(super) fn handle_project_get(
    store: &StateStore,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match api_get_project_info(store) {
        Ok(project) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "project": project}),
        ),
        Err(err) => crate::http_utils::http_json_error(&err),
    }
}

pub(super) fn handle_project_put_or_patch(
    store: &StateStore,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => return http_json_body_error(err),
    };

    let update = match parse_project_update_payload(&payload) {
        Ok(value) => value,
        Err(message) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": message}),
            );
        }
    };

    if !update.has_changes() {
        return http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": "project update payload is empty"}),
        );
    }

    match api_update_project_info(store, &update) {
        Ok(project) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "project": project}),
        ),
        Err(err) => crate::http_utils::http_json_error(&err),
    }
}

pub(super) fn handle_project_delete(
    store: &StateStore,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let payload = match http_read_json_object_body(request) {
        Ok(value) => value,
        Err(err) => return http_json_body_error(err),
    };

    let fields = match parse_project_delete_fields(&payload) {
        Ok(value) => value,
        Err(message) => {
            return http_json(
                tiny_http::StatusCode(400),
                json!({"ok": false, "error": "bad_request", "message": message}),
            );
        }
    };

    match api_unset_project_info_fields(store, &fields) {
        Ok(project) => http_json(
            tiny_http::StatusCode(200),
            json!({"ok": true, "project": project}),
        ),
        Err(err) => crate::http_utils::http_json_error(&err),
    }
}

fn parse_project_update_payload(payload: &serde_json::Value) -> Result<ProjectUpdate, String> {
    let mut update = ProjectUpdate::default();

    if let Some(value) = payload.get("name") {
        update.name = Some(parse_optional_string(value, "name")?);
    }
    if let Some(value) = payload.get("description") {
        update.description = Some(parse_optional_string(value, "description")?);
    }
    if let Some(value) = payload.get("repository") {
        update.repository = Some(parse_optional_string(value, "repository")?);
    }
    if let Some(value) = payload.get("languages") {
        update.languages = Some(parse_optional_string_list(value, "languages")?);
    }
    if let Some(value) = payload.get("runtimes") {
        update.runtimes = Some(parse_optional_string_list(value, "runtimes")?);
    }
    if let Some(value) = payload.get("tools") {
        update.tools = Some(parse_optional_string_list(value, "tools")?);
    }
    if let Some(value) = payload.get("tags") {
        update.tags = Some(parse_optional_string_list(value, "tags")?);
    }

    Ok(update)
}

fn parse_optional_string(value: &serde_json::Value, key: &str) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .map(|item| Some(item.to_string()))
        .ok_or_else(|| format!("{key} must be a string or null"))
}

fn parse_optional_string_list(
    value: &serde_json::Value,
    key: &str,
) -> Result<Option<Vec<String>>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let Some(items) = value.as_array() else {
        return Err(format!("{key} must be an array of strings or null"));
    };

    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = item.as_str() else {
            return Err(format!("{key} entries must be strings"));
        };
        values.push(text.to_string());
    }
    Ok(Some(values))
}

fn parse_project_delete_fields(payload: &serde_json::Value) -> Result<Vec<ProjectField>, String> {
    let all = payload
        .get("all")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let Some(fields_value) = payload.get("fields") else {
        if all || payload.as_object().is_some_and(|obj| obj.is_empty()) {
            return Ok(vec![ProjectField::All]);
        }
        return Err("fields must be provided as an array".to_string());
    };

    let Some(items) = fields_value.as_array() else {
        return Err("fields must be an array".to_string());
    };

    if items.is_empty() {
        return Ok(vec![ProjectField::All]);
    }

    let mut fields = Vec::new();
    for item in items {
        let Some(name) = item.as_str() else {
            return Err("fields entries must be strings".to_string());
        };
        let field = parse_project_field(name)
            .ok_or_else(|| format!("unsupported project field: {name}"))?;
        fields.push(field);
    }

    if all && !fields.contains(&ProjectField::All) {
        fields.push(ProjectField::All);
    }

    Ok(fields)
}

fn parse_project_field(value: &str) -> Option<ProjectField> {
    match value.trim().to_ascii_lowercase().as_str() {
        "name" => Some(ProjectField::Name),
        "description" => Some(ProjectField::Description),
        "repository" => Some(ProjectField::Repository),
        "languages" => Some(ProjectField::Languages),
        "runtimes" => Some(ProjectField::Runtimes),
        "tools" => Some(ProjectField::Tools),
        "tags" => Some(ProjectField::Tags),
        "all" => Some(ProjectField::All),
        _ => None,
    }
}
