//! Pure input/output types for the deterministic fleet scheduler. Plain data
//! only: no I/O, no clock access (callers pass `now`). Turns a
//! [`SchedulingRequest`] plus a [`RunnerCandidate`] slice into a
//! [`SelectionOutcome`] — never grants the lease itself; that stays the
//! repository's job.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};

use crate::execution::{
    Approvals, ExecutionRequestId, HarnessCapability, HarnessKind, RunnerId, RunnerSelector,
};

/// Mirrors `agent_runners.state` (migration 040). Only [`RunnerState::Active`]
/// is schedulable — pending has no live credential yet, and revoked must
/// never get new work no matter how fresh its last heartbeat looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunnerState {
    PendingEnrollment,
    Active,
    Revoked,
}

/// Caller-supplied scheduling priority, read today from
/// `execution_requests.metadata` (no dedicated column yet). `Normal` is the
/// default so an unset priority never sorts as most urgent in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// Which project budget stopped new work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetKind {
    Tokens,
    /// `usd_per_day`, priced by the operator's rate card.
    Dollars,
    ReviewQueue {
        waiting: u32,
    },
}

/// A request awaiting runner assignment. Pure data, built by whatever caller
/// wires this module to the real `execution_requests` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingRequest {
    pub request_id: ExecutionRequestId,
    pub selector: RunnerSelector,
    pub priority: Priority,
    pub requested_harness_kind: HarnessKind,
    /// The one model string the run is asked to use; `None` leaves it to
    /// the agent.
    pub requested_model: Option<String>,
    /// Mirrors `PermissionPolicy::approvals`: absent means [`Approvals::Auto`].
    /// [`Approvals::Ask`] narrows eligibility to a candidate whose matched
    /// harness attests `decisions: supported` — see
    /// [`IneligibleReason::DecisionsNotSupported`].
    pub approvals: Option<Approvals>,
    /// Every key/value must match the candidate's own `labels` exactly. An
    /// empty map imposes no constraint.
    pub required_labels: BTreeMap<String, String>,
    /// Set by the caller when the request's project has used a daily budget:
    /// no candidate is eligible while it is `Some`.
    pub budget_reached: Option<BudgetKind>,
    /// Fairness tie-break for the batch scheduler: oldest first within the
    /// same [`Priority`]. Mirrors `execution_requests.created_at`.
    pub created_at: DateTime<Utc>,
}

/// One schedulable runner's current state. Capacity and heartbeat come from
/// `agent_runners`' live columns, not the self-reported `capability_snapshot`
/// (can be stale). `harnesses` is the one piece of that snapshot read.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerCandidate {
    pub runner_id: RunnerId,
    pub state: RunnerState,
    pub fleet_memberships: BTreeSet<String>,
    pub labels: BTreeMap<String, String>,
    pub total_capacity: u32,
    pub available_capacity: u32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// A harness absent here, or with a non-`None` `probe_error`, is not
    /// eligible for this runner.
    pub harnesses: Vec<HarnessCapability>,
}

/// Why one candidate was rejected. Every variant names the disqualifying
/// fact — unsupported is typed, unknown is explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IneligibleReason {
    NotRequestedRunner,
    NotFleetMember {
        fleet_id: String,
    },
    RunnerNotActive {
        state: RunnerState,
    },
    /// Missing, or older than `max_heartbeat_age` as of `now` — freshness is
    /// read, never assumed.
    HeartbeatStale {
        last_heartbeat_at: Option<DateTime<Utc>>,
        max_age: Duration,
    },
    NoAvailableCapacity {
        total: u32,
    },
    MissingLabel {
        key: String,
        expected: String,
        actual: Option<String>,
    },
    HarnessNotDeclared {
        harness: HarnessKind,
    },
    HarnessProbeError {
        harness: HarnessKind,
        error: String,
    },
    /// The request names no model and this candidate's matched harness
    /// reports `requires_model`: it will not pick one itself.
    ModelNotNamed {
        harness: HarnessKind,
    },
    /// The request asked [`Approvals::Ask`] and this candidate's matched
    /// harness does not attest `decisions: supported` — `Advisory` and an
    /// absent attestation are rejected the same as `Unsupported`, so a run
    /// is never silently handed to a harness that would just ignore the
    /// operator's choice and run as `auto`.
    DecisionsNotSupported {
        harness: HarnessKind,
    },
    /// The request's project has used a daily budget; nothing new starts for
    /// it, on any runner, until the budget has room again.
    BudgetReached {
        which: BudgetKind,
    },
}

/// An advisory placement: the scheduler's pick, not yet a granted lease.
/// Only the repository's fenced claim makes a lease valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub runner_id: RunnerId,
    pub matched_harness: HarnessKind,
}

/// The result of scheduling one [`SchedulingRequest`] against a candidate
/// slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionOutcome {
    Selected(Selection),
    /// `reasons` names every candidate actually considered — empty only
    /// when `candidates` itself was empty.
    NoEligibleRunner {
        reasons: Vec<(RunnerId, IneligibleReason)>,
    },
    /// No candidate with the requested exact id was present at all —
    /// distinct from `NoEligibleRunner` (present but disqualified).
    UnknownRunner {
        runner_id: RunnerId,
    },
}
