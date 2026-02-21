use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::thread;
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
const SESSION_CONTROL_MAX_RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub(crate) struct SessionCleanupResult {
    pub dry_run: bool,
    pub matched_session_ids: Vec<String>,
    pub removed_session_ids: Vec<String>,
    pub kept_count: usize,
}

#[derive(Clone)]
struct RestartPlan {
    session_id: String,
    expected_pid: Option<u32>,
    spec: LaunchSpec,
    log_path: PathBuf,
}

pub(crate) fn api_list_sessions(store: &StateStore) -> Result<Vec<SessionRecord>, AppError> {
    let state = store.load()?;
    if state.sessions.is_empty() {
        return Ok(Vec::new());
    }
    let ids_to_reconcile = collect_reconcile_candidate_ids(&state.sessions);
    if ids_to_reconcile.is_empty() {
        return Ok(state.sessions.values().cloned().collect());
    }

    store.update::<_, _, AppError>(move |state| {
        let now = unix_timestamp_secs();
        for id in &ids_to_reconcile {
            let Some(session) = state.sessions.get_mut(id) else {
                continue;
            };
            super::reconcile_session(store, session, now)?;
        }

        Ok(state.sessions.values().cloned().collect())
    })
}

pub(crate) fn resolve_session_id(store: &StateStore, session_id: &str) -> Result<String, AppError> {
    let state = store.load()?;
    resolve_session_id_in_map(&state.sessions, session_id)
}

fn resolve_session_id_in_map(
    sessions: &BTreeMap<String, SessionRecord>,
    session_id: &str,
) -> Result<String, AppError> {
    if sessions.contains_key(session_id) {
        return Ok(session_id.to_string());
    }

    let mut matches: Vec<String> = sessions
        .keys()
        .filter(|candidate| candidate.starts_with(session_id))
        .cloned()
        .collect();
    if matches.is_empty() {
        return Err(AppError::SessionNotFound(session_id.to_string()));
    }
    if matches.len() == 1 {
        return Ok(matches.remove(0));
    }

    matches.sort();
    let preview_limit = 5usize;
    let preview = matches
        .iter()
        .take(preview_limit)
        .cloned()
        .collect::<Vec<String>>()
        .join(",");
    let extra_count = matches.len().saturating_sub(preview_limit);
    let suffix = if extra_count > 0 {
        format!(",+{extra_count}")
    } else {
        String::new()
    };

    Err(AppError::SessionIdAmbiguous(format!(
        "{session_id}; matches={preview}{suffix}"
    )))
}

fn session_requires_reconcile_for_list(session: &SessionRecord) -> bool {
    session.pid.is_some() || !matches!(session.status, SessionStatus::Stopped)
}

