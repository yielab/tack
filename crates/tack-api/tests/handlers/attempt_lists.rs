//! Handler tests for `GET /api/executions/{id}/attempts/{n}/artifacts` and
//! `.../decisions` (`crate::handlers::attempt_lists`): an unauthenticated
//! caller is rejected, and the two routes' ordering matches the handler's
//! own query guarantee. Each test runs once per [`ResourceKind`] since the
//! two routes are structurally identical. Rows are seeded with
//! `tack_test_support::seed`, not the runner-protocol write routes — the write
//! side is exercised in `runner_protocol/artifact_events.rs` and
//! `runner_protocol/decisions.rs`; this file only covers what the two
//! *read* routes return, so a claimed (`leased`) attempt is enough.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::handlers::executions::TerminalReason;
use tack_api::{AppState, router::build_router};
use tack_db::Repository;
use tack_protocol::execution::ExecutionState;
use tack_test_support::{create_test_workspace, seed, setup_test_db};

const OPERATOR_TOKEN: &str = "c5-attempt-lists-operator-token";
const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

async fn setup() -> (axum::Router, Repository, String) {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
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
        tasks: Default::default(),
    };
    let app = crate::common::with_default_host(build_router(state));
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "C5 attempt lists".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/c5.git".into()),
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
                title: "Prove the attempt-list routes".into(),
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
    (app, repo, item.id.to_string())
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer c5-attempt-lists-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn full_capabilities() -> Value {
    let now = chrono::Utc::now().to_rfc3339();
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

/// Enrolls a runner and returns (runner_id, bearer-auth-header-pair).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending, _) = common::send_with_raw(
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

    let (status, enrolled, _) = common::send_with_raw(
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

/// `label` keeps the profile name unique per call — a test that stands up
/// two separate executions (the cross-execution not-found tests) calls this
/// twice, and profile names are unique.
async fn create_agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile, _) = common::send_with_raw(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("C6 {label} profile"), "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

fn execution_request_body(
    item_id: &str,
    key: &str,
    runner_id: &str,
    agent_profile_id: &str,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": "exact_runner",
        "selector_id": runner_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model": "opaque/model-c5",
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/c5.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

/// Creates an execution request and claims it, returning the operator-facing
/// `request_id` and the internal `attempt_id` a direct fixture insert needs.
async fn request_and_claim(app: &axum::Router, item_id: &str, label: &str) -> (String, String) {
    let claimed = claim_as_runner(app, item_id, label).await;
    (claimed.request_id, claimed.attempt_id)
}

struct ClaimedAs {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    fencing_token: i64,
    auth: [(String, String); 1],
}

/// [`request_and_claim`], keeping what the owning runner needs to report on
/// the attempt afterward.
async fn claim_as_runner(app: &axum::Router, item_id: &str, label: &str) -> ClaimedAs {
    let (runner_id, auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(app, label).await;

    let (status, created, _) = common::send_with_raw(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed, _) = common::send_with_raw(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{label}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    ClaimedAs {
        request_id,
        attempt_id,
        runner_id,
        fencing_token,
        auth: auth_owned,
    }
}

/// The two list routes below (`.../artifacts` and `.../decisions`) are
/// structurally identical, so every test in this file runs once per kind
/// rather than existing as two near-duplicate functions.
#[derive(Clone, Copy)]
enum ResourceKind {
    Artifact,
    Decision,
}

impl ResourceKind {
    fn route_segment(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifacts",
            ResourceKind::Decision => "decisions",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifact",
            ResourceKind::Decision => "decision",
        }
    }

    fn id_field(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifact_id",
            ResourceKind::Decision => "decision_id",
        }
    }

    async fn insert(self, repo: &Repository, attempt_id: &str, id: &str, created_at: &str) {
        match self {
            ResourceKind::Artifact => {
                seed::execution_artifact(repo, attempt_id, id, created_at).await
            }
            ResourceKind::Decision => {
                seed::execution_decision(repo, attempt_id, id, created_at).await
            }
        }
    }
}

/// Creates an execution request but never claims it, so its
/// `execution_attempts` table stays empty — the "another execution exists,
/// but this one never claimed an attempt" half of the cross-execution
/// not-found tests below.
async fn request_without_claiming(app: &axum::Router, item_id: &str, label: &str) -> String {
    let (runner_id, _auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let agent_profile_id = create_agent_profile(app, label).await;
    let (status, created, _) = common::send_with_raw(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    created["request_id"].as_str().unwrap().to_owned()
}

// =======================================================================
// GET /api/executions/{request_id}/attempts/{attempt_number}/{artifacts,decisions}
// =======================================================================

/// Asserts an unauthenticated `GET` of `kind`'s list is 401 and leaks
/// nothing of the seeded row — not the status code alone, which would not
/// catch a gate that rejects but still lets the handler run.
async fn assert_unauth_leaks_nothing(
    app: &axum::Router,
    request_id: &str,
    kind: ResourceKind,
    seeded_id: &str,
) {
    let (status, body, raw) = common::send_with_raw(
        app,
        "GET",
        &format!(
            "/api/executions/{request_id}/attempts/1/{}",
            kind.route_segment()
        ),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "{}: {body}",
        kind.route_segment()
    );
    assert!(
        body.get("data").is_none(),
        "{}: unauthenticated response must carry no data field: {body}",
        kind.route_segment()
    );
    assert!(
        !raw.contains(seeded_id),
        "{}: unauthenticated response must not leak the seeded id: {raw}",
        kind.route_segment()
    );
}

#[tokio::test]
async fn attempt_list_requires_auth_and_leaks_nothing_without_it() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        let label = format!("{}-auth", kind.route_segment());
        let (request_id, attempt_id) = request_and_claim(&app, &item_id, &label).await;
        let seeded_id = format!("auth-check-{}", kind.noun());
        kind.insert(&repo, &attempt_id, &seeded_id, "2026-01-01T00:00:00Z")
            .await;

        assert_unauth_leaks_nothing(&app, &request_id, kind, &seeded_id).await;
    }
}

#[tokio::test]
async fn attempt_list_is_empty_before_any_row() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, _repo, item_id) = setup().await;
        let label = format!("{}-empty", kind.route_segment());
        let (request_id, _attempt_id) = request_and_claim(&app, &item_id, &label).await;

        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", kind.route_segment());
        assert_eq!(body["data"], json!([]), "{}", kind.route_segment());
    }
}

