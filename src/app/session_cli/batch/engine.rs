use std::collections::BTreeSet;

use launch_code::model::{SessionRecord, SessionStatus};
use launch_code::state::StateStore;

use crate::cli::BatchSortArg;
use crate::error::AppError;
use crate::link_registry::load_registry;

use super::{
    BatchAction, BatchExecutionControl, BatchExecutionPlan, BatchExecutionResult,
    BatchLinkErrorRow, BatchSessionRow, ListFilters,
};

pub(super) fn execute_batch_control_global(
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<BatchExecutionResult, AppError> {
    let registry = load_registry()?;
    let mut seen_paths = BTreeSet::new();
    let links: Vec<(String, String)> = registry
        .list()
        .into_iter()
        .filter(|item| seen_paths.insert(item.path.clone()))
        .map(|item| (item.name, item.path))
        .collect();

    if links.is_empty() {
        return Ok(BatchExecutionResult {
            rows: Vec::new(),
            selected_count: 0,
            processed_count: 0,
            link_errors: Vec::new(),
            stopped_early: false,
        });
    }

    if plan.jobs <= 1 {
        return execute_batch_control_global_sequential(
            action,
            filters,
            dry_run,
            control,
            plan,
            force,
            grace_timeout_ms,
            links,
        );
    }

    execute_batch_control_global_parallel(
        action,
        filters,
        dry_run,
        control,
        plan,
        force,
        grace_timeout_ms,
        links,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_batch_control_global_sequential(
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
    force: bool,
    grace_timeout_ms: u64,
    links: Vec<(String, String)>,
) -> Result<BatchExecutionResult, AppError> {
    let mut failure_count = 0usize;
    let mut rows = Vec::new();
    let mut link_errors = Vec::new();
    let mut selected_count = 0usize;
    let mut stopped_early = false;

    for (link_name, link_path) in links {
        let store = StateStore::new(&link_path);

        match execute_batch_control_for_store(
            &store,
            action,
            filters,
            dry_run,
            control,
            plan,
            &mut failure_count,
            force,
            grace_timeout_ms,
            Some(link_name.clone()),
            Some(link_path.clone()),
        ) {
            Ok(mut scoped) => {
                selected_count = selected_count.saturating_add(scoped.selected_count);
                rows.append(&mut scoped.rows);
                if scoped.stopped_early {
                    stopped_early = true;
                    break;
                }
            }
            Err(err) => {
                failure_count = failure_count.saturating_add(1);
                link_errors.push(BatchLinkErrorRow {
                    link_name: Some(link_name),
                    link_path: Some(link_path),
                    error: err.to_string(),
                });
                if control.should_stop_after_failure(failure_count) {
                    stopped_early = true;
                    break;
                }
            }
        }
    }

    finalize_batch_result(rows, selected_count, link_errors, stopped_early)
}

#[allow(clippy::too_many_arguments)]
fn execute_batch_control_global_parallel(
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
    force: bool,
    grace_timeout_ms: u64,
    links: Vec<(String, String)>,
) -> Result<BatchExecutionResult, AppError> {
    let workers = plan.jobs.min(links.len()).max(1);
    let mut buckets: Vec<Vec<(String, String)>> = vec![Vec::new(); workers];
    for (index, item) in links.into_iter().enumerate() {
        buckets[index % workers].push(item);
    }

    let mut scoped_results = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for bucket in buckets {
            if bucket.is_empty() {
                continue;
            }
            let filters = filters.clone();
            handles.push(scope.spawn(move || {
                let mut local_rows = Vec::new();
                let mut local_selected = 0usize;
                let mut local_errors = Vec::new();
                let mut local_failure_count = 0usize;
                for (link_name, link_path) in bucket {
                    let store = StateStore::new(&link_path);
                    match execute_batch_control_for_store(
                        &store,
                        action,
                        &filters,
                        dry_run,
                        control,
                        plan,
                        &mut local_failure_count,
                        force,
                        grace_timeout_ms,
                        Some(link_name.clone()),
                        Some(link_path.clone()),
                    ) {
                        Ok(mut scoped) => {
                            local_selected = local_selected.saturating_add(scoped.selected_count);
                            local_rows.append(&mut scoped.rows);
                        }
                        Err(err) => {
                            local_errors.push(BatchLinkErrorRow {
                                link_name: Some(link_name),
                                link_path: Some(link_path),
                                error: err.to_string(),
                            });
                        }
                    }
                }
                (local_rows, local_selected, local_errors)
            }));
        }
        for handle in handles {
            if let Ok(value) = handle.join() {
                scoped_results.push(value);
            }
        }
    });

    let mut rows = Vec::new();
    let mut selected_count = 0usize;
    let mut link_errors = Vec::new();
    for (mut scoped_rows, scoped_selected, mut scoped_errors) in scoped_results {
        rows.append(&mut scoped_rows);
        selected_count = selected_count.saturating_add(scoped_selected);
        link_errors.append(&mut scoped_errors);
    }

    finalize_batch_result(rows, selected_count, link_errors, false)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_batch_control_for_store(
    store: &StateStore,
    action: BatchAction,
    filters: &ListFilters,
    dry_run: bool,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
    failure_count: &mut usize,
    force: bool,
    grace_timeout_ms: u64,
    link_name: Option<String>,
    link_path: Option<String>,
) -> Result<BatchExecutionResult, AppError> {
    let sessions = super::super::super::api_list_sessions(store)?;
    let mut matched_sessions: Vec<SessionRecord> = sessions
        .into_iter()
        .filter(|session| matches_batch_control_filters(action, filters, session))
        .collect();
    sort_batch_sessions(&mut matched_sessions, plan.sort);
    if let Some(limit) = plan.limit {
        matched_sessions.truncate(limit);
    }

    let selected_count = matched_sessions.len();
    if selected_count == 0 {
        return Ok(BatchExecutionResult {
            rows: Vec::new(),
            selected_count: 0,
            processed_count: 0,
            link_errors: Vec::new(),
            stopped_early: false,
        });
    }

    if dry_run {
        let rows = matched_sessions
            .into_iter()
            .map(|session| {
                let status_before = super::super::status_label(&session.status);
                BatchSessionRow {
                    id: session.id,
                    runtime: super::super::super::spec_ops::runtime_label(&session.spec.runtime),
                    status_before,
                    status_after: Some(status_before),
                    link_name: link_name.clone(),
                    link_path: link_path.clone(),
                    ok: true,
                    error: None,
                }
            })
            .collect::<Vec<_>>();
        return Ok(BatchExecutionResult {
            processed_count: rows.len(),
            rows,
            selected_count,
            link_errors: Vec::new(),
            stopped_early: false,
        });
    }

    if plan.jobs > 1 {
        let workers = plan.jobs.min(selected_count).max(1);
        let mut buckets: Vec<Vec<(usize, SessionRecord)>> = vec![Vec::new(); workers];
        for (index, session) in matched_sessions.into_iter().enumerate() {
            buckets[index % workers].push((index, session));
        }

        let mut rows_by_index: Vec<Option<BatchSessionRow>> = vec![None; selected_count];
        let mut local_failures = 0usize;
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for bucket in buckets {
                if bucket.is_empty() {
                    continue;
                }
                let store = store.clone();
                let link_name = link_name.clone();
                let link_path = link_path.clone();
                handles.push(scope.spawn(move || {
                    let mut produced = Vec::new();
                    let mut failures = 0usize;
                    for (index, session) in bucket {
                        let status_before = super::super::status_label(&session.status);
                        let runtime =
                            super::super::super::spec_ops::runtime_label(&session.spec.runtime);
                        let result = apply_batch_action(
                            &store,
                            action,
                            &session.id,
                            force,
                            grace_timeout_ms,
                        );
                        match result {
                            Ok(updated) => produced.push((
                                index,
                                BatchSessionRow {
                                    id: updated.id,
                                    runtime,
                                    status_before,
                                    status_after: Some(super::super::status_label(&updated.status)),
                                    link_name: link_name.clone(),
                                    link_path: link_path.clone(),
                                    ok: true,
                                    error: None,
                                },
                            )),
                            Err(err) => {
                                failures = failures.saturating_add(1);
                                produced.push((
                                    index,
                                    BatchSessionRow {
                                        id: session.id,
                                        runtime,
                                        status_before,
                                        status_after: None,
                                        link_name: link_name.clone(),
                                        link_path: link_path.clone(),
                                        ok: false,
                                        error: Some(err.to_string()),
                                    },
                                ));
                            }
                        }
                    }
                    (produced, failures)
                }));
            }

            for handle in handles {
                if let Ok((produced, failures)) = handle.join() {
                    local_failures = local_failures.saturating_add(failures);
                    for (index, row) in produced {
                        if let Some(slot) = rows_by_index.get_mut(index) {
                            *slot = Some(row);
                        }
                    }
                }
            }
        });

        *failure_count = failure_count.saturating_add(local_failures);
        let rows = rows_by_index
            .into_iter()
            .flatten()
            .collect::<Vec<BatchSessionRow>>();
        return Ok(BatchExecutionResult {
            processed_count: rows.len(),
            rows,
            selected_count,
            link_errors: Vec::new(),
            stopped_early: false,
        });
    }

    let mut rows = Vec::with_capacity(selected_count);
    for session in matched_sessions {
        let status_before = super::super::status_label(&session.status);
        let runtime = super::super::super::spec_ops::runtime_label(&session.spec.runtime);
        let result = apply_batch_action(store, action, &session.id, force, grace_timeout_ms);

        match result {
            Ok(updated) => rows.push(BatchSessionRow {
                id: updated.id,
                runtime,
                status_before,
                status_after: Some(super::super::status_label(&updated.status)),
                link_name: link_name.clone(),
                link_path: link_path.clone(),
                ok: true,
                error: None,
            }),
            Err(err) => {
                rows.push(BatchSessionRow {
                    id: session.id,
                    runtime,
                    status_before,
                    status_after: None,
                    link_name: link_name.clone(),
                    link_path: link_path.clone(),
                    ok: false,
                    error: Some(err.to_string()),
                });
                *failure_count = failure_count.saturating_add(1);
                if control.should_stop_after_failure(*failure_count) {
                    break;
                }
            }
        }
    }

    let stopped_early = rows.len() < selected_count;
    Ok(BatchExecutionResult {
        processed_count: rows.len(),
        rows,
        selected_count,
        link_errors: Vec::new(),
        stopped_early,
    })
}

