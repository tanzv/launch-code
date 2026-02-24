use launch_code::model::RuntimeKind;
use launch_code::state::StateStore;

use crate::cli::{
    BatchFilterArgs, BatchSortArg, ListStatusArg, RestartArgs, ResumeArgs, StopArgs, SuspendArgs,
};
use crate::error::AppError;
use crate::output;

use super::ListFilters;

mod engine;
mod render;

#[derive(Debug, Clone, Copy)]
enum BatchAction {
    Stop,
    Restart,
    Suspend,
    Resume,
}

#[derive(Debug, Clone)]
struct BatchSelector {
    status: Option<ListStatusArg>,
    runtime: Option<RuntimeKind>,
    name_filter: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Clone, Copy)]
struct BatchExecutionControl {
    continue_on_error: bool,
    max_failures: usize,
}

#[derive(Debug, Clone, Copy)]
struct BatchExecutionPlan {
    sort: BatchSortArg,
    limit: Option<usize>,
    summary: bool,
    jobs: usize,
}

#[derive(Debug, Clone)]
struct BatchSessionRow {
    id: String,
    runtime: &'static str,
    status_before: &'static str,
    status_after: Option<&'static str>,
    link_name: Option<String>,
    link_path: Option<String>,
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct BatchLinkErrorRow {
    link_name: Option<String>,
    link_path: Option<String>,
    error: String,
}

#[derive(Debug, Clone)]
struct BatchExecutionResult {
    rows: Vec<BatchSessionRow>,
    selected_count: usize,
    processed_count: usize,
    link_errors: Vec<BatchLinkErrorRow>,
    stopped_early: bool,
}

impl BatchSelector {
    fn filters(&self) -> ListFilters {
        ListFilters {
            status_filter: self.status.clone(),
            runtime_filter: self.runtime.clone(),
            name_filter: self.name_filter.clone(),
        }
    }
}

impl BatchExecutionControl {
    fn should_stop_after_failure(&self, failure_count: usize) -> bool {
        if !self.continue_on_error {
            return true;
        }
        self.max_failures > 0 && failure_count >= self.max_failures
    }
}

pub(super) fn handle_stop(store: &StateStore, args: &StopArgs) -> Result<(), AppError> {
    if args.targets_all() {
        let selector = selector_from_stop_args(args);
        let control = control_from_stop_args(args);
        let plan = plan_from_stop_args(args);
        return handle_batch_control_local(
            store,
            BatchAction::Stop,
            selector,
            control,
            plan,
            args.force,
            args.grace_timeout_ms,
        );
    }
    if args.is_multi_target() {
        if args.has_batch_filters() {
            return Err(AppError::InvalidStartOptions(
                "batch flags require `--all` or `all` session target".to_string(),
            ));
        }
        return handle_multi_target_control_local(
            store,
            BatchAction::Stop,
            args.target_ids(),
            args.force,
            args.grace_timeout_ms,
        );
    }
    if args.has_batch_filters() {
        return Err(AppError::InvalidStartOptions(
            "batch flags require `--all` or `all` session target".to_string(),
        ));
    }
    let Some(session_id) = args.single_target_id() else {
        return Ok(());
    };
    let session = super::super::api_stop_session_with_options(
        store,
        session_id,
        args.force,
        args.grace_timeout_ms,
    )?;
    let output = format!("session_id={} status=stopped", session.id);
    super::print_session_command_output("stop", &session, output);
    Ok(())
}

pub(super) fn handle_restart(store: &StateStore, args: &RestartArgs) -> Result<(), AppError> {
    if args.targets_all() {
        let selector = selector_from_restart_args(args);
        let control = control_from_restart_args(args);
        let plan = plan_from_restart_args(args);
        let force = args.force && !args.no_force;
        return handle_batch_control_local(
            store,
            BatchAction::Restart,
            selector,
            control,
            plan,
            force,
            args.grace_timeout_ms,
        );
    }
    if args.is_multi_target() {
        if args.has_batch_filters() {
            return Err(AppError::InvalidStartOptions(
                "batch flags require `--all` or `all` session target".to_string(),
            ));
        }
        let force = args.force && !args.no_force;
        return handle_multi_target_control_local(
            store,
            BatchAction::Restart,
            args.target_ids(),
            force,
            args.grace_timeout_ms,
        );
    }
    if args.has_batch_filters() {
        return Err(AppError::InvalidStartOptions(
            "batch flags require `--all` or `all` session target".to_string(),
        ));
    }
    let Some(session_id) = args.single_target_id() else {
        return Ok(());
    };
    let force = args.force && !args.no_force;
    let session = super::super::api_restart_session_with_options(
        store,
        session_id,
        force,
        args.grace_timeout_ms,
    )?;
    let output = super::format_status_like_message(&session);
    super::print_session_command_output("restart", &session, output);
    Ok(())
}

pub(super) fn handle_suspend(store: &StateStore, args: &SuspendArgs) -> Result<(), AppError> {
    if args.targets_all() {
        let selector = selector_from_suspend_args(args);
        let control = control_from_suspend_args(args);
        let plan = plan_from_suspend_args(args);
        return handle_batch_control_local(
            store,
            BatchAction::Suspend,
            selector,
            control,
            plan,
            false,
            0,
        );
    }
    if args.is_multi_target() {
        if args.has_batch_filters() {
            return Err(AppError::InvalidStartOptions(
                "batch flags require `--all` or `all` session target".to_string(),
            ));
        }
        return handle_multi_target_control_local(
            store,
            BatchAction::Suspend,
            args.target_ids(),
            false,
            0,
        );
    }
    if args.has_batch_filters() {
        return Err(AppError::InvalidStartOptions(
            "batch flags require `--all` or `all` session target".to_string(),
        ));
    }
    let Some(session_id) = args.single_target_id() else {
        return Ok(());
    };
    let session = super::super::api_suspend_session(store, session_id)?;
    let output = format!("session_id={} status=suspended", session.id);
    super::print_session_command_output("suspend", &session, output);
    Ok(())
}

pub(super) fn handle_resume(store: &StateStore, args: &ResumeArgs) -> Result<(), AppError> {
    if args.targets_all() {
        let selector = selector_from_resume_args(args);
        let control = control_from_resume_args(args);
        let plan = plan_from_resume_args(args);
        return handle_batch_control_local(
            store,
            BatchAction::Resume,
            selector,
            control,
            plan,
            false,
            0,
        );
    }
    if args.is_multi_target() {
        if args.has_batch_filters() {
            return Err(AppError::InvalidStartOptions(
                "batch flags require `--all` or `all` session target".to_string(),
            ));
        }
        return handle_multi_target_control_local(
            store,
            BatchAction::Resume,
            args.target_ids(),
            false,
            0,
        );
    }
    if args.has_batch_filters() {
        return Err(AppError::InvalidStartOptions(
            "batch flags require `--all` or `all` session target".to_string(),
        ));
    }
    let Some(session_id) = args.single_target_id() else {
        return Ok(());
    };
    let session = super::super::api_resume_session(store, session_id)?;
    let output = format!("session_id={} status=running", session.id);
    super::print_session_command_output("resume", &session, output);
    Ok(())
}

pub(super) fn handle_stop_global_default(args: &StopArgs) -> Result<(), AppError> {
    handle_batch_control_global_stop(args)
}

pub(super) fn handle_restart_global_default(args: &RestartArgs) -> Result<(), AppError> {
    handle_batch_control_global_restart(args)
}

pub(super) fn handle_suspend_global_default(args: &SuspendArgs) -> Result<(), AppError> {
    handle_batch_control_global_suspend(args)
}

pub(super) fn handle_resume_global_default(args: &ResumeArgs) -> Result<(), AppError> {
    handle_batch_control_global_resume(args)
}

fn selector_from_stop_args(args: &StopArgs) -> BatchSelector {
    selector_from_batch(&args.batch)
}

fn selector_from_restart_args(args: &RestartArgs) -> BatchSelector {
    selector_from_batch(&args.batch)
}

fn selector_from_suspend_args(args: &SuspendArgs) -> BatchSelector {
    selector_from_batch(&args.batch)
}

fn selector_from_resume_args(args: &ResumeArgs) -> BatchSelector {
    selector_from_batch(&args.batch)
}

fn selector_from_batch(batch: &BatchFilterArgs) -> BatchSelector {
    BatchSelector {
        status: batch.status.clone(),
        runtime: batch
            .runtime
            .as_ref()
            .map(super::super::spec_ops::to_runtime_kind),
        name_filter: batch
            .name_contains
            .as_ref()
            .map(|value| value.to_lowercase()),
        dry_run: batch.dry_run,
    }
}

fn control_from_stop_args(args: &StopArgs) -> BatchExecutionControl {
    control_from_batch(&args.batch)
}

fn control_from_restart_args(args: &RestartArgs) -> BatchExecutionControl {
    control_from_batch(&args.batch)
}

fn control_from_suspend_args(args: &SuspendArgs) -> BatchExecutionControl {
    control_from_batch(&args.batch)
}

fn control_from_resume_args(args: &ResumeArgs) -> BatchExecutionControl {
    control_from_batch(&args.batch)
}

fn control_from_batch(batch: &BatchFilterArgs) -> BatchExecutionControl {
    BatchExecutionControl {
        continue_on_error: batch.continue_on_error,
        max_failures: batch.max_failures,
    }
}

fn plan_from_stop_args(args: &StopArgs) -> BatchExecutionPlan {
    plan_from_batch(&args.batch)
}

fn plan_from_restart_args(args: &RestartArgs) -> BatchExecutionPlan {
    plan_from_batch(&args.batch)
}

fn plan_from_suspend_args(args: &SuspendArgs) -> BatchExecutionPlan {
    plan_from_batch(&args.batch)
}

fn plan_from_resume_args(args: &ResumeArgs) -> BatchExecutionPlan {
    plan_from_batch(&args.batch)
}

fn plan_from_batch(batch: &BatchFilterArgs) -> BatchExecutionPlan {
    BatchExecutionPlan {
        sort: batch.sort.unwrap_or(BatchSortArg::Id),
        limit: batch.limit,
        summary: batch.summary,
        jobs: batch.jobs,
    }
}

fn validate_batch_jobs_with_control(
    plan: BatchExecutionPlan,
    control: BatchExecutionControl,
) -> Result<(), AppError> {
    if plan.jobs <= 1 {
        return Ok(());
    }
    if !control.continue_on_error || control.max_failures > 0 {
        return Err(AppError::InvalidStartOptions(
            "`--jobs > 1` requires `--continue-on-error true` and `--max-failures 0`".to_string(),
        ));
    }
    Ok(())
}

fn handle_batch_control_local(
    store: &StateStore,
    action: BatchAction,
    selector: BatchSelector,
    control: BatchExecutionControl,
    plan: BatchExecutionPlan,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<(), AppError> {
    validate_batch_jobs_with_control(plan, control)?;
    let filters = selector.filters();
    let mut failure_count = 0usize;
    let result = engine::execute_batch_control_for_store(
        store,
        action,
        &filters,
        selector.dry_run,
        control,
        plan,
        &mut failure_count,
        force,
        grace_timeout_ms,
        None,
        None,
    )?;
    render::print_batch_control_result(action, "local", selector.dry_run, control, plan, result);
    Ok(())
}

fn handle_multi_target_control_local(
    store: &StateStore,
    action: BatchAction,
    target_ids: Vec<&str>,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<(), AppError> {
    let mut rows = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        match apply_multi_target_action_with_fallback(
            store,
            action,
            target_id,
            force,
            grace_timeout_ms,
        ) {
            Ok(updated) => rows.push(BatchSessionRow {
                id: updated.id,
                runtime: super::super::spec_ops::runtime_label(&updated.spec.runtime),
                status_before: "unknown",
                status_after: Some(super::status_label(&updated.status)),
                link_name: None,
                link_path: None,
                ok: true,
                error: None,
            }),
            Err(err) => rows.push(BatchSessionRow {
                id: target_id.to_string(),
                runtime: "unknown",
                status_before: "unknown",
                status_after: None,
                link_name: None,
                link_path: None,
                ok: false,
                error: Some(err.to_string()),
            }),
        }
    }

    render::print_multi_target_control_result(action, rows);
    Ok(())
}

fn apply_multi_target_action_with_fallback(
    store: &StateStore,
    action: BatchAction,
    target_id: &str,
    force: bool,
    grace_timeout_ms: u64,
) -> Result<launch_code::model::SessionRecord, AppError> {
    match engine::apply_batch_action(store, action, target_id, force, grace_timeout_ms) {
        Ok(updated) => Ok(updated),
        Err(AppError::SessionNotFound(missing_id))
            if output::is_global_session_fallback_mode() && missing_id == target_id =>
        {
            let Some(routed_store) =
                super::super::resolve_global_store_for_session_id(target_id, store)?
            else {
                return Err(AppError::SessionNotFound(missing_id));
            };
            engine::apply_batch_action(&routed_store, action, target_id, force, grace_timeout_ms)
        }
        Err(err) => Err(err),
    }
}

fn handle_batch_control_global_stop(args: &StopArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("stop", args.batch.dry_run, args.batch.yes)?;
    let _ = super::super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_stop_args(args);
    let control = control_from_stop_args(args);
    let plan = plan_from_stop_args(args);
    validate_batch_jobs_with_control(plan, control)?;
    let filters = selector.filters();
    let result = engine::execute_batch_control_global(
        BatchAction::Stop,
        &filters,
        selector.dry_run,
        control,
        plan,
        args.force,
        args.grace_timeout_ms,
    )?;
    render::print_batch_control_result(
        BatchAction::Stop,
        "global",
        selector.dry_run,
        control,
        plan,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_restart(args: &RestartArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("restart", args.batch.dry_run, args.batch.yes)?;
    let _ = super::super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_restart_args(args);
    let control = control_from_restart_args(args);
    let plan = plan_from_restart_args(args);
    validate_batch_jobs_with_control(plan, control)?;
    let filters = selector.filters();
    let force = args.force && !args.no_force;
    let result = engine::execute_batch_control_global(
        BatchAction::Restart,
        &filters,
        selector.dry_run,
        control,
        plan,
        force,
        args.grace_timeout_ms,
    )?;
    render::print_batch_control_result(
        BatchAction::Restart,
        "global",
        selector.dry_run,
        control,
        plan,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_suspend(args: &SuspendArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("suspend", args.batch.dry_run, args.batch.yes)?;
    let _ = super::super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_suspend_args(args);
    let control = control_from_suspend_args(args);
    let plan = plan_from_suspend_args(args);
    validate_batch_jobs_with_control(plan, control)?;
    let filters = selector.filters();
    let result = engine::execute_batch_control_global(
        BatchAction::Suspend,
        &filters,
        selector.dry_run,
        control,
        plan,
        false,
        0,
    )?;
    render::print_batch_control_result(
        BatchAction::Suspend,
        "global",
        selector.dry_run,
        control,
        plan,
        result,
    );
    Ok(())
}

fn handle_batch_control_global_resume(args: &ResumeArgs) -> Result<(), AppError> {
    ensure_global_batch_apply_confirmation("resume", args.batch.dry_run, args.batch.yes)?;
    let _ = super::super::link_ops::auto_prune_stale_links_for_global_scan();
    let selector = selector_from_resume_args(args);
    let control = control_from_resume_args(args);
    let plan = plan_from_resume_args(args);
    validate_batch_jobs_with_control(plan, control)?;
    let filters = selector.filters();
    let result = engine::execute_batch_control_global(
        BatchAction::Resume,
        &filters,
        selector.dry_run,
        control,
        plan,
        false,
        0,
    )?;
    render::print_batch_control_result(
        BatchAction::Resume,
        "global",
        selector.dry_run,
        control,
        plan,
        result,
    );
    Ok(())
}

fn ensure_global_batch_apply_confirmation(
    action_label: &str,
    dry_run: bool,
    confirmed: bool,
) -> Result<(), AppError> {
    if dry_run || confirmed {
        return Ok(());
    }
    Err(AppError::ConfirmationRequired(format!(
        "global `{action_label} --all` requires `--yes`; use `--dry-run` to preview."
    )))
}
