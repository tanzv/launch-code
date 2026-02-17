use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use launch_code::model::{LaunchSpec, SessionRecord, SessionStatus, unix_timestamp_secs};
use launch_code::process::{
    is_process_alive, resume_process, run_shell_task, stop_process_with_options, suspend_process,
};
use launch_code::runtime::build_command;
use launch_code::state::StateStore;
use serde_json::json;

use crate::error::AppError;

type ShellTaskSpec = (String, String, BTreeMap<String, String>, String);

#[derive(Clone)]
struct RestartPlan {
    session_id: String,
    expected_pid: Option<u32>,
    spec: LaunchSpec,
    log_path: PathBuf,
}

pub(crate) fn api_list_sessions(store: &StateStore) -> Result<Vec<SessionRecord>, AppError> {
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let ids: Vec<String> = state.sessions.keys().cloned().collect();
        for id in ids {
            let session = state
                .sessions
                .get_mut(&id)
                .ok_or_else(|| AppError::SessionNotFound(id.clone()))?;
            super::reconcile_session(store, session, now)?;
        }

        Ok(state.sessions.values().cloned().collect())
    })
}

pub(crate) fn api_get_session(
    store: &StateStore,
    session_id: &str,
) -> Result<SessionRecord, AppError> {
    let session_id = session_id.to_string();
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        super::reconcile_session(store, session, now)?;
        Ok(session.clone())
    })
}

pub(crate) fn api_inspect_session(
    store: &StateStore,
    session_id: &str,
    tail: usize,
) -> Result<serde_json::Value, AppError> {
    let session_id = session_id.to_string();
    let tail = tail.min(super::log_ops::MAX_LOG_TAIL_LINES);
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        super::reconcile_session(store, session, now)?;
        let pid = session.pid;
        let alive = pid.map(is_process_alive).unwrap_or(false);
        let command = build_command(&session.spec)?;
        let log_tail =
            super::log_ops::read_log_tail(session.log_path.as_deref(), tail).unwrap_or_default();

        Ok(json!({
            "ok": true,
            "session": session.clone(),
            "process": {
                "pid": pid,
                "alive": alive,
                "command": command,
            },
            "log": {
                "tail_lines": tail,
                "text": log_tail,
            }
        }))
    })
}

pub(crate) fn api_debug_session(
    store: &StateStore,
    session_id: &str,
) -> Result<serde_json::Value, AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;

    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;

    Ok(super::build_debug_session_doc(session, meta))
}

pub(crate) fn api_stop_session_with_options(
    store: &StateStore,
    session_id: &str,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<SessionRecord, AppError> {
    let session_id = session_id.to_string();
    let grace_timeout = Duration::from_millis(grace_timeout_ms);
    let expected_pid = load_session_pid(store, &session_id)?;
    if let Some(pid) = expected_pid {
        stop_process_with_options(pid, force, grace_timeout)?;
    }

    let (session_clone, post_task) = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        if session.pid != expected_pid {
            if session.pid.is_none() {
                if !matches!(session.status, SessionStatus::Stopped) {
                    session.status = SessionStatus::Stopped;
                    session.updated_at = now;
                }
                return Ok((session.clone(), None));
            }
            return Err(AppError::SessionStateChanged(session.id.clone()));
        }

        let post_task = if expected_pid.is_some() {
            build_post_stop_task(session)
        } else {
            None
        };
        session.pid = None;
        session.status = SessionStatus::Stopped;
        session.updated_at = now;
        Ok((session.clone(), post_task))
    })?;

    if let Some((task, cwd, env_map, log_path)) = post_task {
        run_shell_task(&task, Path::new(&cwd), &env_map, Path::new(&log_path))?;
    }

    Ok(session_clone)
}

pub(crate) fn api_restart_session_with_options(
    store: &StateStore,
    session_id: &str,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<SessionRecord, AppError> {
    let plan = load_restart_plan(store, session_id)?;
    let grace_timeout = Duration::from_millis(grace_timeout_ms);
    if let Some(pid) = plan.expected_pid {
        if is_process_alive(pid) {
            stop_process_with_options(pid, force, grace_timeout)?;
        }
    }

    let mut next_spec = plan.spec.clone();
    let debug_meta = super::prepare_debug_spec(&mut next_spec)?;
    super::run_prelaunch_task_if_any(&next_spec, &plan.log_path)?;
    let pid = super::spawn_session_worker(
        store,
        &plan.session_id,
        &next_spec,
        Some(plan.log_path.clone()),
    )?;
    let log_path = plan.log_path.to_string_lossy().to_string();

    let session = match store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &plan.session_id)?;
        if session.pid != plan.expected_pid {
            return Err(AppError::SessionStateChanged(session.id.clone()));
        }
        session.spec = next_spec.clone();
        session.pid = Some(pid);
        session.status = SessionStatus::Running;
        session.restart_count = session.restart_count.saturating_add(1);
        session.updated_at = now;
        session.debug_meta = debug_meta.clone();
        if session.log_path.is_none() {
            session.log_path = Some(log_path.clone());
        }
        Ok(session.clone())
    }) {
        Ok(value) => value,
        Err(err) => {
            let _ = stop_process_with_options(pid, true, Duration::from_millis(150));
            return Err(err);
        }
    };

    Ok(session)
}

fn build_post_stop_task(session: &SessionRecord) -> Option<ShellTaskSpec> {
    if let (Some(task), Some(log_path)) =
        (session.spec.poststop_task.clone(), session.log_path.clone())
    {
        return Some((
            task,
            session.spec.cwd.clone(),
            session.spec.env.clone(),
            log_path,
        ));
    }
    None
}

fn load_session_pid(store: &StateStore, session_id: &str) -> Result<Option<u32>, AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
    Ok(session.pid)
}

fn load_restart_plan(store: &StateStore, session_id: &str) -> Result<RestartPlan, AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
    let log_path = session
        .log_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| super::default_log_path(store, &session.id));
    Ok(RestartPlan {
        session_id: session.id.clone(),
        expected_pid: session.pid,
        spec: session.spec.clone(),
        log_path,
    })
}

pub(crate) fn api_suspend_session(
    store: &StateStore,
    session_id: &str,
) -> Result<SessionRecord, AppError> {
    let session_id = session_id.to_string();
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;
        suspend_process(pid)?;
        session.status = SessionStatus::Suspended;
        session.updated_at = now;
        Ok(session.clone())
    })
}

pub(crate) fn api_resume_session(
    store: &StateStore,
    session_id: &str,
) -> Result<SessionRecord, AppError> {
    let session_id = session_id.to_string();
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        let pid = session
            .pid
            .ok_or_else(|| AppError::SessionMissingPid(session.id.clone()))?;
        resume_process(pid)?;
        session.status = SessionStatus::Running;
        session.updated_at = now;
        Ok(session.clone())
    })
}