#[tokio::test]
async fn attempt_list_is_returned_oldest_first() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        let label = format!("{}-order", kind.route_segment());
        let (request_id, attempt_id) = request_and_claim(&app, &item_id, &label).await;

        // Inserted out of chronological order on purpose: "newer" lands in
        // the table first but is stamped with the later `created_at`. If the
        // route ever returned rows in insertion/rowid order instead of the
        // handler's actual `ORDER BY created_at`, this would come back
        // [newer, older] and the assertion below would catch it.
        let short = &kind.noun()[..3];
        let newer = format!("{short}-newer");
        let older = format!("{short}-older");
        kind.insert(&repo, &attempt_id, &newer, "2026-01-02T00:00:00Z")
            .await;
        kind.insert(&repo, &attempt_id, &older, "2026-01-01T00:00:00Z")
            .await;

        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", kind.route_segment());
        let data = body["data"].as_array().expect("data array");
        assert_eq!(data.len(), 2, "{}", kind.route_segment());
        assert_eq!(data[0][kind.id_field()], older, "{}", kind.route_segment());
        assert_eq!(data[1][kind.id_field()], newer, "{}", kind.route_segment());
    }
}

#[tokio::test]
async fn artifact_and_decision_lists_404_for_an_unknown_attempt() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, _repo, item_id) = setup().await;
        let label = format!("{}-unknown-n", kind.route_segment());
        let (request_id, _attempt_id) = request_and_claim(&app, &item_id, &label).await;

        // Only attempt 1 was ever claimed for this request; attempt 99 never
        // existed for anyone.
        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/99/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{}: {body}",
            kind.route_segment()
        );
        assert_eq!(
            body["error"]["details"]["resource"],
            "execution_attempt",
            "{}",
            kind.route_segment()
        );
    }
}

/// Asserts a foreign execution's `GET` of `kind`'s list 404s as
/// `execution_attempt` and leaks nothing of the other execution's row — not
/// the status code alone, which would not catch a query that scopes only by
/// attempt_number and forgets request_id.
async fn assert_404_for_foreign_execution(
    app: &axum::Router,
    request_id: &str,
    kind: ResourceKind,
    seeded_id: &str,
) {
    let (status, body, raw) = common::send_with_raw(
        app,
        "GET",
        &format!(
            "/api/executions/{request_id}/attempts/1/{}",
            kind.route_segment()
        ),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "{}: {body}",
        kind.route_segment()
    );
    assert_eq!(
        body["error"]["details"]["resource"],
        "execution_attempt",
        "{}",
        kind.route_segment()
    );
    assert!(
        !raw.contains(seeded_id),
        "{}: must not leak another execution's {}: {raw}",
        kind.route_segment(),
        kind.noun()
    );
}

