use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fs2::FileExt;
use launch_code::model::{
    AppState, LaunchMode, LaunchSpec, RuntimeKind, SessionRecord, SessionStatus,
};
use launch_code::state::{StateError, StateStore};
use tempfile::tempdir;

#[test]
fn state_store_persists_sessions_to_disk() {
    let tmp = tempdir().expect("temp dir should exist");
    let store = StateStore::new(tmp.path());

    let mut state = AppState::default();
    state.sessions.insert(
        "session-1".to_string(),
        SessionRecord {
            id: "session-1".to_string(),
            spec: LaunchSpec {
                name: "python-api".to_string(),
                runtime: RuntimeKind::Python,
                entry: "app.py".to_string(),
                args: vec!["--port".to_string(), "8000".to_string()],
                cwd: ".".to_string(),
                env: BTreeMap::new(),
                env_remove: Vec::new(),
                managed: false,
                mode: LaunchMode::Run,
                debug: None,
                prelaunch_task: None,
                poststop_task: None,
            },
            status: SessionStatus::Running,
            pid: Some(43210),
            supervisor_pid: None,
            log_path: Some(".launch-code/logs/session-1.log".to_string()),
            debug_meta: None,
            created_at: 1,
            updated_at: 2,
            last_exit_code: None,
            restart_count: 0,
        },
    );

    store.save(&state).expect("state save should succeed");

    let loaded = store.load().expect("state load should succeed");
    assert_eq!(
        loaded.schema_version,
        launch_code::model::APP_STATE_SCHEMA_VERSION
    );
    let restored = loaded
        .sessions
        .get("session-1")
        .expect("session should be restored");
    assert_eq!(restored.spec.entry, "app.py");
    assert_eq!(restored.status, SessionStatus::Running);
    assert_eq!(restored.pid, Some(43210));

    let tmp_state_path = tmp.path().join(".launch-code").join("state.json.tmp");
    assert!(
        !tmp_state_path.exists(),
        "temporary state file should be cleaned up after save"
    );
}

#[test]
fn state_store_loads_legacy_state_without_schema_version() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    fs::write(
        &state_path,
        r#"{
  "profiles": {},
  "sessions": {}
}
"#,
    )
    .expect("legacy state should be written");

    let store = StateStore::new(tmp.path());
    let loaded = store.load().expect("legacy state should load");
    assert_eq!(
        loaded.schema_version,
        launch_code::model::APP_STATE_SCHEMA_VERSION
    );
}

#[test]
fn state_store_migrates_legacy_debug_meta_fields_from_runtime() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "profiles": {},
  "sessions": {
    "session-1": {
      "id": "session-1",
      "spec": {
        "name": "py-debug",
        "runtime": "python",
        "entry": "app.py",
        "args": [],
        "cwd": ".",
        "env": {},
        "managed": false,
        "mode": "debug",
        "debug": {
          "host": "127.0.0.1",
          "port": 5678,
          "wait_for_client": true,
          "subprocess": true
        },
        "prelaunch_task": null,
        "poststop_task": null
      },
      "status": "running",
      "pid": 12345,
      "supervisor_pid": null,
      "log_path": null,
      "debug_meta": {
        "host": "127.0.0.1",
        "requested_port": 5678,
        "active_port": 5678,
        "fallback_applied": false,
        "reconnect_policy": "auto-retry"
      },
      "created_at": 1,
      "updated_at": 2,
      "last_exit_code": null,
      "restart_count": 0
    }
  }
}
"#,
    )
    .expect("legacy debug state should be written");

    let store = StateStore::new(tmp.path());
    let loaded = store.load().expect("legacy debug state should load");
    let session = loaded
        .sessions
        .get("session-1")
        .expect("session should be present");
    let meta = session
        .debug_meta
        .as_ref()
        .expect("debug meta should be present");
    assert_eq!(meta.adapter_kind, "python-debugpy");
    assert_eq!(meta.transport, "tcp");
    assert!(
        meta.capabilities.iter().any(|value| value == "dap"),
        "python debug meta should include dap capability"
    );
}

