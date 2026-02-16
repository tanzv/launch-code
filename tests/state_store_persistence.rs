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
use launch_code::state::StateStore;
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
    let restored = loaded
        .sessions
        .get("session-1")
        .expect("session should be restored");
    assert_eq!(restored.spec.entry, "app.py");
    assert_eq!(restored.status, SessionStatus::Running);
    assert_eq!(restored.pid, Some(43210));
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
