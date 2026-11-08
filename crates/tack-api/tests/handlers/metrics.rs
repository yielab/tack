//! HTTP test for `GET /api/projects/{id}/metrics/factory`: rows are seeded
//! directly, and every number the endpoint returns is worked out by hand here.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::sync::Arc;
use tack_api::config::AppConfig;
use tack_api::{
    AppState, LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
    router::build_router,
};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::Repository;
use tack_test_support::{create_test_workspace, seed, setup_test_db};
use uuid::Uuid;

const TOKEN: &str = "g1a-metrics-token";
const MRP: &str = "application/vnd.tack.mrp+json";

fn close(value: &Value, expected: f64) {
    let got = value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"));
    assert!((got - expected).abs() < 1e-9, "{got} != {expected}");
}

/// An embedded runner whose `[verify]` names `program` as its prill.
struct RunnerVerifyingWith(String);

#[async_trait::async_trait]
impl LocalRunnerControl for RunnerVerifyingWith {
    async fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: RuntimeState::Stopped,
            since: None,
            max_concurrent_attempts: 1,
            active_attempts: 0,
        }
    }
    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        Ok(())
    }
    async fn stop(&self) {}
    async fn runner_id(&self) -> Option<String> {
        None
    }
    async fn resolve_secret(&self, _reference: &str) -> Result<String, LocalRunnerControlError> {
        Err(LocalRunnerControlError::SecretStore("none".into()))
    }
    async fn verify_program(&self) -> Option<String> {
        Some(self.0.clone())
    }
}

async fn project_with_item(repo: &Repository, workspace_id: Uuid, name: &str) -> (Uuid, String) {
    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: name.into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    let item = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                recurrence: None,
                recurrence_of: None,
                title: "work".into(),
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
        .unwrap();
    (project.id, item.id.to_string())
}

fn usage(tokens_in: i64, tokens_out: i64) -> String {
    json!({
        "tokens_in": {"value": tokens_in, "source": "measured"},
        "tokens_out": {"value": tokens_out, "source": "measured"},
    })
    .to_string()
}

