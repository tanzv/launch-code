use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use launch_code::model::{SessionRecord, SessionStatus};
use launch_code::state::StateStore;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

const LIST_CACHE_FILE: &str = "list-cache.json";
const LIST_CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StateFileSignature {
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionListCacheDoc {
    schema_version: u32,
    state_signature: StateFileSignature,
    sessions: Vec<SessionRecord>,
}

pub(super) fn load_sessions_for_listing(
    store: &StateStore,
) -> Result<Vec<SessionRecord>, AppError> {
    let state_path = store.state_file_path();
    if let Some(signature) = read_state_signature(&state_path) {
        if let Some(cached_sessions) = try_read_cache(store, &signature) {
            return Ok(cached_sessions);
        }
    }

    let sessions = super::super::api_list_sessions(store)?;
    refresh_cache_after_list(store, &state_path, &sessions);
    Ok(sessions)
}

fn refresh_cache_after_list(store: &StateStore, state_path: &Path, sessions: &[SessionRecord]) {
    if sessions.is_empty() || !sessions_are_cacheable(sessions) {
        remove_cache_file(store);
        return;
    }

    let Some(signature) = read_state_signature(state_path) else {
        remove_cache_file(store);
        return;
    };

    let _ = write_cache(store, &signature, sessions);
}

fn read_state_signature(path: &Path) -> Option<StateFileSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let timestamp = modified.duration_since(UNIX_EPOCH).ok()?;

    Some(StateFileSignature {
        len: metadata.len(),
        modified_secs: timestamp.as_secs(),
        modified_nanos: timestamp.subsec_nanos(),
    })
}

fn sessions_are_cacheable(sessions: &[SessionRecord]) -> bool {
    sessions.iter().all(session_is_cacheable)
}

fn session_is_cacheable(session: &SessionRecord) -> bool {
    session.pid.is_none() && matches!(session.status, SessionStatus::Stopped)
}

fn cache_path(store: &StateStore) -> PathBuf {
    store.state_dir_path().join(LIST_CACHE_FILE)
}

fn try_read_cache(
    store: &StateStore,
    signature: &StateFileSignature,
) -> Option<Vec<SessionRecord>> {
    let payload = fs::read_to_string(cache_path(store)).ok()?;
    let doc: SessionListCacheDoc = serde_json::from_str(&payload).ok()?;
    if doc.schema_version != LIST_CACHE_SCHEMA_VERSION {
        return None;
    }
    if &doc.state_signature != signature {
        return None;
    }
    if !sessions_are_cacheable(&doc.sessions) {
        return None;
    }
    Some(doc.sessions)
}

fn write_cache(
    store: &StateStore,
    signature: &StateFileSignature,
    sessions: &[SessionRecord],
) -> Result<(), AppError> {
    fs::create_dir_all(store.state_dir_path())?;
    let cache = SessionListCacheDoc {
        schema_version: LIST_CACHE_SCHEMA_VERSION,
        state_signature: signature.clone(),
        sessions: sessions.to_vec(),
    };
    let payload = serde_json::to_string(&cache)?;
    fs::write(cache_path(store), payload)?;
    Ok(())
}

fn remove_cache_file(store: &StateStore) {
    let path = cache_path(store);
    if !path.exists() {
        return;
    }
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    use launch_code::state::StateStore;
    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::{SessionListCacheDoc, cache_path, load_sessions_for_listing, read_state_signature};

    fn build_session(id: &str, name: &str) -> Value {
        json!({
            "id": id,
            "spec": {
                "name": name,
                "runtime": "python",
                "entry": "app.py",
                "args": [],
                "cwd": ".",
                "env": {},
                "managed": false,
                "mode": "run",
                "debug": null,
                "prelaunch_task": null,
                "poststop_task": null
            },
            "status": "stopped",
            "pid": null,
            "supervisor_pid": null,
            "log_path": null,
            "debug_meta": null,
            "created_at": 1,
            "updated_at": 1,
            "last_exit_code": null,
            "restart_count": 0
        })
    }

    fn write_state(root: &Path, sessions: Vec<Value>) {
        let state_dir = root.join(".launch-code");
        fs::create_dir_all(&state_dir).expect("state dir should exist");

        let mut session_map = serde_json::Map::new();
        for session in sessions {
            let id = session["id"]
                .as_str()
                .expect("session id should exist")
                .to_string();
            session_map.insert(id, session);
        }

        let state_doc = json!({
            "schema_version": 1,
            "profiles": {},
            "project_info": null,
            "sessions": session_map
        });
        fs::write(
            state_dir.join("state.json"),
            serde_json::to_string_pretty(&state_doc).expect("state json should serialize"),
        )
        .expect("state should be written");
    }

    #[test]
    fn load_sessions_for_listing_writes_cache_for_stopped_sessions() {
        let tmp = tempdir().expect("temp dir should exist");
        write_state(tmp.path(), vec![build_session("session-a", "alpha")]);
        let store = StateStore::new(tmp.path());

        let sessions = load_sessions_for_listing(&store).expect("list should succeed");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-a");

        let cache_payload =
            fs::read_to_string(cache_path(&store)).expect("cache file should be present");
        let cache_doc: SessionListCacheDoc =
            serde_json::from_str(&cache_payload).expect("cache json should be valid");
        assert_eq!(cache_doc.sessions.len(), 1);
        assert_eq!(cache_doc.sessions[0].id, "session-a");

        let state_signature =
            read_state_signature(&store.state_file_path()).expect("state signature should exist");
        assert_eq!(cache_doc.state_signature, state_signature);
    }

    #[test]
    fn load_sessions_for_listing_refreshes_cache_after_state_signature_change() {
        let tmp = tempdir().expect("temp dir should exist");
        let store = StateStore::new(tmp.path());

        write_state(tmp.path(), vec![build_session("session-a", "alpha")]);
        let first_sessions = load_sessions_for_listing(&store).expect("first list should succeed");
        assert_eq!(first_sessions.len(), 1);
        assert_eq!(first_sessions[0].id, "session-a");

        thread::sleep(Duration::from_millis(20));
        write_state(
            tmp.path(),
            vec![build_session("session-b-with-longer-id", "beta")],
        );

        let second_sessions =
            load_sessions_for_listing(&store).expect("second list should succeed");
        assert_eq!(second_sessions.len(), 1);
        assert_eq!(second_sessions[0].id, "session-b-with-longer-id");

        let cache_payload =
            fs::read_to_string(cache_path(&store)).expect("cache file should be present");
        let cache_doc: SessionListCacheDoc =
            serde_json::from_str(&cache_payload).expect("cache json should be valid");
        assert_eq!(cache_doc.sessions.len(), 1);
        assert_eq!(cache_doc.sessions[0].id, "session-b-with-longer-id");
    }
}
