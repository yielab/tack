//! The Docket compatibility decision's label, plus the pure, DB-free pieces of "one
//! scheduling owner" (ADR 0060). Runner-v1 always outranks legacy Docket: an active
//! `execution_requests` row makes legacy dispatch defer, never the reverse.

use tack_db::repo::orch::OrchTask;

/// The one stable compatibility label for this decision (ADR 0060), in a
/// machine-quotable `<decision>:<scope>-v<n>` form for embedding verbatim.
pub const LEGACY_DOCKET_COMPATIBILITY_LABEL: &str = "legacy-docket:maintained-bridge-v1";

/// Human-readable justification for [`LEGACY_DOCKET_COMPATIBILITY_LABEL`].
pub const LEGACY_DOCKET_COMPATIBILITY_POLICY: &str = "Docket is maintained as an optional legacy bridge (TACK_ORCH_ENABLE, default off). \
     It is never the owner of a new runner-v1 execution request; runner-v1 is the \
     plan-of-record scheduler. An item with an active runner-v1 execution \
     request refuses legacy Docket dispatch (one scheduling owner). Docket-origin work \
     is identified with a provider-scoped id (`docket:<remote_task_id>`), distinct from \
     any runner-v1 attempt or opaque model id.";

/// Which plane currently owns (or would own) scheduling an item's execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingOwner {
    /// The neutral runner-v1 domain — the plan-of-record scheduler.
    RunnerV1,
    /// The legacy Docket bridge (`orch_tasks`).
    LegacyDocket,
}

/// Pure decision (no I/O): given whether an item already has an active
/// runner-v1 request, may a *new* legacy Docket dispatch proceed?
/// `Err(RunnerV1)` means runner-v1 already owns this item and legacy must defer.
pub fn decide_scheduling_owner(
    has_active_runner_request: bool,
) -> Result<SchedulingOwner, SchedulingOwner> {
    if has_active_runner_request {
        Err(SchedulingOwner::RunnerV1)
    } else {
        Ok(SchedulingOwner::LegacyDocket)
    }
}

/// Namespaces a docket id as `docket:<remote_task_id>` (see module doc).
pub fn provider_scoped_task_id(remote_task_id: &str) -> String {
    format!("docket:{remote_task_id}")
}

/// Read-only, display-normalized projection of one `orch_tasks` row — a
/// presentation mapping, not a runner-v1 attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyAttemptProjection {
    /// `docket:<remote_task_id>` — see [`provider_scoped_task_id`].
    pub provider_scoped_id: String,
    pub item_id: uuid::Uuid,
    /// Docket's own status string, unvalidated and shown as-is.
    pub remote_status: String,
    /// Always `LegacyDocket` — carried explicitly, never inferred from the id.
    pub scheduling_owner: SchedulingOwner,
}

impl From<&OrchTask> for LegacyAttemptProjection {
    fn from(task: &OrchTask) -> Self {
        Self {
            provider_scoped_id: provider_scoped_task_id(&task.remote_task_id),
            item_id: task.item_id,
            remote_status: task.remote_status.clone(),
            scheduling_owner: SchedulingOwner::LegacyDocket,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn fixture_task(remote_task_id: &str, remote_status: &str) -> OrchTask {
        let item_id = uuid::Uuid::new_v4();
        OrchTask {
            item_id,
            remote_task_id: remote_task_id.to_string(),
            remote_run_id: None,
            remote_status: remote_status.to_string(),
            attempt: 1,
            tokens_in: 0,
            tokens_out: 0,
            cost_usd_estimated: None,
            dispatched_at: Utc::now(),
            trusted: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn runner_active_blocks_legacy_dispatch() {
        assert_eq!(
            decide_scheduling_owner(true),
            Err(SchedulingOwner::RunnerV1)
        );
    }

    #[test]
    fn runner_inactive_allows_legacy_dispatch() {
        assert_eq!(
            decide_scheduling_owner(false),
            Ok(SchedulingOwner::LegacyDocket)
        );
    }

    #[test]
    fn provider_scoped_id_is_namespaced() {
        assert_eq!(provider_scoped_task_id("abc123"), "docket:abc123");
        // Distinct from a bare id — never collides with an unprefixed runner-v1
        // attempt id or opaque model id displayed alongside it.
        assert_ne!(provider_scoped_task_id("abc123"), "abc123");
    }

    #[test]
    fn projection_carries_the_scheduling_owner_explicitly() {
        let task = fixture_task("task-9", "running");
        let projection = LegacyAttemptProjection::from(&task);
        assert_eq!(projection.provider_scoped_id, "docket:task-9");
        assert_eq!(projection.item_id, task.item_id);
        assert_eq!(projection.remote_status, "running");
        assert_eq!(projection.scheduling_owner, SchedulingOwner::LegacyDocket);
    }

    #[test]
    fn unrecognised_remote_status_is_shown_as_is_not_normalized() {
        // repo/orch.rs's own discipline: an unrecognised docket status string is
        // never validated or rewritten by this layer, only carried through.
        let task = fixture_task("task-x", "some_future_docket_status");
        let projection = LegacyAttemptProjection::from(&task);
        assert_eq!(projection.remote_status, "some_future_docket_status");
    }
}
