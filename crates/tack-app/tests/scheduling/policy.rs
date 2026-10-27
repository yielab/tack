//! Proves `tack_app::model_policy_wiring::resolve_request_model_policy` against a real
//! `tack_db::Repository`: the request's model, else the project's default model for the
//! project's default agent, else none; and the load-bearing safety claim that a run needing a
//! model but given none can never be leased — the unmodified claim path (`scheduler_wiring` +
//! `claim_execution_idempotent_with_snapshot`) rejects it before any `execution_attempts` row,
//! fencing token, or capacity change is committed. Assertions check the database directly (row
//! counts, request state, runner capacity), not just return values.

use chrono::{Duration as ChronoDuration, Utc};
use tack_app::model_policy_wiring::resolve_request_model_policy;
use tack_app::scheduler_wiring::choose_request_for_runner;
use tack_core::models::UpdateProject;
use tack_db::repo::execution::{NewExecutionRequest, NewRunner, RequestSelection};
use tack_db::{Repository, queries};
use tack_orch::model_policy::{ModelPolicyTier, ResolvedModelPolicy};
use tack_orch::scheduler::SchedulingPolicy;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;
use uuid::Uuid;

use crate::scheduling::support::{codex_capability_snapshot, setup_repo};
use tack_test_support::FixedClock;

