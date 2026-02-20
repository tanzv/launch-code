use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

const SESSION_LOOKUP_SCHEMA_VERSION: u32 = 1;
const SESSION_LOOKUP_FILE: &str = "session-index.json";
const SESSION_LOOKUP_LOCK_FILE: &str = "session-index.lock";

fn default_schema_version() -> u32 {
    SESSION_LOOKUP_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionLookupIndex {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    sessions: BTreeMap<String, SessionLookupRecord>,
}

impl Default for SessionLookupIndex {
    fn default() -> Self {
        Self {
            schema_version: SESSION_LOOKUP_SCHEMA_VERSION,
            sessions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionLookupRecord {
    path: String,
    updated_at: u64,
}

pub(crate) fn lookup_session_path(session_id: &str) -> Result<Option<PathBuf>, AppError> {
    let index = load_index()?;
    Ok(index
        .sessions
        .get(session_id)
        .map(|record| PathBuf::from(&record.path)))
}

pub(crate) fn upsert_session_path(session_id: &str, path: &Path) -> Result<(), AppError> {
    let normalized = normalize_lookup_path(path);
    let normalized_display = normalized.to_string_lossy().to_string();
    let now = launch_code::model::unix_timestamp_secs();
    update_index(|index| {
        index.sessions.insert(
            session_id.to_string(),
            SessionLookupRecord {
                path: normalized_display.clone(),
                updated_at: now,
            },
        );
        Ok(())
    })
}

pub(crate) fn upsert_sessions_for_path<I>(session_ids: I, path: &Path) -> Result<(), AppError>
where
    I: IntoIterator<Item = String>,
{
    let ids: BTreeSet<String> = session_ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(());
    }

    let normalized = normalize_lookup_path(path);
    let normalized_display = normalized.to_string_lossy().to_string();
    let now = launch_code::model::unix_timestamp_secs();

    update_index(|index| {
        for session_id in &ids {
            index.sessions.insert(
                session_id.clone(),
                SessionLookupRecord {
                    path: normalized_display.clone(),
                    updated_at: now,
                },
            );
        }
        Ok(())
    })
}

pub(crate) fn remove_session_mapping(session_id: &str) -> Result<(), AppError> {
    update_index(|index| {
        index.sessions.remove(session_id);
        Ok(())
    })
}

pub(crate) fn remove_session_mappings<I>(session_ids: I) -> Result<(), AppError>
where
    I: IntoIterator<Item = String>,
{
    let ids: BTreeSet<String> = session_ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(());
    }

    update_index(|index| {
        for session_id in &ids {
            index.sessions.remove(session_id);
        }
        Ok(())
    })
}

fn load_index() -> Result<SessionLookupIndex, AppError> {
    let path = session_lookup_path()?;
    let lock_file = open_session_lookup_lock(&path)?;
    FileExt::lock_shared(&lock_file)?;
    let result = load_index_unlocked(&path);
    let _ = FileExt::unlock(&lock_file);
    result
}

fn update_index<F>(mutate: F) -> Result<(), AppError>
where
    F: FnOnce(&mut SessionLookupIndex) -> Result<(), AppError>,
{
    let path = session_lookup_path()?;
    let lock_file = open_session_lookup_lock(&path)?;
    FileExt::lock_exclusive(&lock_file)?;

    let result = (|| -> Result<(), AppError> {
        let mut index = load_index_unlocked(&path)?;
        mutate(&mut index)?;
        save_index_unlocked(&path, &index)?;
        Ok(())
    })();

    let _ = FileExt::unlock(&lock_file);
    result
}

fn session_lookup_path() -> Result<PathBuf, AppError> {
    Ok(crate::link_registry::resolve_home_dir()?
        .join(".launch-code")
        .join(SESSION_LOOKUP_FILE))
}

fn load_index_unlocked(path: &Path) -> Result<SessionLookupIndex, AppError> {
    if !path.exists() {
        return Ok(SessionLookupIndex::default());
    }
    let payload = fs::read_to_string(path)?;
    if payload.trim().is_empty() {
        return Ok(SessionLookupIndex::default());
    }
    let mut index: SessionLookupIndex = serde_json::from_str(&payload)?;
    if index.schema_version == 0 {
        index.schema_version = SESSION_LOOKUP_SCHEMA_VERSION;
    }
    Ok(index)
}

fn save_index_unlocked(path: &Path, index: &SessionLookupIndex) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut persisted = index.clone();
    if persisted.schema_version == 0 {
        persisted.schema_version = SESSION_LOOKUP_SCHEMA_VERSION;
    }
    let payload = serde_json::to_string_pretty(&persisted)?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, payload.as_bytes())?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn open_session_lookup_lock(path: &Path) -> Result<std::fs::File, AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_path = path.with_file_name(SESSION_LOOKUP_LOCK_FILE);
    Ok(OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?)
}

fn normalize_lookup_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
