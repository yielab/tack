//! Pure input/output types for the deterministic fleet scheduler.
//!
//! Every type here is plain data: no I/O, no database handle, no clock
//! access baked in (callers pass `now` explicitly — see [`super::select`]).
//! The scheduler's job is to turn a [`SchedulingRequest`] plus a candidate
//! [`RunnerCandidate`] slice into a [`SelectionOutcome`] and nothing else —
//! it never grants the authoritative lease (`TODO.md` III-E1's acceptance
//! criterion; that stays the repository/API's job once a later integration
//! card wires this module to the real `agent_runners`/`agent_fleet_members`
//! tables, see `crates/tack-db/src/migrations.rs` migrations 039–041).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};

use crate::execution::{
    ExecutionRequestId, HarnessCapability, HarnessKind, RequestedModelId, RequestedModelProvider,
    RunnerId, RunnerSelector,
};

/// A runner's enrollment lifecycle state, mirroring `agent_runners.state`
/// (`crates/tack-db/src/migrations.rs`, migration 040: `'pending_enrollment'`
/// | `'active'` | `'revoked'`, enforced today only by hand-written SQL
/// literals in `crates/tack-db/src/repo/execution.rs`, not a typed enum
/// there). Only [`RunnerState::Active`] is ever schedulable — a pending
/// runner has no live credential yet and a revoked one must never be handed
/// new work, no matter how fresh its last heartbeat looked before revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunnerState {
    PendingEnrollment,
    Active,
    Revoked,
}

/// Caller-supplied scheduling priority. `TODO.md` III-E1's task list names
/// "priority/fairness" as a required behavior, but no `execution_requests`
/// column carries a priority value today (see migration 044 in
/// `crates/tack-db/src/migrations.rs` — `state`, `selector_kind`,
/// `requested_harness_kind`, etc., but no `priority`). This type is this
/// card's typed answer to that gap: [`crate::scheduler::batch::schedule`]
/// orders by it, and a later integration card must supply a real value
/// (from a future column, or a policy read out of `execution_requests.metadata`)
/// rather than a caller inventing one ad hoc. `Normal` is the explicit
/// default so an unset priority never silently sorts as the *most* urgent
/// request in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// The requested model, or an explicit request for the runner/harness to
/// auto-select one. Uses [`RequestedModelProvider`]/[`RequestedModelId`] —
/// not the bare `ModelProvider`/`ModelId` a runner uses to *declare*
/// support — because III.0's "vocabulary that must remain distinct" rule
/// treats requested and declared/actual as different namespaces even though
/// both wrap an opaque string; [`super::select`] compares across the two via
/// `.as_str()` rather than conflating the types.
///
/// Deliberately makes the "one of provider/model set, the other absent"
/// shape unrepresentable: `execution::ExecutionRequestSnapshot` carries
/// `requested_model_provider`/`requested_model_id` as two independently
/// nullable fields (III.1.2), so a caller building a [`SchedulingRequest`]
/// from that snapshot must reconcile them through [`ModelSelector::from_parts`],
/// which surfaces the partial case as a typed
/// [`super::select::SchedulingError`] instead of silently guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelector {
    Explicit {
        provider: RequestedModelProvider,
        model_id: RequestedModelId,
    },
    AutoSelect,
}

impl ModelSelector {
    /// Reconciles the two independently-nullable wire fields into a
    /// [`ModelSelector`]. `Ok(AutoSelect)` only when *both* are absent;
    /// exactly one present is a caller/data error, not a runner-eligibility
    /// question, so it is reported once here rather than repeated as an
    /// identical [`super::select::IneligibleReason`] against every
    /// candidate.
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

/// A request awaiting runner assignment. Pure data — no reference to any
/// database row or HTTP payload — built by whatever later integration card
/// wires this module to the real `execution_requests` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingRequest {
    pub request_id: ExecutionRequestId,
    /// Exact runner, fleet, or `Any` — `execution::RunnerSelector` verbatim
    /// (III.1.2), not a re-declared copy.
    pub selector: RunnerSelector,
    pub priority: Priority,
    pub requested_harness_kind: HarnessKind,
    pub requested_model: ModelSelector,
    /// Every key/value here must match the candidate's own `labels`
    /// (case-sensitive, exact value match) for the candidate to be eligible.
    /// An empty map imposes no label constraint.
    pub required_labels: BTreeMap<String, String>,
    /// When this request entered the queue — the batch scheduler's fairness
    /// tie-break (oldest first within the same [`Priority`]). Mirrors
    /// `execution_requests.created_at`.
    pub created_at: DateTime<Utc>,
}