#[tokio::test]
async fn artifact_and_decision_lists_404_for_a_foreign_execution() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        // Execution X claims attempt 1 and writes a real row against it.
        let owner_label = format!("{}-cross-owner", kind.route_segment());
        let (_other_request_id, other_attempt_id) =
            request_and_claim(&app, &item_id, &owner_label).await;
        let seeded_id = format!("cross-execution-{}", kind.noun());
        kind.insert(&repo, &other_attempt_id, &seeded_id, "2026-01-01T00:00:00Z")
            .await;
        // Execution Y is real but never claimed anything — it has no
        // attempt 1 of its own.
        let caller_label = format!("{}-cross-caller", kind.route_segment());
        let request_id = request_without_claiming(&app, &item_id, &caller_label).await;

        assert_404_for_foreign_execution(&app, &request_id, kind, &seeded_id).await;
    }
}

#[test]
fn terminal_reason_round_trips_typed() {
    // Table of test cases: (description, input_json)
    let cases = vec![
        (
            "harness_rejected (user's failed attempt)",
            json!({"code":"harness_rejected","message":"requested harness kind \"x\" does not match this adapter's kind \"claude-code\""}),
        ),
        (
            "completed with result",
            json!({"code":"completed","message":"Harness exited successfully","result":"..."}),
        ),
        (
            "completed with artifacts",
            json!({"code":"completed","message":"Task complete","artifacts":[{"kind":"patch","size_bytes":123}]}),
        ),
        (
            "workspace_kept_at after recovery",
            json!({"code":"recovered","message":"Recovered from checkpoint","workspace_kept_at":"2026-10-07T10:30:00Z"}),
        ),
        ("empty terminal_reason", json!({})),
    ];

    for (description, input_json) in cases {
        // Deserialize JSON → TerminalReason
        let deserialized: TerminalReason = serde_json::from_value(input_json.clone())
            .unwrap_or_else(|_| panic!("Failed to deserialize {}", description));

        // Serialize TerminalReason → JSON
        let serialized = serde_json::to_value(&deserialized)
            .unwrap_or_else(|_| panic!("Failed to serialize {}", description));

        // Verify round-trip preserves all values (input_json must be a subset of serialized due to flatten)
        assert_eq!(
            serialized, input_json,
            "Round-trip mismatch for '{}': {:?} != {:?}",
            description, serialized, input_json
        );
    }
}