#[tokio::test]
async fn seeded_rows_yield_the_expected_ratios() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app = crate::common::with_default_host(build_router(AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    }));
    let (project_id, item) = project_with_item(&repo, workspace_id, "G1a seeded").await;
    let (empty_id, _) = project_with_item(&repo, workspace_id, "G1a empty").await;

    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'g1a', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;
    // Requests: R1 first work, R2 rework of R1, R3 first work (one old attempt),
    // R4 first work whose usage was never reported.
    for (id, metadata) in [
        ("R1", "{}"),
        ("R2", r#"{"rework_of":"R1"}"#),
        ("R3", "{}"),
        ("R4", "{}"),
    ] {
        seed::statement(
            &repo,
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
               request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
               repository_snapshot, permission_policy, metadata, created_at, updated_at)
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', ?,
               '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            &[id, &item, id, metadata],
        )
        .await;
    }
    // (attempt id, request, number, usage, created_at). Window starts 2026-09-01.
    let attempts = [
        (
            "R1a1",
            "R1",
            "1",
            Some(usage(100, 50)),
            "2026-09-10T09:00:00Z",
        ),
        (
            "R1a2",
            "R1",
            "2",
            Some(usage(30, 20)),
            "2026-09-10T10:00:00Z",
        ),
        (
            "R2a1",
            "R2",
            "1",
            Some(usage(40, 10)),
            "2026-09-10T11:00:00Z",
        ),
        (
            "R3a1",
            "R3",
            "1",
            Some(usage(200, 100)),
            "2026-09-11T09:00:00Z",
        ),
        (
            "R3a0",
            "R3",
            "2",
            Some(usage(500, 499)),
            "2026-08-20T09:00:00Z",
        ),
        ("R4a1", "R4", "1", None, "2026-09-12T09:00:00Z"),
    ];
    for (id, request, number, usage, created) in &attempts {
        let usage = usage.clone().unwrap_or_default();
        let sql = if usage.is_empty() {
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
               lease_issued_at, lease_expires_at, created_at, updated_at)
             VALUES (?, ?, ?, 'run', ?, ?, ?, ?, ?)"
        } else {
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
               lease_issued_at, lease_expires_at, created_at, updated_at, usage)
             VALUES (?, ?, ?, 'run', ?, ?, ?, ?, ?, ?)"
        };
        let mut binds = vec![
            *id, *request, *number, *number, *created, *created, *created, *created,
        ];
        if !usage.is_empty() {
            binds.push(&usage);
        }
        seed::statement(&repo, sql, &binds).await;
    }
    // Decisions: D1 viewed (10 min), D2 never viewed (30 min), D3 pending, D4 before the window.
    for (id, attempt, viewed, created, resolved) in [
        (
            "D1",
            "R1a1",
            Some("2026-09-10T10:00:00Z"),
            "2026-09-10T09:50:00Z",
            Some("2026-09-10T10:10:00Z"),
        ),
        (
            "D2",
            "R1a1",
            None,
            "2026-09-10T11:00:00Z",
            Some("2026-09-10T11:30:00Z"),
        ),
        ("D3", "R1a2", None, "2026-09-10T12:00:00Z", None),
        (
            "D4",
            "R3a0",
            None,
            "2026-08-20T09:00:00Z",
            Some("2026-08-20T09:05:00Z"),
        ),
    ] {
        seed::statement(
            &repo,
            "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, created_at,
               updated_at, viewed_at, resolved_at) VALUES (?, ?, ?, 'choice', 'p', ?, ?, ?, ?)",
            &[
                id,
                attempt,
                id,
                created,
                created,
                viewed.unwrap_or_default(),
                resolved.unwrap_or_default(),
            ],
        )
        .await;
    }
    seed::statement(
        &repo,
        "UPDATE execution_decisions SET viewed_at = NULLIF(viewed_at, ''), resolved_at = NULLIF(resolved_at, '')",
        &[],
    )
    .await;
    // Packs: R1a1 (7+3 tokens), R3a1 (20+10), R4a1 (content never uploaded).
    for (attempt, reference, body, created) in [
        (
            "R1a1",
            Some("p1.json"),
            Some(r#"{"usage":{"tokens_in":7,"tokens_out":3}}"#),
            "2026-09-10T11:50:00Z",
        ),
        (
            "R3a1",
            Some("p3.json"),
            Some(r#"{"usage":{"tokens_in":20,"tokens_out":10}}"#),
            "2026-09-11T13:00:00Z",
        ),
        ("R4a1", None, None, "2026-09-12T10:00:00Z"),
    ] {
        if let (Some(reference), Some(body)) = (reference, body) {
            let dir = root.path().join("execution-artifacts");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(reference), body).unwrap();
        }
        seed::statement(
            &repo,
            "INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type,
               size_bytes, sha256, content_reference, created_at)
             VALUES (?, ?, 'mrp-1', 'report', 'mrp.json', ?, 1, 'x', NULLIF(?, ''), ?)",
            &[
                &format!("x-{attempt}"),
                attempt,
                MRP,
                reference.unwrap_or_default(),
                created,
            ],
        )
        .await;
    }
    // Reviews: R1a1 accepted after a view (5 min), R3a1 rejected unviewed (30 min); R4a1 unreviewed.
    seed::statement(
        &repo,
        "INSERT INTO attempt_reviews (attempt_id, artifact_id, verdict, reason, reviewed_at) VALUES
           ('R1a1', 'mrp-1', 'accepted', NULL, '2026-09-10T12:05:00Z'),
           ('R3a1', 'mrp-1', 'rejected', 'broken', '2026-09-11T13:30:00Z')",
        &[],
    )
    .await;
    seed::statement(
        &repo,
        "UPDATE execution_artifacts SET viewed_at = '2026-09-10T12:00:00Z' WHERE attempt_id = 'R1a1'",
        &[],
    )
    .await;
    // Pull requests: merged, closed, reverted, open, and one merged before the window.
    for (attempt, number, state, opened) in [
        ("R1a1", "1", "merged", "2026-09-10T13:00:00Z"),
        ("R3a1", "2", "closed", "2026-09-11T14:00:00Z"),
        ("R1a2", "3", "reverted", "2026-09-10T14:00:00Z"),
        ("R2a1", "4", "open", "2026-09-10T15:00:00Z"),
        ("R3a0", "5", "merged", "2026-08-20T10:00:00Z"),
    ] {
        seed::statement(
            &repo,
            "INSERT INTO pull_requests (attempt_id, repo, number, url, state, opened_at)
             VALUES (?, 'o/r', ?, 'u', ?, ?)",
            &[attempt, number, state, opened],
        )
        .await;
    }

    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory?since=2026-09-01T00:00:00Z");
    let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{m}");

    // 5 attempts in the window, 3 decisions: 3/5.
    assert_eq!(
        (m["attempts"].as_i64(), m["decisions"].as_i64()),
        (Some(5), Some(3))
    );
    close(&m["escalation_rate"]["value"], 0.6);
    // Decisions: D1 10 min from viewed_at, D2 30 min from created_at; D3 unresolved, D4 old.
    let d = &m["human_minutes_per_decision"];
    assert_eq!(
        (
            d["measured"].as_i64(),
            d["start_viewed_at"].as_i64(),
            d["start_created_at"].as_i64()
        ),
        (Some(2), Some(1), Some(1))
    );
    close(&d["median_minutes"], 20.0);
    close(&d["p90_minutes"], 30.0);
    // Packs: 5 min from viewed_at, 30 min from the artifact's created_at.
    let p = &m["human_minutes_per_mrp"];
    assert_eq!(
        (
            p["measured"].as_i64(),
            p["start_viewed_at"].as_i64(),
            p["start_created_at"].as_i64()
        ),
        (Some(2), Some(1), Some(1))
    );
    close(&p["median_minutes"], 17.5);
    close(&p["p90_minutes"], 30.0);
    // implementation 150 + 300 + 0 (R4 unreported) = 450; rework 50 + 50 = 100;
    // verification 10 + 30 = 40; (40 + 100) / 450.
    let t = &m["verification_tax"];
    assert_eq!(t["implementation_tokens"], 450);
    assert_eq!(t["implementation_attempts"], 3);
    assert_eq!(t["rework_tokens"], 100);
    assert_eq!(t["rework_attempts"], 2);
    assert_eq!(t["verification_tokens"], 40);
    assert_eq!(
        (
            t["verification_packs"].as_i64(),
            t["verification_packs_unmeasured"].as_i64()
        ),
        (Some(2), Some(1))
    );
    assert_eq!(t["attempts_unmeasured"], 1);
    assert_eq!(
        (
            t["ratio"]["numerator"].as_i64(),
            t["ratio"]["denominator"].as_i64()
        ),
        (Some(140), Some(450))
    );
    close(&t["ratio"]["value"], 140.0 / 450.0);
    // 3 packs produced, 2 reviewed, 1 accepted.
    assert_eq!(
        (m["mrp_produced"].as_i64(), m["mrp_unreviewed"].as_i64()),
        (Some(3), Some(1))
    );
    close(&m["mrp_acceptance_rate"]["value"], 0.5);
    // 4 pull requests opened in the window; one is merged and not reverted.
    let o = &m["outcomes"];
    assert_eq!(
        (
            o["opened"].as_i64(),
            o["merged"].as_i64(),
            o["closed"].as_i64(),
            o["reverted"].as_i64(),
            o["merged_not_reverted"].as_i64()
        ),
        (Some(4), Some(1), Some(1), Some(1), Some(1))
    );
    assert!(!m.to_string().contains("pqc"), "{m}");
    // No prill: no quality, and its reason.
    assert!(m.get("quality").is_none(), "{m}");
    assert_eq!(
        m["quality_unavailable"],
        "The verifier is not installed on the board. An administrator sets it up: see the Administrator Guide"
    );
    // Spend is tokens: no dollar figure anywhere.
    assert!(!m.to_string().contains("usd"), "{m}");

    // A project with no rows: every ratio is null with its reason, never 0.
    let uri = format!("/api/projects/{empty_id}/metrics/factory");
    let (status, e) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    for ratio in [
        &e["escalation_rate"],
        &e["decided_alone_rate"],
        &e["verification_tax"]["ratio"],
        &e["mrp_acceptance_rate"],
        &e["lights_off_rate"],
    ] {
        assert!(ratio["value"].is_null(), "{ratio}");
        assert_eq!(ratio["null_reason"], "denominator_zero");
    }
    for minutes in [
        &e["human_minutes_per_decision"],
        &e["human_minutes_per_mrp"],
    ] {
        assert!(minutes["median_minutes"].is_null() && minutes["p90_minutes"].is_null());
        assert_eq!(minutes["null_reason"], "not_measured");
    }
}