#[test]
fn state_store_rejects_future_schema_version() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let state_path = state_dir.join("state.json");
    fs::write(
        &state_path,
        r#"{
  "schema_version": 999,
  "profiles": {},
  "sessions": {}
}
"#,
    )
    .expect("future state should be written");

    let store = StateStore::new(tmp.path());
    let err = store
        .load()
        .expect_err("future schema version should be rejected");
    assert!(
        matches!(
            err,
            StateError::UnsupportedStateSchemaVersion {
                found: 999,
                supported: launch_code::model::APP_STATE_SCHEMA_VERSION
            }
        ),
        "unexpected error: {err:?}"
    );
}

#[test]
fn state_store_save_waits_for_existing_exclusive_lock() {
    let tmp = tempdir().expect("temp dir should exist");
    let store = StateStore::new(tmp.path());
    let state_dir = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_dir).expect("state dir should exist");
    let lock_path = state_dir.join("state.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .expect("lock file should open");
    FileExt::lock_exclusive(&lock_file).expect("exclusive lock should be acquired");

    let store_for_thread = store.clone();
    let (tx, rx) = mpsc::channel::<()>();
    let join = thread::spawn(move || {
        let state = AppState::default();
        store_for_thread
            .save(&state)
            .expect("save should succeed after lock is released");
        tx.send(()).expect("send completion signal");
    });

    thread::sleep(Duration::from_millis(200));
    assert!(
        rx.try_recv().is_err(),
        "save should still block while another process holds the lock"
    );

    FileExt::unlock(&lock_file).expect("exclusive lock should be released");
    rx.recv_timeout(Duration::from_secs(2))
        .expect("save should finish after lock release");
    join.join().expect("thread should join");
}

#[test]
fn state_store_update_serializes_concurrent_writes() {
    let tmp = tempdir().expect("temp dir should exist");
    let store = Arc::new(StateStore::new(tmp.path()));
    let worker_count = 12usize;
    let barrier = Arc::new(Barrier::new(worker_count));
    let mut joins = Vec::new();

    for idx in 0..worker_count {
        let store_for_thread = Arc::clone(&store);
        let barrier_for_thread = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            barrier_for_thread.wait();
            let session_id = format!("session-{idx}");
            store_for_thread
                .update(|state| {
                    state.sessions.insert(
                        session_id.clone(),
                        SessionRecord {
                            id: session_id.clone(),
                            spec: LaunchSpec {
                                name: format!("job-{idx}"),
                                runtime: RuntimeKind::Python,
                                entry: format!("job_{idx}.py"),
                                args: Vec::new(),
                                cwd: ".".to_string(),
                                env: BTreeMap::new(),
                                env_remove: Vec::new(),
                                managed: false,
                                mode: LaunchMode::Run,
                                debug: None,
                                prelaunch_task: None,
                                poststop_task: None,
                            },
                            status: SessionStatus::Running,
                            pid: Some(1000 + idx as u32),
                            supervisor_pid: None,
                            log_path: None,
                            debug_meta: None,
                            created_at: idx as u64,
                            updated_at: idx as u64,
                            last_exit_code: None,
                            restart_count: 0,
                        },
                    );
                    Ok::<(), launch_code::state::StateError>(())
                })
                .expect("update should succeed");
        }));
    }

    for join in joins {
        join.join().expect("thread should join");
    }

    let loaded = store.load().expect("state load should succeed");
    assert_eq!(loaded.sessions.len(), worker_count);
    for idx in 0..worker_count {
        assert!(loaded.sessions.contains_key(&format!("session-{idx}")));
    }
}

#[test]
fn state_store_normalizes_launch_code_directory_root() {
    let tmp = tempdir().expect("temp dir should exist");
    let state_root = tmp.path().join(".launch-code");
    fs::create_dir_all(&state_root).expect("state root should exist");

    let store = StateStore::new(&state_root);
    assert_eq!(
        store.root_path(),
        tmp.path(),
        "state store root should normalize to workspace root when input points to .launch-code"
    );
    assert_eq!(
        store.state_dir_path(),
        state_root,
        "state directory should remain the canonical .launch-code path"
    );
}
