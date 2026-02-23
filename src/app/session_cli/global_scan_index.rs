use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use launch_code::model::{SessionRecord, SessionStatus};
use serde::{Deserialize, Serialize};

use super::{ListFilters, ListStatusArg};

const GLOBAL_LIST_SCAN_INDEX_FILE: &str = "list-global-index.json";
const GLOBAL_LIST_SCAN_INDEX_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StateFileSignature {
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkSessionStatusSummary {
    state_signature: StateFileSignature,
    total_count: usize,
    running_count: usize,
    stopped_count: usize,
    suspended_count: usize,
    unknown_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalListScanIndexDoc {
    schema_version: u32,
    links: BTreeMap<String, LinkSessionStatusSummary>,
}

impl Default for GlobalListScanIndexDoc {
    fn default() -> Self {
        Self {
            schema_version: GLOBAL_LIST_SCAN_INDEX_SCHEMA_VERSION,
            links: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct GlobalListScanIndex {
    path: Option<PathBuf>,
    doc: GlobalListScanIndexDoc,
}

impl GlobalListScanIndex {
    pub(super) fn load_best_effort() -> Self {
        let path = index_path();
        let Some(ref resolved_path) = path else {
            return Self {
                path,
                doc: GlobalListScanIndexDoc::default(),
            };
        };

        let payload = match fs::read_to_string(resolved_path) {
            Ok(value) => value,
            Err(_) => {
                return Self {
                    path,
                    doc: GlobalListScanIndexDoc::default(),
                };
            }
        };

        let doc: GlobalListScanIndexDoc = serde_json::from_str(&payload).unwrap_or_default();
        if doc.schema_version != GLOBAL_LIST_SCAN_INDEX_SCHEMA_VERSION {
            return Self {
                path,
                doc: GlobalListScanIndexDoc::default(),
            };
        }

        Self { path, doc }
    }

    pub(super) fn persist_best_effort(&self) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let payload = match serde_json::to_string(&self.doc) {
            Ok(value) => value,
            Err(_) => return,
        };
        let _ = fs::write(path, payload);
    }

    pub(super) fn should_skip_for_filters(
        &self,
        link_path: &str,
        signature: &StateFileSignature,
        filters: &ListFilters,
    ) -> bool {
        let Some(summary) = self.doc.links.get(link_path) else {
            return false;
        };
        if &summary.state_signature != signature {
            return false;
        }
        if summary.total_count == 0 {
            return true;
        }

        match filters.status_filter {
            Some(ListStatusArg::Running) => summary.running_count == 0,
            Some(ListStatusArg::Stopped) => summary.stopped_count == 0,
            Some(ListStatusArg::Suspended) => summary.suspended_count == 0,
            Some(ListStatusArg::Unknown) => summary.unknown_count == 0,
            None => false,
        }
    }

    pub(super) fn update_link_summary(
        &mut self,
        link_path: &str,
        signature: StateFileSignature,
        sessions: &[SessionRecord],
    ) {
        let mut running_count = 0usize;
        let mut stopped_count = 0usize;
        let mut suspended_count = 0usize;
        let mut unknown_count = 0usize;

        for session in sessions {
            match session.status {
                SessionStatus::Running => running_count = running_count.saturating_add(1),
                SessionStatus::Stopped => stopped_count = stopped_count.saturating_add(1),
                SessionStatus::Suspended => suspended_count = suspended_count.saturating_add(1),
                SessionStatus::Unknown => unknown_count = unknown_count.saturating_add(1),
            }
        }

        self.doc.links.insert(
            link_path.to_string(),
            LinkSessionStatusSummary {
                state_signature: signature,
                total_count: sessions.len(),
                running_count,
                stopped_count,
                suspended_count,
                unknown_count,
            },
        );
    }
}

pub(super) fn read_state_signature(path: &Path) -> Option<StateFileSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let timestamp = modified.duration_since(UNIX_EPOCH).ok()?;

    Some(StateFileSignature {
        len: metadata.len(),
        modified_secs: timestamp.as_secs(),
        modified_nanos: timestamp.subsec_nanos(),
    })
}

fn index_path() -> Option<PathBuf> {
    crate::link_registry::resolve_home_dir()
        .ok()
        .map(|home| home.join(".launch-code").join(GLOBAL_LIST_SCAN_INDEX_FILE))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use launch_code::model::{LaunchMode, LaunchSpec, RuntimeKind, SessionRecord, SessionStatus};

    use super::{GlobalListScanIndex, StateFileSignature};
    use crate::cli::ListStatusArg;

    fn build_signature(len: u64) -> StateFileSignature {
        StateFileSignature {
            len,
            modified_secs: 1,
            modified_nanos: 2,
        }
    }

    fn empty_index() -> GlobalListScanIndex {
        GlobalListScanIndex {
            path: None,
            doc: super::GlobalListScanIndexDoc::default(),
        }
    }

    fn build_session(id: &str, status: SessionStatus) -> SessionRecord {
        SessionRecord {
            id: id.to_string(),
            spec: LaunchSpec {
                name: id.to_string(),
                runtime: RuntimeKind::Python,
                entry: "app.py".to_string(),
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
            status,
            pid: None,
            supervisor_pid: None,
            log_path: None,
            debug_meta: None,
            created_at: 1,
            updated_at: 1,
            last_exit_code: None,
            restart_count: 0,
        }
    }

    #[test]
    fn should_skip_for_running_filter_when_running_count_is_zero() {
        let mut index = empty_index();
        let signature = build_signature(10);
        index.update_link_summary(
            "/tmp/project",
            signature.clone(),
            &[build_session("a", SessionStatus::Stopped)],
        );

        let filters = super::super::ListFilters {
            status_filter: Some(ListStatusArg::Running),
            runtime_filter: None,
            name_filter: None,
        };

        assert!(index.should_skip_for_filters("/tmp/project", &signature, &filters));
    }

    #[test]
    fn should_not_skip_when_signature_does_not_match() {
        let mut index = empty_index();
        index.update_link_summary(
            "/tmp/project",
            build_signature(10),
            &[build_session("a", SessionStatus::Stopped)],
        );

        let filters = super::super::ListFilters {
            status_filter: Some(ListStatusArg::Running),
            runtime_filter: None,
            name_filter: None,
        };

        assert!(!index.should_skip_for_filters("/tmp/project", &build_signature(11), &filters));
    }

    #[test]
    fn should_skip_when_link_summary_has_zero_sessions() {
        let mut index = empty_index();
        let signature = build_signature(10);
        index.update_link_summary("/tmp/project", signature.clone(), &[]);

        let filters = super::super::ListFilters {
            status_filter: None,
            runtime_filter: None,
            name_filter: None,
        };

        assert!(index.should_skip_for_filters("/tmp/project", &signature, &filters));
    }
}
