use clap::{Args, ValueEnum};

use super::{ListStatusArg, RuntimeArg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BatchSortArg {
    Id,
    Name,
    Status,
    Runtime,
}

#[derive(Debug, Clone, Args)]
pub struct SessionTargetArgs {
    #[arg(
        long,
        required_unless_present_any = ["all", "session_ids"],
        conflicts_with_all = ["all", "session_ids"],
        help = "Target session id. Use `all` to target all sessions."
    )]
    pub id: Option<String>,
    #[arg(
        value_name = "ID",
        index = 1,
        num_args = 1..,
        required_unless_present_any = ["all", "id"],
        conflicts_with_all = ["all", "id"],
        help = "Target session ids (positional shorthand). Repeat values to target multiple ids. Use `all` to target all sessions."
    )]
    pub session_ids: Vec<String>,
    #[arg(
        long,
        default_value_t = false,
        required_unless_present_any = ["id", "session_ids"],
        help = "Apply operation to all matched sessions in scope."
    )]
    pub all: bool,
}

#[derive(Debug, Clone, Args)]
pub struct BatchFilterArgs {
    #[arg(
        long,
        value_enum,
        help = "Filter matched sessions by reconciled status."
    )]
    pub status: Option<ListStatusArg>,
    #[arg(long, value_enum, help = "Filter matched sessions by runtime kind.")]
    pub runtime: Option<RuntimeArg>,
    #[arg(long, help = "Case-insensitive substring filter on session name.")]
    pub name_contains: Option<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Preview matched sessions without applying operation."
    )]
    pub dry_run: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Confirm global non-dry-run batch operation (required in global scope)."
    )]
    pub yes: bool,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = "Continue processing matched sessions after individual failures (set false for fail-fast)."
    )]
    pub continue_on_error: bool,
    #[arg(
        long,
        default_value_t = 0,
        help = "Maximum allowed failures before stopping batch apply; 0 means unlimited."
    )]
    pub max_failures: usize,
    #[arg(
        long,
        value_enum,
        help = "Sort matched sessions before batch execution."
    )]
    pub sort: Option<BatchSortArg>,
    #[arg(
        long,
        value_name = "N",
        help = "Limit matched sessions before batch execution."
    )]
    pub limit: Option<usize>,
    #[arg(
        long,
        default_value_t = false,
        help = "Print grouped summary for batch execution results."
    )]
    pub summary: bool,
    #[arg(
        long,
        default_value_t = 1,
        value_parser = super::parse_positive_usize,
        help = "Maximum parallel jobs for batch execution."
    )]
    pub jobs: usize,
}

#[derive(Debug, Clone, Args)]
pub struct StopArgs {
    #[command(flatten)]
    pub target: SessionTargetArgs,
    #[command(flatten)]
    pub batch: BatchFilterArgs,
    #[arg(
        long,
        default_value_t = false,
        help = "Force kill if process is still alive after grace timeout."
    )]
    pub force: bool,
    #[arg(
        long,
        default_value_t = 1500,
        help = "Graceful stop timeout in milliseconds before force kill."
    )]
    pub grace_timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct RestartArgs {
    #[command(flatten)]
    pub target: SessionTargetArgs,
    #[command(flatten)]
    pub batch: BatchFilterArgs,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = "Force kill if process is still alive after grace timeout."
    )]
    pub force: bool,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "force",
        help = "Shortcut to disable force kill fallback (equivalent to --force false)."
    )]
    pub no_force: bool,
    #[arg(
        long,
        default_value_t = 150,
        help = "Graceful stop timeout in milliseconds before optional force kill."
    )]
    pub grace_timeout_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct SuspendArgs {
    #[command(flatten)]
    pub target: SessionTargetArgs,
    #[command(flatten)]
    pub batch: BatchFilterArgs,
}

#[derive(Debug, Clone, Args)]
pub struct ResumeArgs {
    #[command(flatten)]
    pub target: SessionTargetArgs,
    #[command(flatten)]
    pub batch: BatchFilterArgs,
}

fn is_all_target_keyword(value: Option<&str>) -> bool {
    value.is_some_and(|candidate| candidate.eq_ignore_ascii_case("all"))
}

impl SessionTargetArgs {
    pub fn targets_all(&self) -> bool {
        self.all
            || is_all_target_keyword(self.id.as_deref())
            || self
                .session_ids
                .iter()
                .any(|value| value.eq_ignore_ascii_case("all"))
    }

    pub fn target_ids(&self) -> Vec<&str> {
        if self.targets_all() {
            return Vec::new();
        }

        if let Some(id) = self.id.as_deref() {
            return vec![id];
        }

        self.session_ids
            .iter()
            .map(|value| value.as_str())
            .collect()
    }

    pub fn single_target_id(&self) -> Option<&str> {
        let targets = self.target_ids();
        if targets.len() == 1 {
            targets.first().copied()
        } else {
            None
        }
    }

    pub fn is_multi_target(&self) -> bool {
        self.target_ids().len() > 1
    }
}

impl BatchFilterArgs {
    pub fn has_batch_filters(&self) -> bool {
        self.status.is_some()
            || self.runtime.is_some()
            || self.name_contains.is_some()
            || self.dry_run
            || self.yes
            || !self.continue_on_error
            || self.max_failures > 0
            || self.sort.is_some()
            || self.limit.is_some()
            || self.summary
            || self.jobs > 1
    }
}

impl StopArgs {
    pub fn targets_all(&self) -> bool {
        self.target.targets_all()
    }

    pub fn target_ids(&self) -> Vec<&str> {
        self.target.target_ids()
    }

    pub fn single_target_id(&self) -> Option<&str> {
        self.target.single_target_id()
    }

    pub fn is_multi_target(&self) -> bool {
        self.target.is_multi_target()
    }

    pub fn has_batch_filters(&self) -> bool {
        self.batch.has_batch_filters()
    }
}

impl RestartArgs {
    pub fn targets_all(&self) -> bool {
        self.target.targets_all()
    }

    pub fn target_ids(&self) -> Vec<&str> {
        self.target.target_ids()
    }

    pub fn single_target_id(&self) -> Option<&str> {
        self.target.single_target_id()
    }

    pub fn is_multi_target(&self) -> bool {
        self.target.is_multi_target()
    }

    pub fn has_batch_filters(&self) -> bool {
        self.batch.has_batch_filters()
    }
}

impl SuspendArgs {
    pub fn targets_all(&self) -> bool {
        self.target.targets_all()
    }

    pub fn target_ids(&self) -> Vec<&str> {
        self.target.target_ids()
    }

    pub fn single_target_id(&self) -> Option<&str> {
        self.target.single_target_id()
    }

    pub fn is_multi_target(&self) -> bool {
        self.target.is_multi_target()
    }

    pub fn has_batch_filters(&self) -> bool {
        self.batch.has_batch_filters()
    }
}

impl ResumeArgs {
    pub fn targets_all(&self) -> bool {
        self.target.targets_all()
    }

    pub fn target_ids(&self) -> Vec<&str> {
        self.target.target_ids()
    }

    pub fn single_target_id(&self) -> Option<&str> {
        self.target.single_target_id()
    }

    pub fn is_multi_target(&self) -> bool {
        self.target.is_multi_target()
    }

    pub fn has_batch_filters(&self) -> bool {
        self.batch.has_batch_filters()
    }
}
