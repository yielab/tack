//! Black-box tests for the deterministic fleet scheduler (III-E1), exercised
//! only through `tack_orch::scheduler`'s public API — no access to private
//! module internals, the same vantage point a later integration card has.
//!
//! These complement (do not duplicate) the table tests inside
//! `crates/tack-orch/src/scheduler/{select,batch}.rs`: this file's job is
//! the property-style "identical input selects identically, regardless of
//! arrival order" claim, exercised over every permutation of a small
//! candidate/request set rather than a couple of hand-picked reorderings.
//! No `proptest`/`quickcheck` dependency is added — workspace `Cargo.lock`
//! is B3's chokepoint this wave (`TODO.md` III.3) — so permutations are
//! enumerated by hand with a small recursive generator, the same
//! no-new-dependency discipline `reconciler.rs`'s deterministic jitter
//! already uses instead of pulling in `rand`.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeZone, Utc};

use tack_orch::execution::{
    ExecutionRequestId, HarnessCapability, HarnessKind, ModelCombination, ModelId, ModelProvider,
    RequestedModelId, RequestedModelProvider, RunnerId, RunnerSelector,
};
use tack_orch::scheduler::{
    ModelSelector, Priority, RunnerCandidate, RunnerState, SchedulingPolicy, SchedulingRequest,
    Selection, SelectionOutcome, schedule, select_runner,
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap()
}

fn harness(kind: &str, combos: Vec<(&str, &[&str])>) -> HarnessCapability {
    HarnessCapability {
        harness_kind: HarnessKind::new(kind),
        installed_version: "1.0.0".to_string(),
        probe_error: None,
        probed_at: now(),
        model_combinations: combos
            .into_iter()
            .map(|(provider, models)| ModelCombination {
                model_provider: ModelProvider::new(provider),
                model_ids: models.iter().map(|m| ModelId::new(*m)).collect(),
                discovery: "reported".to_string(),
                additional: BTreeMap::new(),
            })
            .collect(),
        additional: BTreeMap::new(),
    }
}

fn candidate(id: &str, available_capacity: u32) -> RunnerCandidate {
    RunnerCandidate {
        runner_id: RunnerId::new(id),
        state: RunnerState::Active,
        fleet_memberships: BTreeSet::from(["fleet-main".to_string()]),
        labels: BTreeMap::new(),
        total_capacity: available_capacity.max(1),
        available_capacity,
        last_heartbeat_at: Some(now()),
        harnesses: vec![harness(
            "claude_code",
            vec![("anthropic", &["opaque/sonnet"])],
        )],
    }
}

fn request() -> SchedulingRequest {
    SchedulingRequest {
        request_id: ExecutionRequestId::new("req-1"),
        selector: RunnerSelector::Fleet {
            fleet_id: "fleet-main".to_string(),
        },
        priority: Priority::Normal,
        requested_harness_kind: HarnessKind::new("claude_code"),
        requested_model: ModelSelector::Explicit {
            provider: RequestedModelProvider::new("anthropic"),
            model_id: RequestedModelId::new("opaque/sonnet"),
        },
        required_labels: BTreeMap::new(),
        created_at: now(),
    }
}

/// Every permutation of `items`, via a small recursive Heap's-algorithm-style
/// generator — no dependency added (see module doc).
fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head.clone());
            out.push(tail);
        }
    }
    out
}

#[test]
fn permutations_helper_produces_the_expected_count() {
    // Sanity-check the test helper itself: 4! = 24.
    let items = ["a", "b", "c", "d"];
    assert_eq!(permutations(&items).len(), 24);
}

#[test]
fn selection_is_identical_across_every_permutation_of_a_heterogeneous_fleet() {
    // Four candidates, each disqualified (or not) for a different reason —
    // deliberately heterogeneous, deliberately including a near-tie so the
    // capacity/id tie-break is exercised too.
    let mut wrong_fleet = candidate("runner-outsider", 5);
    wrong_fleet.fleet_memberships = BTreeSet::from(["fleet-other".to_string()]);

    let mut saturated = candidate("runner-full", 5);
    saturated.available_capacity = 0;

    let winner_a = candidate("runner-b", 3);
    let winner_b = candidate("runner-a", 3); // ties winner_a on capacity; lower id wins

    let candidates = vec![wrong_fleet, saturated, winner_a, winner_b];
    let req = request();
    let policy = SchedulingPolicy::default();

    let expected = SelectionOutcome::Selected(Selection {
        runner_id: RunnerId::new("runner-a"),
        matched_harness: HarnessKind::new("claude_code"),
    });

    for permutation in permutations(&candidates) {
        let outcome = select_runner(&req, &permutation, now(), &policy);
        assert_eq!(
            outcome,
            expected,
            "selection changed for permutation starting with {:?}",
            permutation.first().map(|c| c.runner_id.clone())
        );
    }
}