/// T4a: a succeeded attempt that left a patch makes its item "need review";
/// a verdict clears it, shows on the attempt, and a second verdict is a 409.
#[tokio::test]
async fn a_reviewed_attempt_clears_needs_review() {
    let (app, repo, item_id) = setup().await;
    let (request_id, attempt_id) = request_and_claim(&app, &item_id, "review").await;
    seed::execution_attempt_ended(
        &repo,
        &attempt_id,
        ExecutionState::Succeeded,
        &json!({"artifacts": [{"kind": "patch", "size_bytes": 12}]}).to_string(),
    )
    .await;

    let item_path = format!("/api/items/{item_id}");
    let review_path = format!("/api/executions/{request_id}/attempts/1/review");
    let needs_review = |app: axum::Router, path: String| async move {
        let (status, body, _) =
            common::send_with_raw(&app, "GET", &path, Value::Null, &operator_headers()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["item"]["needs_review"]
            .as_bool()
            .expect("needs_review bool")
    };
    assert!(needs_review(app.clone(), item_path.clone()).await);

    for (verdict, expected) in [
        ("maybe", StatusCode::BAD_REQUEST),
        ("accepted", StatusCode::OK),
        ("rejected", StatusCode::CONFLICT),
    ] {
        let (status, body, _) = common::send_with_raw(
            &app,
            "POST",
            &review_path,
            json!({"verdict": verdict, "note": "looks right"}),
            &operator_headers(),
        )
        .await;
        assert_eq!(status, expected, "{verdict}: {body}");
        if expected == StatusCode::CONFLICT {
            assert_eq!(body["error"]["code"], "conflict", "{body}");
            assert_eq!(
                body["error"]["details"]["reason"], "already_reviewed",
                "{body}"
            );
        }
    }
    assert!(!needs_review(app.clone(), item_path).await);

    let (_, listed, _) = common::send_with_raw(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    let review = &listed["data"][0]["review"];
    assert_eq!(review["verdict"], "accepted", "{listed}");
    assert_eq!(review["note"], "looks right", "{listed}");
    assert!(review["reviewed_at"].is_string(), "{listed}");
}

/// What the runner said at start (`environment_names`, `sandbox`) reads back on
/// the attempt: names as sent, an absent report as `[]`/null, and a sandbox
/// outside `bwrap`/`none` as null.
#[tokio::test]
async fn the_start_report_reads_back_on_the_attempt() {
    let cases = [
        (
            "kept",
            json!({"environment_names": ["B", "A", 7, null], "sandbox": "bwrap"}),
            json!(["B", "A"]),
            json!("bwrap"),
        ),
        ("none", json!({"sandbox": "none"}), json!([]), json!("none")),
        ("absent", json!({}), json!([]), Value::Null),
        (
            "bogus",
            json!({"environment_names": "A", "sandbox": "rm -rf"}),
            json!([]),
            Value::Null,
        ),
        (
            "capped",
            json!({"environment_names": (0..300).map(|i| format!("V{i}")).collect::<Vec<_>>()}),
            json!((0..256).map(|i| format!("V{i}")).collect::<Vec<_>>()),
            Value::Null,
        ),
        (
            "long name dropped",
            json!({"environment_names": ["A".repeat(129), "B".repeat(128)]}),
            json!(["B".repeat(128)]),
            Value::Null,
        ),
    ];
    for (label, extra, names, sandbox) in cases {
        let (app, _repo, item_id) = setup().await;
        let claimed = claim_as_runner(&app, &item_id, "start-report").await;
        let auth = headers_ref(&claimed.auth);
        let mut base = json!({
            "protocol_version": 1,
            "runner_id": claimed.runner_id,
            "attempt_id": claimed.attempt_id,
            "fencing_token": claimed.fencing_token,
            "workspace_id": "ws-1",
            "base_revision": BASE_REVISION,
        });
        let (status, accepted, _) = common::send_with_raw(
            &app,
            "POST",
            &format!("/api/runner/v1/attempts/{}/accept", claimed.attempt_id),
            base.clone(),
            &auth,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{label}: {accepted}");
        base["process_id"] = json!("pid-1");
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        let (status, started, _) = common::send_with_raw(
            &app,
            "POST",
            &format!("/api/runner/v1/attempts/{}/start", claimed.attempt_id),
            base,
            &auth,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{label}: {started}");

        let (_, listed, _) = common::send_with_raw(
            &app,
            "GET",
            &format!("/api/executions/{}/attempts", claimed.request_id),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(listed["data"][0]["environment_names"], names, "{label}");
        assert_eq!(listed["data"][0]["sandbox"], sandbox, "{label}");
    }
}

/// A harness's `sandbox` and `sandbox_reason`, reported at enrollment, are
/// returned in the runner's capabilities.
#[tokio::test]
async fn a_harness_sandbox_report_shows_in_the_runner_capabilities() {
    let (app, _repo, _item_id) = setup().await;
    let (status, pending, _) = common::send_with_raw(
        &app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": "sandbox runner", "total_capacity": 1, "available_capacity": 1}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let mut capabilities = full_capabilities();
    capabilities["harnesses"][0]["sandbox"] = json!("none");
    capabilities["harnesses"][0]["sandbox_reason"] = json!("x");
    let (status, enrolled, _) = common::send_with_raw(
        &app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": pending["enrollment_token"],
            "runner_name": "sandbox runner",
            "runner_version": "0.1.0",
            "capabilities": capabilities,
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");

    let (_, runners, _) = common::send_with_raw(
        &app,
        "GET",
        "/api/runners",
        Value::Null,
        &operator_headers(),
    )
    .await;
    let harness = &runners["data"][0]["capability_snapshot"]["harnesses"][0];
    assert_eq!(harness["sandbox"], "none", "{runners}");
    assert_eq!(harness["sandbox_reason"], "x", "{runners}");
}

/// The attempt's counted figure is the one rule applied to its stored usage:
/// input less the cache reads, plus output; `null` when nothing was measured.
#[tokio::test]
async fn the_attempt_list_carries_the_counted_tokens() {
    let measured = |value: u64| json!({"value": value, "source": "measured"});
    let unmeasured = json!({"value": null, "source": "not_measured"});
    let cases = [
        (
            "cache reads are left out",
            Some(
                json!({"tokens_in": measured(1_000_000), "tokens_out": measured(9_000),
                "duration_ms": unmeasured, "cost_usd": unmeasured, "cache_read_tokens": 412_000}),
            ),
            json!(597_000),
        ),
        (
            "no cache figure counts the whole input",
            Some(
                json!({"tokens_in": measured(48_000), "tokens_out": measured(210),
                "duration_ms": unmeasured, "cost_usd": unmeasured}),
            ),
            json!(48_210),
        ),
        (
            "nothing measured is null, not zero",
            Some(json!({"tokens_in": unmeasured, "tokens_out": unmeasured,
                "duration_ms": unmeasured, "cost_usd": unmeasured})),
            Value::Null,
        ),
        ("no usage reported is null", None, Value::Null),
    ];
    for (label, usage, counted) in cases {
        let (app, repo, item_id) = setup().await;
        let (request_id, attempt_id) = request_and_claim(&app, &item_id, "counted").await;
        if let Some(usage) = &usage {
            seed::execution_attempt_usage(&repo, &attempt_id, &usage.to_string()).await;
        }
        let (status, listed, _) = common::send_with_raw(
            &app,
            "GET",
            &format!("/api/executions/{request_id}/attempts"),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{label}: {listed}");
        assert_eq!(listed["data"][0]["counted_tokens"], counted, "{label}");
    }
}
