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
        "cost_usd": {"value": 9.99, "source": "measured"},
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
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
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
        "INSERT INTO mrp_reviews (attempt_id, artifact_id, verdict, viewed_at, reviewed_at) VALUES
           ('R1a1', 'mrp-1', 'accept', '2026-09-10T12:00:00Z', '2026-09-10T12:05:00Z'),
           ('R3a1', 'mrp-1', 'reject', NULL, '2026-09-11T13:30:00Z')",
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
    // No prill and no rate card: no quality, its reason, and no dollar.
    assert!(m.get("quality").is_none(), "{m}");
    assert_eq!(
        m["quality_unavailable"],
        "prill is not installed on the board (give the runner a [verify] table naming prill in its settings file, or set TACK_PRILL_PROGRAM to its path)"
    );
    assert!(m.get("cost_usd").is_none(), "{m}");
    // No dollar figure anywhere, though the seeded usage carries one.
    assert!(!m.to_string().contains("usd"), "{m}");

    // A project with no rows: every ratio is null with its reason, never 0.
    let uri = format!("/api/projects/{empty_id}/metrics/factory");
    let (status, e) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    for ratio in [
        &e["escalation_rate"],
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
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: root.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
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

/// A prill that answers `--version` and `metrics` from the captured output,
/// with `qualified` raised to 2 so the per-dollar figure is not zero.
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
async fn quality_is_prills_and_cost_is_the_rate_cards() {
    let root = tempfile::tempdir().unwrap();
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = |program: String| AppState {
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
        webhook: None,
        local_runner: None,
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
    let (app, m) = get(program.clone()).await;
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
        "prill is not installed on the board (give the runner a [verify] table naming prill in its settings file, or set TACK_PRILL_PROGRAM to its path)",
        "{m}"
    );
    // No rate card: no dollar anywhere, whatever the usage carried.
    assert!(!m.to_string().contains("cost_usd"), "{m}");
    assert!(repos[0].get("qualified_per_usd").is_none(), "{m}");
    // The model is named, and nothing prices it: 0 of 1.
    assert_eq!(
        (m["priced_models"].as_i64(), m["models"].as_i64()),
        (Some(0), Some(1)),
        "{m}"
    );

    // The rate card: $3 in and $15 out per million tokens, 1M in and 100k out = $4.50.
    let card = json!({
        "as_of": "2026-10-01",
        "models": {"opaque/model": {"input_per_mtok": 3.0, "output_per_mtok": 15.0}},
        "ci_minute": 0.008,
        "human_hour": 60.0,
    });
    let (status, saved) = common::send(
        &app,
        "PUT",
        "/api/settings/rate-card",
        card.clone(),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (status, read) = common::send(
        &app,
        "GET",
        "/api/settings/rate-card",
        Value::Null,
        &headers,
    )
    .await;
    assert_eq!((status, &read), (StatusCode::OK, &card));
    let (_, m) = get(program).await;
    close(&m["cost_usd"], 4.5);
    assert_eq!(m["rate_card_as_of"], "2026-10-01");
    assert_eq!(
        (m["priced_models"].as_i64(), m["models"].as_i64()),
        (Some(1), Some(1)),
        "{m}"
    );
    close(&m["quality"]["repositories"][0]["cost_usd"], 4.5);
    close(
        &m["quality"]["repositories"][0]["qualified_per_usd"],
        2.0 / 4.5,
    );
    // The harness's own figure is never summed.
    assert!(!m.to_string().contains("9.99"), "{m}");
}