#[test]
fn no_eligible_runner_outcome_is_identical_across_every_permutation() {
    // Three candidates, all ineligible for distinct reasons — the NoEligibleRunner
    // `reasons` list must come back byte-identical (already sorted by runner_id
    // inside select_runner) no matter what order the caller supplies them in.
    let mut wrong_fleet = candidate("runner-outsider", 5);
    wrong_fleet.fleet_memberships = BTreeSet::from(["fleet-other".to_string()]);

    let mut saturated = candidate("runner-full", 5);
    saturated.available_capacity = 0;

    let mut revoked = candidate("runner-gone", 5);
    revoked.state = RunnerState::Revoked;

    let candidates = vec![wrong_fleet, saturated, revoked];
    let req = request();
    let policy = SchedulingPolicy::default();

    let first_outcome = select_runner(&req, &candidates, now(), &policy);
    assert!(matches!(
        first_outcome,
        SelectionOutcome::NoEligibleRunner { .. }
    ));

    for permutation in permutations(&candidates) {
        let outcome = select_runner(&req, &permutation, now(), &policy);
        assert_eq!(outcome, first_outcome);
    }
}

#[test]
fn batch_schedule_outcome_is_identical_across_every_permutation_of_requests() {
    let candidates = vec![candidate("runner-a", 1)];
    let policy = SchedulingPolicy::default();

    let mut base = request();
    base.request_id = ExecutionRequestId::new("req-normal");
    base.priority = Priority::Normal;

    let mut low = request();
    low.request_id = ExecutionRequestId::new("req-low");
    low.priority = Priority::Low;

    let mut high = request();
    high.request_id = ExecutionRequestId::new("req-high");
    high.priority = Priority::High;

    let requests = vec![base, low, high];

    let mut expected = schedule(&requests, &candidates, now(), &policy);
    expected.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    for permutation in permutations(&requests) {
        let mut results = schedule(&permutation, &candidates, now(), &policy);
        results.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        assert_eq!(results, expected);
    }

    // And the high-priority request is the one that actually won the only slot.
    let high_result = expected
        .iter()
        .find(|(id, _)| id.as_str() == "req-high")
        .unwrap();
    assert!(matches!(high_result.1, SelectionOutcome::Selected(_)));
}

#[test]
fn advisory_selection_never_claims_to_be_a_lease() {
    // III-E1's acceptance gate: "only the repository claim can make a lease
    // valid." This is a documentation-level property, not something the type
    // system alone proves, so pin it down structurally: `Selection` carries
    // no fencing token, no lease timestamps, and no state transition — those
    // fields exist only on `execution::AttemptSnapshot`, which this crate's
    // scheduler module never constructs. A scheduler that started minting one
    // of those fields would be silently claiming authority it must not have.
    let candidates = vec![candidate("runner-a", 1)];
    let outcome = select_runner(&request(), &candidates, now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::Selected(selection) => {
            // Selection has exactly two fields: runner_id, matched_harness.
            // A destructure with no `..` fails to compile if a third field
            // is ever added without this test being revisited.
            let Selection {
                runner_id: _,
                matched_harness: _,
            } = selection;
        }
        other => panic!("expected Selected, got {other:?}"),
    }
}

#[test]
fn stale_heartbeat_wins_over_capacity_when_both_would_otherwise_pass() {
    // Adversarial case named in the Wave 3 carry-forward: a scheduler must
    // read freshness, not assume a quiet runner is still alive. This proves
    // the check is load-bearing, not merely present: a candidate with ample
    // capacity and a perfectly matching harness is still rejected once its
    // heartbeat is one second past the policy's max age.
    let policy = SchedulingPolicy::default();
    let mut stale = candidate("runner-a", 5);
    stale.last_heartbeat_at = Some(now() - policy.max_heartbeat_age - chrono::Duration::seconds(1));

    let outcome = select_runner(&request(), &[stale], now(), &policy);
    assert!(matches!(outcome, SelectionOutcome::NoEligibleRunner { .. }));

    // One second inside the window, everything else equal, it is selected —
    // proving the boundary is the heartbeat age, not some other hidden factor.
    let mut fresh_enough = candidate("runner-a", 5);
    fresh_enough.last_heartbeat_at =
        Some(now() - policy.max_heartbeat_age + chrono::Duration::seconds(1));
    let outcome = select_runner(&request(), &[fresh_enough], now(), &policy);
    assert!(matches!(outcome, SelectionOutcome::Selected(_)));
}
