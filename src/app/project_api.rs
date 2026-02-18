use std::collections::HashSet;

use launch_code::model::ProjectInfo;
use launch_code::state::StateStore;

use crate::error::AppError;

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectUpdate {
    pub name: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub repository: Option<Option<String>>,
    pub languages: Option<Option<Vec<String>>>,
    pub runtimes: Option<Option<Vec<String>>>,
    pub tools: Option<Option<Vec<String>>>,
    pub tags: Option<Option<Vec<String>>>,
}

impl ProjectUpdate {
    pub(crate) fn has_changes(&self) -> bool {
        self.name.is_some()
            || self.description.is_some()
            || self.repository.is_some()
            || self.languages.is_some()
            || self.runtimes.is_some()
            || self.tools.is_some()
            || self.tags.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectField {
    Name,
    Description,
    Repository,
    Languages,
    Runtimes,
    Tools,
    Tags,
    All,
}

pub(crate) fn api_get_project_info(store: &StateStore) -> Result<Option<ProjectInfo>, AppError> {
    let state = store.load()?;
    Ok(state.project_info)
}

pub(crate) fn api_update_project_info(
    store: &StateStore,
    update: &ProjectUpdate,
) -> Result<Option<ProjectInfo>, AppError> {
    if !update.has_changes() {
        return api_get_project_info(store);
    }

    store.update::<_, _, AppError>(|state| {
        let project = state.project_info.get_or_insert_with(ProjectInfo::default);

        if let Some(name) = &update.name {
            project.name = normalize_optional_string(name.as_deref());
        }
        if let Some(description) = &update.description {
            project.description = normalize_optional_string(description.as_deref());
        }
        if let Some(repository) = &update.repository {
            project.repository = normalize_optional_string(repository.as_deref());
        }
        if let Some(languages) = &update.languages {
            project.languages = normalize_string_list(languages.as_deref());
        }
        if let Some(runtimes) = &update.runtimes {
            project.runtimes = normalize_string_list(runtimes.as_deref());
        }
        if let Some(tools) = &update.tools {
            project.tools = normalize_string_list(tools.as_deref());
        }
        if let Some(tags) = &update.tags {
            project.tags = normalize_string_list(tags.as_deref());
        }

        if project.is_empty() {
            state.project_info = None;
        }

        Ok(state.project_info.clone())
    })
}

pub(crate) fn api_unset_project_info_fields(
    store: &StateStore,
    fields: &[ProjectField],
) -> Result<Option<ProjectInfo>, AppError> {
    store.update::<_, _, AppError>(|state| {
        if fields.contains(&ProjectField::All) {
            state.project_info = None;
            return Ok(None);
        }

        let Some(project) = state.project_info.as_mut() else {
            return Ok(None);
        };

        for field in fields {
            match field {
                ProjectField::Name => project.name = None,
                ProjectField::Description => project.description = None,
                ProjectField::Repository => project.repository = None,
                ProjectField::Languages => project.languages = None,
                ProjectField::Runtimes => project.runtimes = None,
                ProjectField::Tools => project.tools = None,
                ProjectField::Tags => project.tags = None,
                ProjectField::All => {}
            }
        }

        if project.is_empty() {
            state.project_info = None;
        }

        Ok(state.project_info.clone())
    })
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_string_list(values: Option<&[String]>) -> Option<Vec<String>> {
    let values = values?;

    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let item = value.trim();
        if item.is_empty() {
            continue;
        }
        let owned = item.to_string();
        if seen.insert(owned.clone()) {
            normalized.push(owned);
        }
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}