pub(super) fn apply_batch_action(
    store: &StateStore,
    action: BatchAction,
    session_id: &str,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<SessionRecord, AppError> {
    match action {
        BatchAction::Stop => super::super::super::api_stop_session_with_options(
            store,
            session_id,
            force,
            grace_timeout_ms,
        ),
        BatchAction::Restart => super::super::super::api_restart_session_with_options(
            store,
            session_id,
            force,
            grace_timeout_ms,
        ),
        BatchAction::Suspend => super::super::super::api_suspend_session(store, session_id),
        BatchAction::Resume => super::super::super::api_resume_session(store, session_id),
    }
}

fn sort_batch_sessions(sessions: &mut [SessionRecord], sort: BatchSortArg) {
    sessions.sort_by(|left, right| {
        let ordering = match sort {
            BatchSortArg::Id => left.id.cmp(&right.id),
            BatchSortArg::Name => left.spec.name.cmp(&right.spec.name),
            BatchSortArg::Status => status_sort_rank(&left.status)
                .cmp(&status_sort_rank(&right.status))
                .then_with(|| left.id.cmp(&right.id)),
            BatchSortArg::Runtime => {
                super::super::super::spec_ops::runtime_label(&left.spec.runtime)
                    .cmp(super::super::super::spec_ops::runtime_label(
                        &right.spec.runtime,
                    ))
                    .then_with(|| left.id.cmp(&right.id))
            }
        };
        if ordering.is_eq() {
            left.id.cmp(&right.id)
        } else {
            ordering
        }
    });
}

fn status_sort_rank(status: &SessionStatus) -> u8 {
    match status {
        SessionStatus::Running => 0,
        SessionStatus::Suspended => 1,
        SessionStatus::Stopped => 2,
        SessionStatus::Unknown => 3,
    }
}

fn finalize_batch_result(
    mut rows: Vec<BatchSessionRow>,
    selected_count: usize,
    mut link_errors: Vec<BatchLinkErrorRow>,
    stopped_early: bool,
) -> Result<BatchExecutionResult, AppError> {
    rows.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.link_name.cmp(&right.link_name))
    });
    link_errors.sort_by(|left, right| {
        left.link_name
            .cmp(&right.link_name)
            .then_with(|| left.link_path.cmp(&right.link_path))
    });

    Ok(BatchExecutionResult {
        processed_count: rows.len(),
        rows,
        selected_count,
        link_errors,
        stopped_early,
    })
}

fn matches_batch_control_filters(
    action: BatchAction,
    filters: &ListFilters,
    session: &SessionRecord,
) -> bool {
    let status_match = if let Some(status_filter) = &filters.status_filter {
        super::super::matches_list_status(status_filter, &session.status)
    } else {
        default_batch_status_match(action, &session.status)
    };
    if !status_match {
        return false;
    }
    if let Some(runtime_filter) = &filters.runtime_filter {
        if session.spec.runtime != *runtime_filter {
            return false;
        }
    }
    if let Some(name_filter) = &filters.name_filter {
        if !session.spec.name.to_lowercase().contains(name_filter) {
            return false;
        }
    }
    true
}

fn default_batch_status_match(action: BatchAction, status: &SessionStatus) -> bool {
    match action {
        BatchAction::Stop => !matches!(status, SessionStatus::Stopped),
        BatchAction::Restart => matches!(status, SessionStatus::Running | SessionStatus::Suspended),
        BatchAction::Suspend => matches!(status, SessionStatus::Running),
        BatchAction::Resume => matches!(status, SessionStatus::Suspended),
    }
}
