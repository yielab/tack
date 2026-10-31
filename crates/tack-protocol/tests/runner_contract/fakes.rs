use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FakeClock {
    now_millis: u64,
}

impl FakeClock {
    pub(crate) fn new(now_millis: u64) -> Self {
        Self { now_millis }
    }

    pub(crate) fn now_millis(self) -> u64 {
        self.now_millis
    }

    pub(crate) fn advance(&mut self, millis: u64) {
        self.now_millis = self
            .now_millis
            .checked_add(millis)
            .expect("fake clock overflow");
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CommitResult {
    Applied(String),
    Replay(String),
    StaleFence,
    IdempotencyConflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecoveryResult {
    Applied {
        disposition: String,
        committed_at: u64,
    },
    Replay {
        disposition: String,
        committed_at: u64,
    },
    IdempotencyConflict,
}

#[derive(Debug)]
struct FakeRecoveryLedger {
    clock: FakeClock,
    reports: HashMap<String, (String, String, u64)>,
}

impl FakeRecoveryLedger {
    fn new(now_millis: u64) -> Self {
        Self {
            clock: FakeClock::new(now_millis),
            reports: HashMap::new(),
        }
    }

    fn observe(
        &mut self,
        recovery_key: &str,
        canonical_request: &str,
        disposition: &str,
    ) -> RecoveryResult {
        if let Some((original_request, original_disposition, committed_at)) =
            self.reports.get(recovery_key)
        {
            return if original_request == canonical_request {
                RecoveryResult::Replay {
                    disposition: original_disposition.clone(),
                    committed_at: *committed_at,
                }
            } else {
                RecoveryResult::IdempotencyConflict
            };
        }
        let committed_at = self.clock.now_millis();
        self.reports.insert(
            recovery_key.to_owned(),
            (
                canonical_request.to_owned(),
                disposition.to_owned(),
                committed_at,
            ),
        );
        RecoveryResult::Applied {
            disposition: disposition.to_owned(),
            committed_at,
        }
    }
}

#[derive(Debug)]
struct FakeRunner {
    clock: FakeClock,
    queued: bool,
    active_fence: u64,
    lease_expires_at: u64,
    committed_events: Vec<String>,
    completions: HashMap<String, (String, String)>,
}

impl FakeRunner {
    fn new() -> Self {
        Self {
            clock: FakeClock::new(1_000),
            queued: true,
            active_fence: 0,
            lease_expires_at: 0,
            committed_events: Vec::new(),
            completions: HashMap::new(),
        }
    }

    fn claim(&mut self, lease_millis: u64) -> Option<u64> {
        if !self.queued {
            return None;
        }
        self.queued = false;
        self.active_fence += 1;
        self.lease_expires_at = self.clock.now_millis() + lease_millis;
        Some(self.active_fence)
    }

    fn report_event(&mut self, fence: u64, event: &str) -> CommitResult {
        if fence != self.active_fence || self.clock.now_millis() >= self.lease_expires_at {
            return CommitResult::StaleFence;
        }
        self.committed_events.push(event.to_owned());
        CommitResult::Applied(event.to_owned())
    }

    fn complete(&mut self, fence: u64, idempotency_key: &str, payload: &str) -> CommitResult {
        if fence != self.active_fence || self.clock.now_millis() >= self.lease_expires_at {
            return CommitResult::StaleFence;
        }
        if let Some((original_payload, result)) = self.completions.get(idempotency_key) {
            return if original_payload == payload {
                CommitResult::Replay(result.clone())
            } else {
                CommitResult::IdempotencyConflict
            };
        }

        let result = "succeeded".to_owned();
        self.completions.insert(
            idempotency_key.to_owned(),
            (payload.to_owned(), result.clone()),
        );
        CommitResult::Applied(result)
    }
}

#[test]
fn fake_time_advances_lease_expiry_without_sleeping() {
    let mut runner = FakeRunner::new();
    let fence = runner.claim(500).expect("queued work can be claimed");
    assert_eq!(
        runner.report_event(fence, "before-expiry"),
        CommitResult::Applied("before-expiry".to_owned())
    );

    runner.clock.advance(500);
    assert_eq!(
        runner.report_event(fence, "after-expiry"),
        CommitResult::StaleFence
    );
    assert_eq!(runner.committed_events, ["before-expiry"]);
}

#[test]
fn concurrent_claim_driver_produces_exactly_one_winner() {
    let runner = Arc::new(Mutex::new(FakeRunner::new()));
    let handles = (0..16)
        .map(|_| {
            let runner = Arc::clone(&runner);
            thread::spawn(move || runner.lock().expect("runner lock poisoned").claim(500))
        })
        .collect::<Vec<_>>();

    let winners = handles
        .into_iter()
        .filter_map(|handle| handle.join().expect("claimer thread panicked"))
        .collect::<Vec<_>>();
    assert_eq!(winners, [1]);
}

#[test]
fn stale_fence_driver_writes_nothing() {
    let mut runner = FakeRunner::new();
    let active_fence = runner.claim(500).expect("queued work can be claimed");
    assert_eq!(
        runner.report_event(active_fence + 1, "forged"),
        CommitResult::StaleFence
    );
    assert!(runner.committed_events.is_empty());
}

#[test]
fn byte_equivalent_replay_returns_original_and_conflicting_reuse_fails() {
    let mut runner = FakeRunner::new();
    let fence = runner.claim(500).expect("queued work can be claimed");

    assert_eq!(
        runner.complete(fence, "complete-1", r#"{"state":"succeeded"}"#),
        CommitResult::Applied("succeeded".to_owned())
    );
    assert_eq!(
        runner.complete(fence, "complete-1", r#"{"state":"succeeded"}"#),
        CommitResult::Replay("succeeded".to_owned())
    );
    assert_eq!(
        runner.complete(fence, "complete-1", r#"{"state":"failed"}"#),
        CommitResult::IdempotencyConflict
    );
    assert_eq!(runner.completions.len(), 1);
}

#[test]
fn recovery_replay_key_is_stable_and_ledgers_share_no_global_state() {
    let key = "recovery:attempt:7:process_stopped";
    let request = r#"{"observation":"process_stopped","process_observed":false}"#;
    let mut first = FakeRecoveryLedger::new(1_000);
    assert_eq!(
        first.observe(key, request, "safe_pre_spawn_requeue"),
        RecoveryResult::Applied {
            disposition: "safe_pre_spawn_requeue".to_owned(),
            committed_at: 1_000,
        }
    );
    first.clock.advance(500);
    assert_eq!(
        first.observe(key, request, "needs_operator"),
        RecoveryResult::Replay {
            disposition: "safe_pre_spawn_requeue".to_owned(),
            committed_at: 1_000,
        }
    );
    assert_eq!(
        first.observe(
            key,
            r#"{"observation":"ambiguous","process_observed":true}"#,
            "needs_operator",
        ),
        RecoveryResult::IdempotencyConflict
    );

    let mut independent = FakeRecoveryLedger::new(2_000);
    assert_eq!(
        independent.observe(key, request, "safe_pre_spawn_requeue"),
        RecoveryResult::Applied {
            disposition: "safe_pre_spawn_requeue".to_owned(),
            committed_at: 2_000,
        },
        "a separate deterministic test ledger must not share mutable replay state"
    );
}
