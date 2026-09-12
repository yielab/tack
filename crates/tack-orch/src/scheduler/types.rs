//! Pure input/output types for the deterministic fleet scheduler.
//!
//! Plain data only: no I/O, no clock access (callers pass `now` — see
//! [`super::select`]). Turns a [`SchedulingRequest`] plus a candidate
//! [`RunnerCandidate`] slice into a [`SelectionOutcome`] — it never grants
//! the lease itself; that stays the repository's job against the real
//! `agent_runners`/`agent_fleet_members` tables (migrations 039–041).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};

use crate::execution::{
    ExecutionRequestId, HarnessCapability, HarnessKind, RequestedModelId, RequestedModelProvider,
    RunnerId, RunnerSelector,
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

/// Caller-supplied scheduling priority. No `execution_requests` column
/// carries one yet, so this is a typed stand-in, read today from
/// `execution_requests.metadata`. `Normal` is the default so an unset
/// priority never sorts as the most urgent request in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// The requested model, or an explicit request to auto-select. Uses
/// [`RequestedModelProvider`]/[`RequestedModelId`], distinct from the
/// runner's declared `ModelProvider`/`ModelId`; [`super::select`] compares
/// them via `.as_str()`. [`ModelSelector::from_parts`] makes "one of
/// provider/model set, the other absent" unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelector {
    Explicit {
        provider: RequestedModelProvider,
        model_id: RequestedModelId,
    },
    AutoSelect,
}

impl ModelSelector {
    /// `Ok(AutoSelect)` only when both wire fields are absent; exactly one
    /// present is a caller/data error, reported once here rather than
    /// repeated per candidate as an [`super::select::IneligibleReason`].
    pub fn from_parts(
        provider: Option<RequestedModelProvider>,
        model_id: Option<RequestedModelId>,
    ) -> Result<Self, super::select::SchedulingError> {
        match (provider, model_id) {
            (Some(provider), Some(model_id)) => Ok(Self::Explicit { provider, model_id }),
            (None, None) => Ok(Self::AutoSelect),
            (Some(_), None) | (None, Some(_)) => {
                Err(super::select::SchedulingError::PartialModelSelector)
            }
        }
    }
}

/// A request awaiting runner assignment. Pure data, built by whatever caller
/// wires this module to the real `execution_requests` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingRequest {
    pub request_id: ExecutionRequestId,
    pub selector: RunnerSelector,
    pub priority: Priority,
    pub requested_harness_kind: HarnessKind,
    pub requested_model: ModelSelector,
    /// Every key/value must match the candidate's own `labels` exactly. An
    /// empty map imposes no constraint.
    pub required_labels: BTreeMap<String, String>,
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
    /// Not in this candidate's declared combinations, and the harness didn't
    /// attest `model_passthrough: supported` (which would make it eligible
    /// without being declared).
    ModelCombinationNotDeclared {
        harness: HarnessKind,
        provider: String,
        model_id: String,
    },
    /// No runner-v1 capability field records whether a harness safely
    /// accepts an unspecified model, so auto-select is ineligible everywhere
    /// rather than guessed safe.
    AutoSelectNotVerified {
        harness: HarnessKind,
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