// How runs began and how merges came about: a person's request, rework, one started by itself;
// the button and the automatic merge. Only the second task's newest merged run began by itself
// and asked nothing, so lights-off is 1 of the 2 merged; the first task answered a decision.
#[tokio::test]
async fn start_triggers_and_lights_off_are_counted_from_rows() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app = crate::common::with_default_host(build_router(AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    }));
    let (project_id, first) = project_with_item(&repo, workspace_id, "P73 starts").await;
    let second = repo
        .create_item(
            project_id,
            "To Do",
            CreateItem {
                title: "by itself".into(),
                ..CreateItem::default()
            },
        )
        .await
        .unwrap()
        .id
        .to_string();
    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'p73', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;
    // R1 a person's, R2 its rework, R3 started by itself on the second task.
    for (id, item, metadata) in [
        ("R1", &first, "{}"),
        ("R2", &first, r#"{"rework_of":"R1"}"#),
        ("R3", &second, r#"{"started_by":"auto_start"}"#),
    ] {
        seed::statement(
            &repo,
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
               request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
               repository_snapshot, permission_policy, metadata, created_at, updated_at)
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', ?,
               '2026-09-10T08:00:00Z', '2026-09-10T08:00:00Z')",
            &[id, item, id, metadata],
        )
        .await;
    }
    for (attempt, request) in [("R1a", "R1"), ("R2a", "R2"), ("R3a", "R3")] {
        seed::statement(
            &repo,
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
               lease_issued_at, lease_expires_at, created_at, updated_at)
             VALUES (?, ?, 1, 'run', 1, '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z',
               '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z')",
            &[attempt, request],
        )
        .await;
    }
    seed::statement(
        &repo,
        "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, created_at,
           updated_at, resolved_at)
         VALUES ('D1', 'R1a', 'D1', 'choice', 'p', '2026-09-10T09:10:00Z', '2026-09-10T09:10:00Z',
           '2026-09-10T09:20:00Z')",
        &[],
    )
    .await;
    let project = project_id.to_string();
    seed::statement(
        &repo,
        "INSERT INTO tack_merges (attempt_id, project_id, trigger, checked_at, merged_at) VALUES
           ('R2a', ?, 'button', '2026-09-10T10:00:00Z', '2026-09-10T10:00:00Z'),
           ('R3a', ?, 'auto', '2026-09-10T11:00:00Z', '2026-09-10T11:00:00Z')",
        &[&project, &project],
    )
    .await;

    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory");
    let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    assert_eq!(
        m["runs_by_start"],
        json!({"person": 1, "auto": 1, "rework": 1})
    );
    assert_eq!(
        m["merges_by_trigger"],
        json!({"button": 1, "done": 0, "auto": 1})
    );
    assert_eq!(m["decisions_answered"], 1);
    let lights = &m["lights_off_rate"];
    assert_eq!(
        (lights["numerator"].as_i64(), lights["denominator"].as_i64()),
        (Some(1), Some(2))
    );
    close(&lights["value"], 0.5);
}

