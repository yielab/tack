//! Proves `create_execution`'s model resolution
//! (`tack_app::model_policy_wiring::resolve_request_model_policy`) and
//! `GET /api/executions/{id}/attempts`'s `model_provenance`/
//! `usage_economics` fields work through the real production router
//! (`tack_api::router::build_router`), not a handler-local harness. Every
//! claim is checked against persisted database state or an exact JSON
//! shape, never a bare 2xx status.

use crate::common;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::{
    Repository,
    queries::{execution_attempts, execution_requests},
};

const OPERATOR_TOKEN: &str = "model-wiring-operator-token";
const BASE_REVISION: &str = "f6b0123456789abcdef0123456789abcdef0123";

async fn setup() -> (axum::Router, Repository, String, String) {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
    };
    let app = crate::common::with_default_host(build_router(state));

    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "Model wiring".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/wiring.git".into()),
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            tack_core::models::CreateItem {
                recurrence: None,
                recurrence_of: None,
                title: "Prove the model-policy/provenance wiring".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
                source_artifact_id: None,
            },
        )
        .await
        .expect("item");
    (app, repo, item.id.to_string(), project.id.to_string())
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer model-wiring-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// A runner whose `codex` needs a model named — every fixture in this file
/// that needs a successful claim names one.
fn full_capabilities() -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "reported_at": now,
        "labels": {"os": "linux"},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now,
            "requires_model": true,
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending) = common::send(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = common::send(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    (
        runner_id,
        [("authorization".to_string(), bearer(&credential))],
    )
}

fn headers_ref(owned: &[(String, String); 1]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

async fn create_agent_profile(app: &axum::Router, name: &str) -> String {
    let (status, profile) = common::send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": name, "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

/// `requested_model` is the field under test — `None` reproduces "client
/// expressed no opinion", the case wired to the resolver.
fn execution_request_body(
    item_id: &str,
    key: &str,
    selector_kind: &str,
    selector_id: &str,
    agent_profile_id: &str,
    requested_model: Option<&str>,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": selector_kind,
        "selector_id": selector_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model": requested_model,
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/wiring.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

async fn stored_requested_model(repo: &Repository, request_id: &str) -> Option<String> {
    execution_requests::requested_model_of_request(repo, request_id)
        .await
        .unwrap()
        .expect("execution_requests row must exist")
}

async fn stored_requested_model_snapshot(repo: &Repository, request_id: &str) -> Value {
    let snapshot = execution_requests::request_snapshot(repo, request_id)
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&snapshot).unwrap()
}

// =======================================================================
// `create_execution` resolves an absent/explicit model choice: the request's
// own, else the project's default for its default agent, else none — proved
// against the persisted row and its idempotency-fingerprint snapshot.
// =======================================================================

struct ModelResolutionCase {
    name: &'static str,
    /// `(default_harness, default_model)` set on the project, if any.
    project_default: Option<(&'static str, &'static str)>,
    requested: Option<&'static str>,
    expected: Option<&'static str>,
}

/// Runs one [`ModelResolutionCase`] end to end and asserts its expectation —
/// factored out of the table-driven test so each case reads as one call.
async fn assert_model_resolution_case(i: usize, case: ModelResolutionCase) {
    let (app, repo, item_id, project_id) = setup().await;
    if let Some((harness, model)) = case.project_default {
        tack_test_support::seed::project_default_model(&repo, &project_id, harness, model).await;
    }
    let agent_profile_id = create_agent_profile(&app, "profile").await;
    let (runner_id, _auth) = enroll_runner(&app, "runner").await;

    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            &format!("model-precedence-{i}"),
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            case.requested,
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}: {created}", case.name);
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let stored = stored_requested_model(&repo, &request_id).await;
    assert_eq!(stored.as_deref(), case.expected, "{}", case.name);
    // The idempotency-fingerprint snapshot must agree with the stored
    // column — no split-brain between the two. Absence persists as NULL,
    // never a placeholder string.
    let snapshot = stored_requested_model_snapshot(&repo, &request_id).await;
    assert_eq!(
        snapshot["requested_model"],
        json!(case.expected),
        "{}",
        case.name
    );
}

#[tokio::test]
async fn create_execution_resolves_model_by_precedence_tier() {
    let cases = [
        ModelResolutionCase {
            name: "the project default applies to the project's default agent",
            project_default: Some(("codex", "opaque/model-f6b")),
            requested: None,
            expected: Some("opaque/model-f6b"),
        },
        ModelResolutionCase {
            name: "an explicit client choice overrides the project default",
            project_default: Some(("codex", "opaque/should-never-be-used")),
            requested: Some("opaque/model-f6b"),
            expected: Some("opaque/model-f6b"),
        },
        ModelResolutionCase {
            name: "a run on another agent than the project's default gets no project model",
            project_default: Some(("claude-code", "opaque/should-never-be-used")),
            requested: None,
            expected: None,
        },
        ModelResolutionCase {
            name: "no model anywhere persists as NULL, never a placeholder string",
            project_default: None,
            requested: None,
            expected: None,
        },
    ];

    for (i, case) in cases.into_iter().enumerate() {
        assert_model_resolution_case(i, case).await;
    }
}

// =======================================================================
// `GET /api/executions/{id}/attempts` carries honest
// `model_provenance`/`usage_economics` — proved end to end through
// claim → accept → start → completion.
// =======================================================================

struct LiveAttempt {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    runner_auth: [(String, String); 1],
    agent_profile_id: String,
    fencing_token: i64,
}

/// Posts a completion for `live`'s attempt, reporting `model` as the
/// actual model the harness ran, with the given `usage`.
async fn complete_attempt(
    app: &axum::Router,
    live: &LiveAttempt,
    completion_id: &str,
    model: &str,
    usage: Value,
) -> (StatusCode, Value) {
    common::send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", live.attempt_id),
        completion_body(
            &live.runner_id,
            &live.attempt_id,
            live.fencing_token,
            completion_id,
            model,
            usage,
        ),
        &headers_ref(&live.runner_auth),
    )
    .await
}

/// Drives an execution request through claim, accept and start — stopping
/// short of completion so callers can inspect the in-flight
/// `model_provenance`/`usage_economics` shape (still honestly absent) before
/// choosing how to complete it.
async fn claim_accept_start(
    app: &axum::Router,
    item_id: &str,
    idempotency_key: &str,
    requested_model: &str,
) -> LiveAttempt {
    let (runner_id, runner_auth_owned) = enroll_runner(app, "provenance runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);
    let agent_profile_id = create_agent_profile(app, "provenance profile").await;

    let (status, created) = common::send(
        app,
        "POST",
        "/api/executions",
        execution_request_body(
            item_id,
            idempotency_key,
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            Some(requested_model),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed) = common::send(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{idempotency_key}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = common::send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wiring", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wiring", "base_revision": BASE_REVISION, "process_id": "pid-wiring"}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    LiveAttempt {
        request_id,
        attempt_id,
        runner_id,
        runner_auth: runner_auth_owned,
        agent_profile_id,
        fencing_token,
    }
}

/// The `usage` shape for a completion whose harness never reported real
/// token/duration/cost numbers — every dimension `not_measured`, not zero.
fn not_measured_usage() -> Value {
    json!({
        "tokens_in": {"value": null, "source": "not_measured"},
        "tokens_out": {"value": null, "source": "not_measured"},
        "duration_ms": {"value": null, "source": "not_measured"},
        "cost_usd": {"value": null, "source": "not_measured"},
    })
}

fn completion_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    completion_id: &str,
    actual_model: &str,
    usage: Value,
) -> Value {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "completion_id": completion_id,
        "terminal_state": "succeeded",
        "terminal_reason": {"code": "completed", "message": "Harness exited successfully"},
        "actual_execution": {
            "harness_kind": "codex",
            "harness_version": "1.0.0",
            "model": actual_model,
            "model_source": "reported",
            "capability_snapshot": {
                "cancel": {"support": "advisory", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resumable session contract"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "usage may be absent"},
            },
            "workspace_id": "ws-wiring",
            "base_revision": BASE_REVISION,
            "started_at": "2026-08-08T12:00:00Z",
            "ended_at": "2026-08-08T12:05:00Z",
        },
        "usage": usage,
        "final_event_checkpoint": null,
    })
}

async fn get_attempts(app: &axum::Router, request_id: &str) -> Value {
    let (status, body) = common::send(
        app,
        "GET",
        &format!("/api/executions/{request_id}/attempts"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn attempt_summary_hides_provenance_and_costs_before_completion() {
    let (app, _repo, item_id, _project_id) = setup().await;
    let live = claim_accept_start(&app, &item_id, "provenance-in-flight", "opaque/model-f6b").await;

    let body = get_attempts(&app, &live.request_id).await;
    let data = body["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    let attempt = &data[0];
    assert_eq!(
        attempt["model_provenance"],
        Value::Null,
        "no actual_execution has been reported yet — provenance must be honestly absent, not guessed"
    );
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["wall_clock_ms"],
        Value::Null,
        "ended_at is not yet known — wall_clock_ms must stay null, never a fabricated partial duration"
    );
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["cost_usd_estimated"],
        json!({"value": null, "source": "not_measured"})
    );
    assert_eq!(
        attempt["usage_economics"]["model_token_cost_usd_estimated"],
        json!({"value": null, "source": "not_measured"})
    );
}

struct ProvenanceCase {
    name: &'static str,
    key: &'static str,
    actual_model_id: &'static str,
    usage: Value,
    expected_provenance: Value,
    expected_cost: Value,
}

/// Runs one [`ProvenanceCase`] end to end (claim → accept → start →
/// completion) and asserts its expected `model_provenance`/cost shape,
/// plus the cross-cutting invariants that hold regardless of which case:
/// the runner-time dollar dimension always stays `not_measured` (no
/// infra cost-rate exists in this schema, so it must never borrow the
/// harness's own figure or default to 0), and `wall_clock_ms` is computed
/// independently here — not trusted from the handler's own arithmetic —
/// and cross-checked against the database.
async fn assert_provenance_case(case: ProvenanceCase) {
    let (app, repo, item_id, _project_id) = setup().await;
    let live = claim_accept_start(&app, &item_id, case.key, "opaque/model-f6b").await;

    let (status, completed) = complete_attempt(
        &app,
        &live,
        &format!("{}-completion", case.key),
        case.actual_model_id,
        case.usage,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}: {completed}", case.name);

    let body = get_attempts(&app, &live.request_id).await;
    let attempt = &body["data"].as_array().expect("data array")[0];
    assert_eq!(
        attempt["model_provenance"], case.expected_provenance,
        "{}",
        case.name
    );
    assert_eq!(
        attempt["usage_economics"]["model_token_cost_usd_estimated"], case.expected_cost,
        "{}",
        case.name
    );

    let runner_cost = &attempt["usage_economics"]["runner_time_cost"]["cost_usd_estimated"];
    assert_eq!(
        runner_cost["value"],
        Value::Null,
        "{}: must be null, not 0",
        case.name
    );
    assert_eq!(runner_cost["source"], "not_measured");

    let started_at: DateTime<Utc> = attempt["started_at"].as_str().unwrap().parse().unwrap();
    let ended_at: DateTime<Utc> = attempt["ended_at"].as_str().unwrap().parse().unwrap();
    let expected_wall_clock_ms = ended_at
        .signed_duration_since(started_at)
        .num_milliseconds();
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["wall_clock_ms"],
        json!(expected_wall_clock_ms),
        "{}",
        case.name
    );
    let (db_started, db_ended) = execution_attempts::started_and_ended_at(&repo, &live.attempt_id)
        .await
        .unwrap()
        .expect("attempt row");
    assert!(db_started.is_some() && db_ended.is_some(), "{}", case.name);
}

#[tokio::test]
async fn attempt_summary_reports_provenance_and_cost_after_completion() {
    let cases = [
        ProvenanceCase {
            name: "matched: requested and actual model agree",
            key: "matched",
            actual_model_id: "opaque/model-f6b",
            usage: json!({
                "tokens_in": {"value": 1000, "source": "measured"},
                "tokens_out": {"value": 500, "source": "measured"},
                "duration_ms": {"value": 300000, "source": "measured"},
                "cost_usd": {"value": 0.42, "source": "measured"},
            }),
            expected_provenance: json!({"requested": "opaque/model-f6b", "served": "opaque/model-f6b", "source": "reported"}),
            expected_cost: json!({"value": 0.42, "source": "measured"}),
        },
        ProvenanceCase {
            name: "mismatched: the harness ran a different declared model than requested",
            key: "mismatched",
            actual_model_id: "opaque/model-f6b-mismatch",
            usage: not_measured_usage(),
            expected_provenance: json!({
                "requested": "opaque/model-f6b",
                "served": "opaque/model-f6b-mismatch",
                "source": "reported",
            }),
            expected_cost: json!({"value": null, "source": "not_measured"}),
        },
    ];

    for case in cases {
        assert_provenance_case(case).await;
    }
}

/// An item runs one attempt through to a terminal state, then the *same*
/// item is enqueued again with a fresh, never-used idempotency key, reusing
/// the same runner and agent profile a retry through the Run-with-agent
/// modal would. No invariant treats "this item already has a finished
/// attempt" as a reason to refuse a new request — the second call must
/// succeed exactly like the first.
#[tokio::test]
async fn create_execution_succeeds_for_item_with_a_finished_attempt() {
    let (app, _repo, item_id, _project_id) = setup().await;
    let live = claim_accept_start(&app, &item_id, "repeat-first", "opaque/model-f6b").await;

    let (status, completed) = complete_attempt(
        &app,
        &live,
        "repeat-completion",
        "opaque/model-f6b",
        not_measured_usage(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    // The item's one attempt is now terminal (`succeeded`). A fresh
    // idempotency key against the same item, same runner, same profile.
    let (status2, second) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "repeat-second",
            "exact_runner",
            &live.runner_id,
            &live.agent_profile_id,
            Some("opaque/model-f6b"),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(
        status2,
        StatusCode::OK,
        "second enqueue for an item with a finished attempt must not fail: {second}"
    );
    assert_eq!(second["state"], "queued");
    assert_eq!(second["replayed"], false);
}