async fn register_active_runner(
    repo: &Repository,
    runner_id: &str,
    capability_snapshot: &str,
    now: chrono::DateTime<Utc>,
) {
    repo.register_runner(
        NewRunner {
            id: runner_id,
            name: runner_id,
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot,
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    seed::agent_runner_heartbeat(repo, runner_id, &now.to_rfc3339()).await;
}

async fn create_fleet(repo: &Repository, fleet_id: &str, now: chrono::DateTime<Utc>) {
    repo.create_agent_fleet(
        fleet_id,
        &format!("fleet-{fleet_id}"),
        None,
        "{}",
        &FixedClock(now),
    )
    .await
    .expect("insert fleet");
}

async fn add_fleet_member(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
    now: chrono::DateTime<Utc>,
) {
    repo.add_fleet_member(fleet_id, runner_id, &FixedClock(now))
        .await
        .expect("fleet membership");
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_via_fleet(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    idempotency_key: &str,
    fleet_id: &str,
    harness: &str,
    model: Option<&str>,
    created_at: chrono::DateTime<Utc>,
) {
    let selector = serde_json::json!({"kind":"fleet","fleet_id":fleet_id});
    let snapshot = serde_json::json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": idempotency_key,
        "created_by": {"source": "test", "subject_id": "model-policy-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": "profile-1",
        "resolved_agent_profile": {
            "name": "profile", "instructions": "work safely",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "requested_harness_kind": harness,
        "requested_model": model,
        "repository": {
            "kind": "git", "remote": "https://example.test/f3.git",
            "base_revision": "abc123def456abc123def456abc123def456abc", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": {}
    })
    .to_string();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: request_id,
            item_id,
            idempotency_scope: "model-policy-test",
            idempotency_key,
            request_fingerprint: request_id,
            selector_kind: "fleet",
            selector_id: fleet_id,
            agent_profile_id: Some("profile-1"),
            agent_profile_snapshot: r#"{"name":"profile","instructions":"work safely","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
            requested_harness_kind: Some(harness),
            requested_model: model,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/f3.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata: "{}",
            request_snapshot: &snapshot,
        },
        created_at,
    )
    .await
    .expect("enqueue");
}

async fn attempt_count_for_request(repo: &Repository, request_id: &str) -> i64 {
    queries::execution_attempts::attempt_count_of_request(repo, request_id)
        .await
        .expect("count attempts")
}

async fn request_state(repo: &Repository, request_id: &str) -> ExecutionState {
    queries::execution_requests::request_state(repo, request_id)
        .await
        .expect("request state")
        .expect("the request exists")
}

async fn runner_available_capacity(repo: &Repository, runner_id: &str) -> i64 {
    queries::agent_runners::available_capacity(repo, runner_id)
        .await
        .expect("runner capacity")
        .expect("the runner exists")
}

// ---------------------------------------------------------------------
// Resolution against a real `projects` row
// ---------------------------------------------------------------------

async fn project_of(repo: &Repository, item_id: &str) -> Uuid {
    repo.get_item(Uuid::parse_str(item_id).unwrap())
        .await
        .expect("no db error")
        .expect("item exists")
        .project_id
}

async fn set_project_default(
    repo: &Repository,
    project_id: Uuid,
    harness: Option<&str>,
    model: Option<&str>,
) {
    repo.update_project(
        project_id,
        UpdateProject {
            default_harness: Some(harness.map(str::to_owned)),
            default_model: Some(model.map(str::to_owned)),
            ..Default::default()
        },
    )
    .await
    .expect("set project default");
}

#[tokio::test]
async fn the_project_default_model_applies_only_to_the_projects_default_agent() {
    let (repo, item_id) = setup_repo().await;
    let project_id = project_of(&repo, &item_id).await;
    set_project_default(&repo, project_id, Some("codex"), Some("opaque/model-alpha")).await;

    let same_agent = resolve_request_model_policy(&repo, project_id, "codex", None)
        .await
        .expect("no db error");
    assert_eq!(
        same_agent,
        ResolvedModelPolicy {
            model: Some("opaque/model-alpha".into()),
            source: Some(ModelPolicyTier::ProjectDefault),
        }
    );

    let other_agent = resolve_request_model_policy(&repo, project_id, "claude-code", None)
        .await
        .expect("no db error");
    assert_eq!(other_agent.model, None);
    assert_eq!(other_agent.source, None);

    let named = resolve_request_model_policy(&repo, project_id, "codex", Some("opaque/named"))
        .await
        .expect("no db error");
    assert_eq!(named.model.as_deref(), Some("opaque/named"));
    assert_eq!(named.source, Some(ModelPolicyTier::Request));
}

#[tokio::test]
async fn updating_the_default_agent_clears_the_default_model() {
    let (repo, item_id) = setup_repo().await;
    let project_id = project_of(&repo, &item_id).await;
    set_project_default(&repo, project_id, Some("codex"), Some("opaque/model-alpha")).await;

    // The same agent again keeps the model.
    repo.update_project(
        project_id,
        UpdateProject {
            default_harness: Some(Some("codex".into())),
            ..Default::default()
        },
    )
    .await
    .expect("same agent");
    let project = repo.get_project(project_id).await.unwrap().unwrap();
    assert_eq!(project.default_model.as_deref(), Some("opaque/model-alpha"));

    // A different one starts without it.
    repo.update_project(
        project_id,
        UpdateProject {
            default_harness: Some(Some("claude-code".into())),
            ..Default::default()
        },
    )
    .await
    .expect("other agent");
    let project = repo.get_project(project_id).await.unwrap().unwrap();
    assert_eq!(project.default_model, None);
}

// ---------------------------------------------------------------------
// The load-bearing safety claim: a run that needs a model and has none never leases
// ---------------------------------------------------------------------

/// Full pipeline for the safety claim below: resolves the project's default
/// model for `codex`, persists it on the queued request exactly as a wired
/// `POST /executions` handler would, then runs it through the real,
/// unmodified claim path — proving the request is either leased end-to-end
/// or rejected before any state is written, depending on whether any model
/// was named (the only registered runner reports `codex` as needing one).
/// Absence is asserted directly against the database (row counts, request
/// state, runner capacity), not just a function's return value.
async fn assert_default_model_outcome(
    request_id: &str,
    project_default_model: Option<&str>,
    expect_leased: bool,
) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let project_id = project_of(&repo, &item_id).await;
    set_project_default(&repo, project_id, Some("codex"), project_default_model).await;

    register_active_runner(&repo, "runner-a", &codex_capability_snapshot(now), now).await;
    create_fleet(&repo, "fleet-a", now).await;
    add_fleet_member(&repo, "fleet-a", "runner-a", now).await;

    let resolved = resolve_request_model_policy(&repo, project_id, "codex", None)
        .await
        .expect("resolve model policy");
    assert_eq!(resolved.model.as_deref(), project_default_model);

    let key = format!("{request_id}-key");
    enqueue_via_fleet(
        &repo,
        &item_id,
        request_id,
        &key,
        "fleet-a",
        "codex",
        resolved.model.as_deref(),
        now,
    )
    .await;
    let expected = expect_leased.then_some(request_id);
    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen.as_deref(),
        expected,
        "scheduler wiring must match the claim outcome"
    );

    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim",
            "attempt",
            ChronoDuration::seconds(60),
            &FixedClock(now),
            RequestSelection::Scheduled(chosen.as_deref()),
        )
        .await
        .expect("no db error");
    assert_eq!(
        claimed.is_some(),
        expect_leased,
        "claim outcome must match eligibility"
    );
    if let Some(claimed) = claimed {
        assert_eq!(claimed.lease.request_id, request_id);
    }

    let (expect_attempts, expect_state, expect_capacity) = if expect_leased {
        (1, ExecutionState::Leased, 0)
    } else {
        (0, ExecutionState::Queued, 1)
    };
    assert_eq!(
        attempt_count_for_request(&repo, request_id).await,
        expect_attempts
    );
    assert_eq!(request_state(&repo, request_id).await, expect_state);
    assert_eq!(
        runner_available_capacity(&repo, "runner-a").await,
        expect_capacity
    );
}

#[tokio::test]
async fn a_run_with_no_model_for_an_agent_that_needs_one_never_leases() {
    assert_default_model_outcome("req-no-model", None, false).await;
}

/// Positive control for the test above, proving its assertions are
/// load-bearing rather than vacuously true (e.g. because claiming never
/// works in this harness for an unrelated reason): identical pipeline, but
/// the project's default model gives the run one.
#[tokio::test]
async fn a_run_given_a_model_by_the_project_default_leases_successfully() {
    assert_default_model_outcome("req-with-model", Some("opaque/model-alpha"), true).await;
}
