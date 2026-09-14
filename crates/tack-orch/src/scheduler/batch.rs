//! Priority/fairness scheduling across several requests sharing one
//! candidate pool. [`select_runner`](super::select::select_runner) answers
//! "which runner for *this* request" in isolation; [`schedule`] answers the
//! real dispatch question — several queued requests, a shared finite pool,
//! deciding who goes first. Still pure and deterministic: ordering never
//! depends on the input slices' arrival order, only their content.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::select::{SchedulingPolicy, select_runner};
use super::types::{RunnerCandidate, SchedulingRequest, SelectionOutcome};
use crate::execution::{ExecutionRequestId, RunnerId};

/// Schedules every request in `requests` against the shared `candidates`
/// pool: highest [`super::types::Priority`] first, then oldest `created_at`,
/// then `request_id` — so processing order never depends on input order.
///
/// **Capacity is consumed within the batch, but only here.** A runner
/// [`select_runner`] selects for an earlier request has its
/// `available_capacity` reduced by one for every later request in the same
/// call, so two same-priority requests never both get told "yes" for a
/// single-slot runner. This bookkeeping is local to one `schedule` call —
/// the authoritative ledger is `agent_runners.available_capacity`,
/// decremented only by the repository's fenced claim; re-derive
/// `candidates` from fresh state before the next call.
pub fn schedule(
    requests: &[SchedulingRequest],
    candidates: &[RunnerCandidate],
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Vec<(ExecutionRequestId, SelectionOutcome)> {
    let mut ordered: Vec<&SchedulingRequest> = requests.iter().collect();
    ordered.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.request_id.as_str().cmp(b.request_id.as_str()))
    });

    let mut remaining_capacity: BTreeMap<RunnerId, u32> = candidates
        .iter()
        .map(|c| (c.runner_id.clone(), c.available_capacity))
        .collect();

    let mut results = Vec::with_capacity(ordered.len());
    for request in ordered {
        let adjusted: Vec<RunnerCandidate> = candidates
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c.available_capacity = remaining_capacity.get(&c.runner_id).copied().unwrap_or(0);
                c
            })
            .collect();

        let outcome = select_runner(request, &adjusted, now, policy);
        if let SelectionOutcome::Selected(selection) = &outcome
            && let Some(capacity) = remaining_capacity.get_mut(&selection.runner_id)
        {
            *capacity = capacity.saturating_sub(1);
        }
        results.push((request.request_id.clone(), outcome));
    }
    results
}

#[cfg(test)]
#[path = "batch/tests.rs"]
mod tests;