// A question the agent's own tool took the answer to is closed and answered: it is counted as a
// decision a person answered (and so the task was not run lights-off), never as one still open,
// and the question still pending beside it is not counted as answered.
#[tokio::test]
async fn a_decision_answered_elsewhere_counts_as_answered() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app = crate::common::with_default_host(build_router(AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    }));
    let (project_id, item) = project_with_item(&repo, workspace_id, "Answered elsewhere").await;
    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'ae', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
           request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
           repository_snapshot, permission_policy, metadata, created_at, updated_at)
         VALUES ('R1', ?, 's', 'R1', 'f', 'exact_runner', 'run', '{}', '{}', '{}',
           '{\"started_by\":\"auto_start\"}', '2026-09-10T08:00:00Z', '2026-09-10T08:00:00Z')",
        &[&item],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
           lease_issued_at, lease_expires_at, created_at, updated_at)
         VALUES ('R1a', 'R1', 1, 'run', 1, '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z',
           '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z')",
        &[],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, state, created_at,
           updated_at, resolved_at)
         VALUES ('DA', 'R1a', 'DA', 'choice', 'p', 'answered_elsewhere', '2026-09-10T09:10:00Z',
           '2026-09-10T09:30:00Z', '2026-09-10T09:30:00Z')",
        &[],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, state, created_at,
           updated_at)
         VALUES ('DP', 'R1a', 'DP', 'choice', 'p', 'pending', '2026-09-10T09:40:00Z',
           '2026-09-10T09:40:00Z')",
        &[],
    )
    .await;
    let project = project_id.to_string();
    seed::statement(
        &repo,
        "INSERT INTO tack_merges (attempt_id, project_id, trigger, checked_at, merged_at)
         VALUES ('R1a', ?, 'auto', '2026-09-10T11:00:00Z', '2026-09-10T11:00:00Z')",
        &[&project],
    )
    .await;

    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory");
    let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    assert_eq!(m["decisions"], 2, "both questions were asked");
    assert_eq!(
        m["decisions_answered"], 1,
        "only the closed one is answered"
    );
    assert_eq!(m["human_minutes_per_decision"]["measured"], 1, "{m}");
    close(&m["human_minutes_per_decision"]["median_minutes"], 20.0);
    assert_eq!(
        m["decided_alone"], 0,
        "a person's tool answered, not the agent's policy"
    );
    let lights = &m["lights_off_rate"];
    assert_eq!(
        (lights["numerator"].as_i64(), lights["denominator"].as_i64()),
        (Some(0), Some(1)),
        "a task that had a question answered is not lights-off"
    );
}

