//! The store the decision tests share: one runner that can hold several attempts, one item.

#![allow(dead_code)]

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};
use tack_db::Repository;
use tack_db::queries::{execution_attempts, execution_decisions, execution_requests};
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::{ControllableClock, create_test_workspace, make_item, make_project, seed};

pub const RUNNER_ID: &str = "runner-decisions";
pub const PROFILE_ID: &str = "profile-decisions";

pub struct FakeClock(pub ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

pub fn clock() -> FakeClock {
    FakeClock(ControllableClock::new(
        Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap(),
    ))
}

/// Registers the runner (capacity for four attempts) and the profile on `repo`; returns the id of
/// one item to run against.
pub async fn seed_store(repo: &Repository, clock: &FakeClock) -> String {
    let workspace_id = create_test_workspace(repo).await;
    let project = make_project(repo, workspace_id).await;
    let item_id = make_item(repo, &project).await.id.to_string();
    seed::runner_with_profile(repo, clock, RUNNER_ID, PROFILE_ID, "hash", 4).await;
    item_id
}

/// A claimed, running attempt tagged `tag`.
pub async fn running_attempt(
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    tag: &str,
) -> String {
    seed::running_attempt(repo, clock, RUNNER_ID, PROFILE_ID, item_id, tag).await
}

pub fn two_options() -> Value {
    json!([
        {"option_id": "allow_once", "label": "Allow once"},
        {"option_id": "deny", "label": "Deny"},
    ])
}

pub async fn pending_decision(
    repo: &Repository,
    clock: &FakeClock,
    attempt_id: &str,
    decision_id: &str,
    expires_at: Option<DateTime<Utc>>,
) {
    seed::pending_decision(
        repo,
        clock,
        RUNNER_ID,
        attempt_id,
        decision_id,
        &two_options().to_string(),
        expires_at,
    )
    .await;
}

/// The stored state of one decision.
pub async fn decision_state(repo: &Repository, attempt_id: &str, decision_id: &str) -> String {
    execution_decisions::decision_of_attempt(repo, attempt_id, decision_id)
        .await
        .expect("decision")
        .expect("decision exists")
        .state
}

/// The states of an attempt and of its request (`seed::running_attempt` names them
/// `att-<tag>` and `req-<tag>`).
pub async fn attempt_and_request_state(
    repo: &Repository,
    attempt_id: &str,
) -> (ExecutionState, ExecutionState) {
    (
        execution_attempts::attempt_state(repo, attempt_id)
            .await
            .expect("attempt state")
            .expect("attempt exists"),
        execution_requests::request_state(repo, &attempt_id.replacen("att-", "req-", 1))
            .await
            .expect("request state")
            .expect("request exists"),
    )
}
