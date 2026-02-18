use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

const LINK_REGISTRY_SCHEMA_VERSION: u32 = 1;
const LINK_REGISTRY_FILE: &str = "links.json";
const LINK_REGISTRY_LOCK_FILE: &str = "links.lock";

fn default_schema_version() -> u32 {
    LINK_REGISTRY_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LinkRegistry {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub links: BTreeMap<String, LinkRecord>,
}

impl Default for LinkRegistry {
    fn default() -> Self {
        Self {
            schema_version: LINK_REGISTRY_SCHEMA_VERSION,
            links: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LinkRecord {
    pub name: String,
    pub path: String,
}

impl LinkRegistry {
    pub(crate) fn get(&self, name: &str) -> Option<&LinkRecord> {
        self.links.get(name)
    }

    pub(crate) fn find_by_path(&self, path: &str) -> Option<&LinkRecord> {
        self.links.values().find(|record| record.path == path)
    }

    pub(crate) fn upsert(&mut self, name: String, path: String) -> LinkRecord {
        let record = LinkRecord {
            name: name.clone(),
            path,
        };
        self.links.insert(name, record.clone());
        record
    }

    pub(crate) fn remove(&mut self, name: &str) -> Option<LinkRecord> {
        self.links.remove(name)
    }

    pub(crate) fn list(&self) -> Vec<LinkRecord> {
        self.links.values().cloned().collect()
    }
}

pub(crate) fn resolve_home_dir() -> Result<PathBuf, AppError> {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        return Ok(home);
    }

    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        return Ok(home);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "HOME and USERPROFILE are not set",
    )
    .into())
}

pub(crate) fn registry_path() -> Result<PathBuf, AppError> {
    Ok(resolve_home_dir()?
        .join(".launch-code")
        .join(LINK_REGISTRY_FILE))
}

pub(crate) fn load_registry() -> Result<LinkRegistry, AppError> {
    let path = registry_path()?;
    let lock_file = open_registry_lock(&path)?;
    FileExt::lock_shared(&lock_file)?;
    let result = load_registry_unlocked(&path);
    let _ = FileExt::unlock(&lock_file);
    result
}

pub(crate) fn update_registry<F, T>(mutate: F) -> Result<T, AppError>
where
    F: FnOnce(&mut LinkRegistry) -> Result<T, AppError>,
{
    let path = registry_path()?;
    let lock_file = open_registry_lock(&path)?;
    FileExt::lock_exclusive(&lock_file)?;

    let result = (|| -> Result<T, AppError> {
        let mut registry = load_registry_unlocked(&path)?;
        let output = mutate(&mut registry)?;
        save_registry_unlocked(&path, &registry)?;
        Ok(output)
    })();

    let _ = FileExt::unlock(&lock_file);
    result
}

pub(crate) fn resolve_link_path(name: &str) -> Result<PathBuf, AppError> {
    let registry = load_registry()?;
    let record = registry
        .get(name)
        .ok_or_else(|| AppError::LinkNotFound(name.to_string()))?;
    Ok(PathBuf::from(&record.path))
}

pub(crate) fn ensure_link_for_workspace(path: &Path) -> Result<LinkRecord, AppError> {
    let normalized = normalize_link_path(path)?;
    let normalized_display = normalized.to_string_lossy().to_string();
    update_registry(|registry| {
        if let Some(existing) = registry.find_by_path(&normalized_display) {
            return Ok(existing.clone());
        }

        let base_name = normalized
            .file_name()
            .and_then(|value| value.to_str())
            .map(sanitize_link_name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "workspace".to_string());

        let mut candidate = base_name.clone();
        let mut suffix = 2u32;
        while registry.get(&candidate).is_some() {
            candidate = format!("{base_name}-{suffix}");
            suffix += 1;
        }

        Ok(registry.upsert(candidate, normalized_display.clone()))
    })
}

pub(crate) fn normalize_link_path(path: &Path) -> Result<PathBuf, AppError> {
    fs::canonicalize(path).map_err(|err| AppError::InvalidLinkPath(format!("{path:?}: {err}")))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn load_registry_unlocked(path: &Path) -> Result<LinkRegistry, AppError> {
    if !path.exists() {
        return Ok(LinkRegistry::default());
    }
    let payload = fs::read_to_string(path)?;
    if payload.trim().is_empty() {
        return Ok(LinkRegistry::default());
    }
    let mut registry: LinkRegistry = serde_json::from_str(&payload)?;
    if registry.schema_version == 0 {
        registry.schema_version = LINK_REGISTRY_SCHEMA_VERSION;
    }
    Ok(registry)
}

fn save_registry_unlocked(path: &Path, registry: &LinkRegistry) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut persisted = registry.clone();
    if persisted.schema_version == 0 {
        persisted.schema_version = LINK_REGISTRY_SCHEMA_VERSION;
    }
    let payload = serde_json::to_string_pretty(&persisted)?;
    write_atomically(path, payload.as_bytes())?;
    Ok(())
}

fn open_registry_lock(path: &Path) -> Result<std::fs::File, AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_path = path.with_file_name(LINK_REGISTRY_LOCK_FILE);
    Ok(OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?)
}

fn sanitize_link_name(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            continue;
        }
        if ch == '-' || ch == '_' {
            output.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            output.push('-');
        }
    }

    while output.contains("--") {
        output = output.replace("--", "-");
    }
    output.trim_matches('-').to_string()
}