/// A prill that answers `--version` and `metrics` from the captured output,
/// with `qualified` raised to 2.
fn fake_prill(dir: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prill");
    let metrics = std::fs::read_to_string(fixtures.join("metrics-merged.stdout"))
        .unwrap()
        .replace("\"qualified\": 0,", "\"qualified\": 2,");
    std::fs::write(dir.join("metrics.json"), metrics).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let script = dir.join("bin/prill");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n[ \"$1\" = \"--version\" ] && {{ echo 'prill 0.1.0'; exit 0; }}\n\
             [ \"$1\" = \"metrics\" ] && {{ cat '{}/metrics.json'; exit 0; }}\nexit 2\n",
            dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    script.display().to_string()
}

#[tokio::test]
async fn quality_is_prills_and_spend_is_tokens() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = |program: String| AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            prill_program: program,
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx.clone(),
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    };
    let (project_id, item) = project_with_item(&repo, workspace_id, "P").await;
    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'p', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
           request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
           repository_snapshot, permission_policy, metadata, requested_model, created_at, updated_at)
         VALUES ('R1', ?, 's', 'R1', 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', 'opaque/model',
           '2026-09-10T08:00:00Z', '2026-09-10T08:00:00Z')",
        &[&item],
    )
    .await;
    let used = usage(1_000_000, 100_000);
    seed::statement(
        &repo,
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
           lease_issued_at, lease_expires_at, created_at, updated_at, usage)
         VALUES ('A1', 'R1', 1, 'run', 1, '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z',
           '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z', ?)",
        &[&used],
    )
    .await;
    seed::statement(
        &repo,
        "INSERT INTO pull_requests (attempt_id, repo, number, url, state, opened_at)
         VALUES ('A1', 'o/r', 1, 'u', 'merged', '2026-09-10T13:00:00Z')",
        &[],
    )
    .await;
    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory");
    let get = |program: String| {
        let app = crate::common::with_default_host(build_router(state(program)));
        let (uri, headers) = (uri.clone(), headers);
        async move {
            let (status, body) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            (app, body)
        }
    };

    // prill installed, but no repository has a store yet.
    let program = fake_prill(root.path());
    let (_, m) = get(program.clone()).await;
    assert!(m.get("quality").is_none(), "{m}");
    assert_eq!(
        m["quality_unavailable"],
        "no repository has a prill store yet"
    );

    // With a store: prill's figures, named as prill's, with its version.
    std::fs::create_dir_all(root.path().join("prill")).unwrap();
    std::fs::write(root.path().join("prill/o__r.db"), "").unwrap();
    let (_, m) = get(program.clone()).await;
    assert!(m.get("quality_unavailable").is_none(), "{m}");
    assert_eq!(m["quality"]["source"], "prill");
    assert_eq!(m["quality"]["prill_version"], "0.1.0");
    let repos = m["quality"]["repositories"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "{m}");
    assert_eq!(repos[0]["repository"], "o/r");
    assert_eq!(repos[0]["metrics"]["qualified"], 2);
    // The board's own setting is the default and prill is not on its PATH: the program the
    // embedded runner verifies with is the one the board uses, and `TACK_PRILL_PROGRAM` still wins.
    let by_runner = AppState {
        local_runner: Some(Arc::new(RunnerVerifyingWith(program.clone()))),
        tasks: Default::default(),
        ..state(AppConfig::default().prill_program)
    };
    let (status, m) = common::send(
        &crate::common::with_default_host(build_router(by_runner.clone())),
        "GET",
        &uri,
        Value::Null,
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{m}");
    assert_eq!(m["quality"]["prill_version"], "0.1.0", "{m}");
    let own = AppState {
        config: AppConfig {
            prill_program: "tack-no-such-prill".into(),
            ..by_runner.config.clone()
        },
        ..by_runner
    };
    let (_, m) = common::send(
        &crate::common::with_default_host(build_router(own)),
        "GET",
        &uri,
        Value::Null,
        &headers,
    )
    .await;
    assert_eq!(
        m["quality_unavailable"],
        "The verifier is not installed on the board. An administrator sets it up: see the Administrator Guide",
        "{m}"
    );
    // Spend is the attempts' tokens by model and team, and nothing prices it.
    assert_eq!(
        m["cost_by_model"],
        json!([{
            "model": "opaque/model", "tokens_in": 1_000_000, "cache_read": 0,
            "tokens_out": 100_000, "attempts": 1
        }]),
        "{m}"
    );
    assert_eq!(
        m["cost_by_team"],
        json!([{
            "team": "No team", "tokens_in": 1_000_000, "cache_read": 0,
            "tokens_out": 100_000, "attempts": 1
        }]),
        "{m}"
    );
    assert!(!m.to_string().contains("usd"), "{m}");
}