/// One schedulable runner's current state, as the caller resolved it from
/// `agent_runners` + `agent_fleet_members` (migrations 039–041). Capacity and
/// heartbeat freshness come from `agent_runners`' own live columns
/// (`available_capacity`, `last_heartbeat_at`), not from the runner's
/// self-reported `capability_snapshot` — that JSON blob is refreshed only on
/// enroll/refresh (III.1.4) and can be stale relative to the DB's live
/// capacity ledger, which every claim/heartbeat/completion call updates
/// directly (`crates/tack-db/src/repo/execution.rs`). `harnesses` is the one
/// piece of the capability snapshot the scheduler does read: whichever
/// harness/model combinations the runner most recently reported.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerCandidate {
    pub runner_id: RunnerId,
    pub state: RunnerState,
    /// Every fleet id (`agent_fleets.id`) this runner is currently a member
    /// of, via `agent_fleet_members`. Empty for a runner enrolled but not
    /// yet placed in any fleet.
    pub fleet_memberships: BTreeSet<String>,
    pub labels: BTreeMap<String, String>,
    pub total_capacity: u32,
    pub available_capacity: u32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// The runner's most recently reported harness/model support
    /// (`execution::HarnessCapability`, from `agent_runners.capability_snapshot`
    /// verbatim) — read, never assumed. A harness absent from this list, or
    /// present with a non-`None` `probe_error`, is not eligible for this
    /// runner.
    pub harnesses: Vec<HarnessCapability>,
}

/// Why one candidate was rejected. Every variant names the exact fact that
/// disqualified it — `TODO.md` III.2 rule 7 ("unsupported is typed, unknown
/// is explicit") and III-E1's acceptance criterion ("invalid combinations
/// name reasons, not a bare boolean").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IneligibleReason {
    /// `selector` was `ExactRunner { runner_id }` and this candidate is not
    /// that runner.
    NotRequestedRunner,
    /// `selector` was `Fleet { fleet_id }` and this candidate's
    /// `fleet_memberships` does not contain it.
    NotFleetMember { fleet_id: String },
    /// Only [`RunnerState::Active`] is schedulable.
    RunnerNotActive { state: RunnerState },
    /// `last_heartbeat_at` is missing, or older than the policy's
    /// `max_heartbeat_age` as of `now`. Mirrors the Wave 3 carry-forward
    /// instruction (`TODO.md`, "Wave 3 carry-forward"): the scheduler reads
    /// freshness, it does not assume a runner is alive.
    HeartbeatStale {
        last_heartbeat_at: Option<DateTime<Utc>>,
        max_age: Duration,
    },
    /// `available_capacity` is zero.
    NoAvailableCapacity { total: u32 },
    /// A key from `required_labels` is missing, or present with a different
    /// value, on this candidate.
    MissingLabel {
        key: String,
        expected: String,
        actual: Option<String>,
    },
    /// The requested harness does not appear in this candidate's most
    /// recently reported `harnesses` at all.
    HarnessNotDeclared { harness: HarnessKind },
    /// The requested harness was declared, but its last probe recorded an
    /// error — not currently usable, whatever it worked before.
    HarnessProbeError { harness: HarnessKind, error: String },
    /// [`ModelSelector::Explicit`] named a provider/model this candidate's
    /// declared `model_combinations` for the matched harness does not list.
    ModelCombinationNotDeclared {
        harness: HarnessKind,
        provider: String,
        model_id: String,
    },
    /// [`ModelSelector::AutoSelect`] was requested. No capability field in
    /// runner-v1 v1 (`docs/contracts/runner-v1/capabilities.json`) records
    /// whether a harness safely accepts an unspecified model — the Wave 3
    /// carry-forward found that two of three real adapters (Codex, OpenCode)
    /// reject auto-select pre-spawn rather than fabricate a selection. Until
    /// a capability snapshot can attest to this, every candidate is reported
    /// ineligible for an auto-select request with this named reason rather
    /// than silently narrowing to whichever harness the scheduler happens to
    /// guess is safe. See this card's handoff, "Known limitations".
    AutoSelectNotVerified { harness: HarnessKind },
}

/// A successful, advisory placement: this runner is the scheduler's pick,
/// not yet a granted lease. Only the repository's fenced claim (III.1.5) can
/// make a lease valid — see this module's top doc comment.
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
    /// No candidate qualified. `reasons` names every candidate that was
    /// actually considered and why it was rejected — empty only when
    /// `candidates` itself was empty (an empty fleet, or no runners at all).
    NoEligibleRunner {
        reasons: Vec<(RunnerId, IneligibleReason)>,
    },
    /// `selector` was `ExactRunner { runner_id }` and no candidate with that
    /// id was present at all — distinct from `NoEligibleRunner` (which means
    /// the runner was present but disqualified) because "doesn't exist" and
    /// "exists but unhealthy" call for different operator responses.
    UnknownRunner {
        runner_id: RunnerId,
    },
}