fn collect_reconcile_candidate_ids(sessions: &BTreeMap<String, SessionRecord>) -> Vec<String> {
    sessions
        .iter()
        .filter_map(|(id, session)| {
            if session_requires_reconcile_for_list(session) {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn api_cleanup_sessions(
    store: &StateStore,
    statuses: &[SessionStatus],
    dry_run: bool,
) -> Result<SessionCleanupResult, AppError> {
    api_cleanup_sessions_with_options(store, statuses, None, dry_run)
}

pub(crate) fn api_cleanup_sessions_with_options(
    store: &StateStore,
    statuses: &[SessionStatus],
    older_than_secs: Option<u64>,
    dry_run: bool,
) -> Result<SessionCleanupResult, AppError> {
    let statuses: Vec<SessionStatus> = statuses.to_vec();
    let preloaded = store.load()?;
    let ids_to_reconcile = collect_reconcile_candidate_ids(&preloaded.sessions);

    let result = store.update::<_, _, AppError>(move |state| {
        let now = unix_timestamp_secs();
        for id in &ids_to_reconcile {
            let Some(session) = state.sessions.get_mut(id) else {
                continue;
            };
            super::reconcile_session(store, session, now)?;
        }

        let mut matched_session_ids: Vec<String> = state
            .sessions
            .iter()
            .filter_map(|(id, session)| {
                let status_matches = statuses.iter().any(|value| value == &session.status);
                let age_matches = older_than_secs
                    .is_none_or(|secs| now.saturating_sub(session.updated_at) >= secs);
                if status_matches && age_matches {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        matched_session_ids.sort();

        let removed_session_ids = if dry_run {
            Vec::new()
        } else {
            for id in &matched_session_ids {
                state.sessions.remove(id);
            }
            matched_session_ids.clone()
        };

        Ok(SessionCleanupResult {
            dry_run,
            matched_session_ids,
            removed_session_ids,
            kept_count: state.sessions.len(),
        })
    })?;

    if !result.removed_session_ids.is_empty() {
        let _ = crate::session_lookup::remove_session_mappings(
            result.removed_session_ids.iter().cloned(),
        );
    }

    Ok(result)
}

pub(crate) fn api_get_session(
    store: &StateStore,
    session_id: &str,
) -> Result<SessionRecord, AppError> {
    let session_id = resolve_session_id(store, session_id)?;
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
    let session_id = resolve_session_id(store, session_id)?;
    let tail = tail.min(super::log_ops::MAX_LOG_TAIL_LINES);
    store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        super::reconcile_session(store, session, now)?;
        let session_snapshot = session.clone();
        let pid = session_snapshot.pid;
        let alive = pid.map(is_process_alive).unwrap_or(false);
        let command = build_command(&session_snapshot.spec)?;
        let log_tail = super::log_ops::read_log_tail(session_snapshot.log_path.as_deref(), tail)
            .unwrap_or_default();
        let parent_session_id = infer_parent_session_id(&session_snapshot, &state.sessions);
        let child_session_ids = collect_child_session_ids(&session_snapshot, &state.sessions);

        Ok(json!({
            "ok": true,
            "session": session_snapshot,
            "process": {
                "pid": pid,
                "alive": alive,
                "command": command,
            },
            "topology": {
                "parent_session_id": parent_session_id,
                "child_session_ids": child_session_ids,
            },
            "log": {
                "tail_lines": tail,
                "text": log_tail,
            }
        }))
    })
}

pub(super) fn collect_child_session_ids(
    session: &SessionRecord,
    sessions: &BTreeMap<String, SessionRecord>,
) -> Vec<String> {
    let mut children = Vec::new();
    let id_prefix = format!("{}-subprocess-", session.id);

    for (candidate_id, candidate) in sessions {
        if candidate_id == &session.id {
            continue;
        }

        let id_based_match = candidate_id.starts_with(&id_prefix);
        let pid_based_match = session.pid.is_some() && candidate.supervisor_pid == session.pid;
        if id_based_match || pid_based_match {
            children.push(candidate_id.clone());
        }
    }

    children.sort();
    children.dedup();
    children
}

pub(super) fn infer_parent_session_id(
    session: &SessionRecord,
    sessions: &BTreeMap<String, SessionRecord>,
) -> Option<String> {
    if let Some((candidate_parent_id, _)) = session.id.split_once("-subprocess-") {
        if sessions.contains_key(candidate_parent_id) {
            return Some(candidate_parent_id.to_string());
        }
    }

    let supervisor_pid = session.supervisor_pid?;
    let mut matched_ids = sessions.iter().filter_map(|(candidate_id, candidate)| {
        if candidate_id != &session.id && candidate.pid == Some(supervisor_pid) {
            Some(candidate_id.clone())
        } else {
            None
        }
    });
    let first = matched_ids.next()?;
    if matched_ids.next().is_some() {
        return None;
    }
    Some(first)
}

pub(crate) fn api_debug_session(
    store: &StateStore,
    session_id: &str,
) -> Result<serde_json::Value, AppError> {
    let session_id = resolve_session_id(store, session_id)?;
    let state = store.load()?;
    let session = state
        .sessions
        .get(&session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.clone()))?;

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
    let resolved_session_id = resolve_session_id(store, session_id)?;
    let grace_timeout = Duration::from_millis(grace_timeout_ms);
    retry_session_control(&resolved_session_id, || {
        api_stop_session_once(store, &resolved_session_id, force, grace_timeout)
    })
}

pub(crate) fn api_restart_session_with_options(
    store: &StateStore,
    session_id: &str,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<SessionRecord, AppError> {
    let resolved_session_id = resolve_session_id(store, session_id)?;
    let grace_timeout = Duration::from_millis(grace_timeout_ms);
    retry_session_control(&resolved_session_id, || {
        api_restart_session_once(store, &resolved_session_id, force, grace_timeout)
    })
}

fn api_stop_session_once(
    store: &StateStore,
    session_id: &str,
    force: bool,
    grace_timeout: Duration,
) -> Result<SessionRecord, AppError> {
    let expected_pid = load_session_pid(store, session_id)?;
    if let Some(pid) = expected_pid {
        stop_process_with_options(pid, force, grace_timeout)?;
    }

    let (session_clone, post_task) = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, session_id)?;
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

fn api_restart_session_once(
    store: &StateStore,
    session_id: &str,
    force: bool,
    grace_timeout: Duration,
) -> Result<SessionRecord, AppError> {
    let plan = load_restart_plan(store, session_id)?;
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

fn retry_backoff(attempt: usize) -> Duration {
    let step = u64::try_from(attempt).unwrap_or(u64::MAX).saturating_add(1);
    Duration::from_millis(step.saturating_mul(10))
}

fn retry_session_control<T, F>(session_id: &str, mut operation: F) -> Result<T, AppError>
where
    F: FnMut() -> Result<T, AppError>,
{
    for attempt in 0..SESSION_CONTROL_MAX_RETRIES {
        match operation() {
            Ok(value) => return Ok(value),
            Err(AppError::SessionStateChanged(_)) => {
                if attempt + 1 < SESSION_CONTROL_MAX_RETRIES {
                    thread::sleep(retry_backoff(attempt));
                    continue;
                }
                return Err(AppError::SessionStateChanged(session_id.to_string()));
            }
            Err(err) => return Err(err),
        }
    }
    Err(AppError::SessionStateChanged(session_id.to_string()))
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
    let session_id = resolve_session_id(store, session_id)?;
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
    let session_id = resolve_session_id(store, session_id)?;
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn retry_session_control_recovers_after_transient_conflicts() {
        let attempts = AtomicUsize::new(0);
        let value = retry_session_control("session-a", || {
            let observed = attempts.fetch_add(1, Ordering::SeqCst);
            if observed < 2 {
                return Err(AppError::SessionStateChanged("session-a".to_string()));
            }
            Ok(7u64)
        })
        .expect("retry should recover");

        assert_eq!(value, 7);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retry_session_control_returns_conflict_after_retry_budget() {
        let attempts = AtomicUsize::new(0);
        let err = retry_session_control("session-b", || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<u64, AppError>(AppError::SessionStateChanged("other-session".to_string()))
        })
        .expect_err("retry should eventually fail");

        match err {
            AppError::SessionStateChanged(session_id) => {
                assert_eq!(session_id, "session-b");
            }
            other => panic!("unexpected error variant: {other}"),
        }
        assert_eq!(attempts.load(Ordering::SeqCst), SESSION_CONTROL_MAX_RETRIES);
    }
}