// An escalation the agent's policy settled itself leaves the person-answered figures where they
// were and raises its own count: attempts with such an event, once each however many it has, over
// all attempts; a project with none says 0.
#[tokio::test]
async fn an_escalation_decided_alone_is_counted_beside_decisions_and_never_inside() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app = crate::common::with_default_host(build_router(AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    }));
    let (project_id, item) = project_with_item(&repo, workspace_id, "decided alone").await;
    let (other_id, other_item) = project_with_item(&repo, workspace_id, "another").await;
    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'da', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;
    for (request, item) in [("Q1", &item), ("Q2", &item), ("Q3", &other_item)] {
        seed::statement(
            &repo,
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
               request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
               repository_snapshot, permission_policy, metadata, created_at, updated_at)
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}',
               '2026-09-10T08:00:00Z', '2026-09-10T08:00:00Z')",
            &[request, item, request],
        )
        .await;
    }
    for (attempt, request) in [("Q1a1", "Q1"), ("Q2a1", "Q2"), ("Q3a1", "Q3")] {
        seed::statement(
            &repo,
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
               lease_issued_at, lease_expires_at, created_at, updated_at)
             VALUES (?, ?, 1, 'run', 1, '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z',
               '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z')",
            &[attempt, request],
        )
        .await;
    }
    // One person-answered decision, 10 minutes from the view.
    seed::statement(
        &repo,
        "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, created_at,
           updated_at, viewed_at, resolved_at)
         VALUES ('D1', 'Q1a1', 'D1', 'choice', 'p', '2026-09-10T09:50:00Z', '2026-09-10T09:50:00Z',
           '2026-09-10T10:00:00Z', '2026-09-10T10:10:00Z')",
        &[],
    )
    .await;
    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let metrics = |id: Uuid| {
        let app = app.clone();
        async move {
            let uri = format!("/api/projects/{id}/metrics/factory");
            let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
            assert_eq!(status, StatusCode::OK, "{m}");
            m
        }
    };

    let before = metrics(project_id).await;
    assert_eq!(before["decided_alone"], 0);
    close(&before["decided_alone_rate"]["value"], 0.0);
    assert_eq!(before["escalation_rate"]["numerator"], 1);

    // Q2a1 decided two alone, Q1a1 one beside its person's answer; another project's one is not ours.
    for (attempt, kind, sequence) in [
        ("Q2a1", "escalation.decided", 1),
        ("Q2a1", "escalation.decided", 2),
        ("Q1a1", "escalation.decided", 1),
        ("Q3a1", "escalation.decided", 1),
        ("Q2a1", "agent.hop_finished", 3),
    ] {
        seed::execution_event_of_kind(&repo, attempt, kind, sequence, "2026-09-10T09:30:00Z").await;
    }
    let after = metrics(project_id).await;
    assert_eq!(after["decided_alone"], 2, "{after}");
    assert_eq!(after["decided_alone_rate"]["numerator"], 2);
    assert_eq!(after["decided_alone_rate"]["denominator"], 2);
    close(&after["decided_alone_rate"]["value"], 1.0);
    // What a person answered is untouched, and no decision row appeared.
    assert_eq!(after["decisions"], before["decisions"]);
    assert_eq!(after["escalation_rate"], before["escalation_rate"]);
    assert_eq!(
        after["human_minutes_per_decision"],
        before["human_minutes_per_decision"]
    );
    close(&after["human_minutes_per_decision"]["median_minutes"], 10.0);
    let other = metrics(other_id).await;
    assert_eq!(other["decided_alone"], 1);
}

/// The router over `repo`, with `storage_dir` holding the stored packs.
fn app_over(repo: &Repository, workspace_id: Uuid, storage_dir: &std::path::Path) -> axum::Router {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    crate::common::with_default_host(build_router(AppState {
        notifications: crate::common::inbox(&repo.clone()),
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: storage_dir.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        event_bus: Default::default(),
        local_runner: None,
        tasks: Default::default(),
    }))
}

/// A new task in the project's first column.
async fn another_item(repo: &Repository, project: Uuid) -> String {
    repo.create_item(
        project,
        "To Do",
        CreateItem {
            recurrence: None,
            recurrence_of: None,
            title: "work".into(),
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
    .unwrap()
    .id
    .to_string()
}

/// One request with one attempt on `item`: `(attempt, number, state, reason, harness, profile, template)`.
#[allow(clippy::too_many_arguments)]
async fn quality_attempt(
    repo: &Repository,
    item: &str,
    attempt: &str,
    number: &str,
    state: &str,
    reason: &str,
    harness: &str,
    profile: &str,
    template: &str,
) {
    let snapshot = json!({"resolved_agent_profile": {"name": profile}}).to_string();
    seed::statement(
        repo,
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key,
           request_fingerprint, selector_kind, selector_id, agent_profile_snapshot,
           repository_snapshot, permission_policy, metadata, request_snapshot,
           requested_harness_kind, prompt_template_sha, created_at, updated_at)
         VALUES (?1, ?2, 's', ?1, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', ?3, ?4, ?5,
           '2026-09-10T08:00:00Z', '2026-09-10T08:00:00Z')",
        &[
            &format!("req-{attempt}"),
            item,
            &snapshot,
            harness,
            template,
        ],
    )
    .await;
    // Attempts are created a minute apart, in the order they are written.
    seed::statement(
        repo,
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token,
           state, terminal_reason, lease_issued_at, lease_expires_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'run', ?3, ?4, NULLIF(?5, ''), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
           strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-' || (1000 - (SELECT COUNT(*) FROM execution_attempts)) || ' minutes'),
           strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
        &[attempt, &format!("req-{attempt}"), number, state, reason],
    )
    .await;
}

/// Known rates over a fixture, each worked out by hand; the verifier's agreement is the four-case table.
#[tokio::test]
async fn the_quality_figures_give_the_rates_the_rows_make() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let app = app_over(&repo, workspace_id, root.path());
    let (project_id, first_item) = project_with_item(&repo, workspace_id, "Quality").await;
    seed::statement(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at)
         VALUES ('run', 'q', 'h', 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        &[],
    )
    .await;

    // Four tasks the verifier judged; the first also has a second attempt (rework) nobody reviewed.
    let good_merge = first_item;
    let bad_merge = another_item(&repo, project_id).await;
    let bad_reject = another_item(&repo, project_id).await;
    let good_reject = another_item(&repo, project_id).await;
    // Two on another agent: one failed after starting, one turned away.
    let failed = another_item(&repo, project_id).await;
    let refused = another_item(&repo, project_id).await;
    // A recurring task that has run four times: ok, ok, failed, ok.
    let mut weekly = Vec::new();
    for _ in 0..4 {
        weekly.push(another_item(&repo, project_id).await);
    }
    seed::statement(
        &repo,
        "UPDATE items SET recurrence = 'weekly' WHERE id = ?",
        &[&weekly[0]],
    )
    .await;
    for pair in weekly.windows(2) {
        seed::statement(
            &repo,
            "UPDATE items SET recurrence = 'weekly', recurrence_of = ? WHERE id = ?",
            &[&pair[0], &pair[1]],
        )
        .await;
    }

    let claude = "claude-code";
    for (item, attempt, state) in [
        (&good_merge, "v1", "succeeded"),
        (&bad_merge, "v2", "succeeded"),
        (&bad_reject, "v3", "succeeded"),
        (&good_reject, "v4", "succeeded"),
    ] {
        quality_attempt(
            &repo,
            item,
            attempt,
            "1",
            state,
            "",
            claude,
            "Implementer",
            "sha-1",
        )
        .await;
    }
    quality_attempt(
        &repo,
        &good_merge,
        "v1b",
        "2",
        "succeeded",
        "",
        claude,
        "Implementer",
        "sha-1",
    )
    .await;
    quality_attempt(
        &repo,
        &failed,
        "f1",
        "1",
        "failed",
        r#"{"code":"timeout"}"#,
        "codex",
        "Reviewer",
        "sha-2",
    )
    .await;
    quality_attempt(
        &repo,
        &refused,
        "f2",
        "1",
        "failed",
        r#"{"code":"harness_rejected"}"#,
        "codex",
        "Reviewer",
        "sha-2",
    )
    .await;
    for (n, state) in ["succeeded", "succeeded", "failed", "succeeded"]
        .into_iter()
        .enumerate()
    {
        let id = ["w1", "w2", "w3", "w4"][n];
        quality_attempt(
            &repo,
            &weekly[n],
            id,
            "1",
            state,
            "",
            claude,
            "Implementer",
            "sha-1",
        )
        .await;
    }

    // Reviews and pull requests: v1 accepted and merged, v2 accepted and reverted, v3 rejected, v4 accepted and merged.
    seed::statement(
        &repo,
        "INSERT INTO attempt_reviews (attempt_id, verdict, reviewed_at) VALUES
           ('v1', 'accepted', '2026-09-10T12:00:00Z'), ('v2', 'accepted', '2026-09-10T12:00:00Z'),
           ('v3', 'rejected', '2026-09-10T12:00:00Z'), ('v4', 'accepted', '2026-09-10T12:00:00Z')",
        &[],
    )
    .await;
    for (attempt, number, state) in [
        ("v1", "1", "merged"),
        ("v2", "2", "reverted"),
        ("v4", "3", "merged"),
    ] {
        seed::statement(
            &repo,
            "INSERT INTO pull_requests (attempt_id, repo, number, url, state, opened_at)
             VALUES (?, 'o/r', ?, 'u', ?, '2026-09-10T13:00:00Z')",
            &[attempt, number, state],
        )
        .await;
    }
    // The verifier's packs: merge on v1 (good) and v2 (bad); reject on v3 (bad) and v4 (good).
    let dir = root.path().join("execution-artifacts");
    std::fs::create_dir_all(&dir).unwrap();
    for (attempt, word) in [
        ("v1", "merge"),
        ("v2", "merge"),
        ("v3", "reject"),
        ("v4", "reject"),
    ] {
        let reference = format!("{attempt}.json");
        std::fs::write(
            dir.join(&reference),
            json!({"recommendation": {"decision": word}}).to_string(),
        )
        .unwrap();
        seed::statement(
            &repo,
            "INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type,
               size_bytes, sha256, content_reference, created_at)
             VALUES ('x-' || ?1, ?1, 'mrp-1', 'report', 'mrp.json', ?2, 1, 'x', ?3, '2026-09-10T11:00:00Z')",
            &[attempt, MRP, &reference],
        )
        .await;
    }
    // Two questions on v1, one allowed and one denied, over four steps.
    for (id, option) in [("q1", "allow_once"), ("q2", "deny")] {
        seed::statement(
            &repo,
            "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, state, prompt, answer,
               created_at, updated_at, resolved_at)
             VALUES (?1, 'v1', ?1, 'approval', 'resolved', 'p', json_object('option_id', ?2),
               '2026-09-10T09:00:00Z', '2026-09-10T09:00:00Z', '2026-09-10T09:05:00Z')",
            &[id, option],
        )
        .await;
    }
    for n in 0..4 {
        seed::execution_event_of_kind(&repo, "v1", "agent.model_call", n, "2026-09-10T09:01:00Z")
            .await;
    }

    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory");
    let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    let q = &m["work_quality"];
    let counts = |r: &Value| (r["numerator"].as_i64(), r["denominator"].as_i64());

    // Succeeded: v1 v2 v3 v4 v1b w1 w2 w4 = 8; failed after starting: f1 w3 = 2; f2 never started.
    assert_eq!(counts(&q["success_rate"]), (Some(8), Some(10)));
    // First attempts a person reviewed: v1 v2 v3 v4; accepted: v1 v2 v4.
    assert_eq!(counts(&q["first_pass_acceptance"]), (Some(3), Some(4)));
    // One attempt after a task's first (v1b) over the 10 tasks with an attempt.
    assert_eq!(counts(&q["rework_per_item"]), (Some(1), Some(10)));
    // Merged v1 and v4, reverted v2.
    assert_eq!(counts(&q["revert_after_merge"]), (Some(1), Some(3)));

    // The hand table: v1 merge/good agrees, v2 merge/bad is a false approval,
    // v3 reject/bad agrees, v4 reject/good is a false rejection.
    let a = &q["verifier_agreement"];
    assert_eq!(a["verifier"], "prill");
    assert_eq!(
        (
            a["agreed"].as_i64(),
            a["false_approvals"].as_i64(),
            a["false_rejections"].as_i64(),
            a["handed_to_a_person"].as_i64(),
            a["without_a_pack"].as_i64()
        ),
        (Some(2), Some(1), Some(1), Some(0), Some(0))
    );
    assert_eq!(counts(&a["false_approval_rate"]), (Some(1), Some(2)));
    assert_eq!(counts(&a["false_rejection_rate"]), (Some(1), Some(2)));

    // The recurring task ran 4 times and 3 succeeded: C(3,3) / C(4,3) = 1/4.
    let k = &q["pass_pow_k"];
    assert_eq!(
        (
            k["k"].as_i64(),
            k["tasks_measured"].as_i64(),
            k["tasks_too_short"].as_i64()
        ),
        (Some(3), Some(1), Some(0))
    );
    close(&k["value"], 0.25);

    // 2 questions over 4 steps of the one agent that asked: 50 per hundred; 1 of 2 allowed.
    let n = &q["questions"][0];
    assert_eq!(
        (n["harness"].as_str(), n["class"].as_str()),
        (Some("claude-code"), Some("approval"))
    );
    assert_eq!(
        (n["asked"].as_i64(), n["actions"].as_i64()),
        (Some(2), Some(4))
    );
    close(&n["per_100_actions"], 50.0);
    assert_eq!(counts(&n["granted"]), (Some(1), Some(2)));

    // Stops: a timeout, a team that turned the run away, and a failure with no code.
    let stops: Vec<(String, i64, bool)> = q["stops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["reason"].as_str().unwrap().to_owned(),
                s["count"].as_i64().unwrap(),
                s["not_started"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(stops.len(), 3, "{stops:?}");
    for expected in [
        ("harness_rejected", 1, true),
        ("timeout", 1, false),
        ("unspecified", 1, false),
    ] {
        assert!(
            stops.contains(&(expected.0.to_owned(), expected.1, expected.2)),
            "{stops:?}"
        );
    }

    // Cuts: claude-code succeeded 8 of 9 that started; codex 0 of 1 (the turned-away one is out).
    let cut = |by: &str, name: &str| -> Value {
        q["cuts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["by"] == by)
            .unwrap_or_else(|| panic!("no cut by {by}"))["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap_or_else(|| panic!("no {name} in {by}"))
            .clone()
    };
    assert_eq!(
        counts(&cut("harness", "claude-code")["success_rate"]),
        (Some(8), Some(9))
    );
    assert_eq!(
        counts(&cut("harness", "codex")["success_rate"]),
        (Some(0), Some(1))
    );
    assert_eq!(
        counts(&cut("template", "sha-1")["first_pass_acceptance"]),
        (Some(3), Some(4))
    );
    assert_eq!(cut("template", "sha-2")["attempts"], 2);
    assert_eq!(
        counts(&cut("profile", "Reviewer")["success_rate"]),
        (Some(0), Some(1))
    );
    assert_eq!(cut("team", "No team")["attempts"], 11);
    assert_eq!(
        counts(&cut("model", "Not recorded")["revert_after_merge"]),
        (Some(1), Some(3))
    );

    assert!(!q.to_string().contains("usd"), "{q}");
}

/// A project with no attempts: every rate is null with its reason, never 0.
#[tokio::test]
async fn quality_figures_with_nothing_to_measure_are_null() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let app = app_over(&repo, workspace_id, root.path());
    let (project_id, _) = project_with_item(&repo, workspace_id, "Empty quality").await;
    let headers = [("authorization", "Bearer g1a-metrics-token")];
    let uri = format!("/api/projects/{project_id}/metrics/factory");
    let (status, m) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    let q = &m["work_quality"];
    for rate in [
        "success_rate",
        "first_pass_acceptance",
        "rework_per_item",
        "revert_after_merge",
    ] {
        assert!(q[rate]["value"].is_null(), "{rate}");
        assert_eq!(q[rate]["null_reason"], "denominator_zero", "{rate}");
    }
    assert!(q["pass_pow_k"]["value"].is_null());
    assert_eq!(q["questions"], json!([]));
    assert_eq!(q["stops"], json!([]));
}
