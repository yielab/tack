//! HTTP test for `GET /api/projects/{id}/metrics/factory`: rows are seeded
//! directly, and every number the endpoint returns is worked out by hand here.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

const TOKEN: &str = "g1a-metrics-token";
const MRP: &str = "application/vnd.tack.mrp+json";

fn close(value: &Value, expected: f64) {
    let got = value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"));
    assert!((got - expected).abs() < 1e-9, "{got} != {expected}");
}

async fn exec(pool: &sqlx::SqlitePool, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
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
            },
        )
        .await
        .unwrap();
    let item = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
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
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_all(&pool).await.unwrap();
    let workspace_id = Uuid::new_v4();
    exec(
        &pool,
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'G1a', '{}')",
        &[&workspace_id.to_string()],
    )
    .await;
    let repo = Repository::new(pool.clone());
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let app = build_router(AppState {
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
    });
    let (project_id, item) = project_with_item(&repo, workspace_id, "G1a seeded").await;
    let (empty_id, _) = project_with_item(&repo, workspace_id, "G1a empty").await;

    exec(
        &pool,
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
        exec(
            &pool,
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
        exec(&pool, sql, &binds).await;
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
        exec(
            &pool,
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
    exec(
        &pool,
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
        exec(
            &pool,
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
    exec(
        &pool,
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
        exec(
            &pool,
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
    // 4 pull requests opened in the window; PQC is the one merged and not reverted.
    let o = &m["outcomes"];
    assert_eq!(
        (
            o["opened"].as_i64(),
            o["merged"].as_i64(),
            o["closed"].as_i64(),
            o["reverted"].as_i64(),
            o["pqc"].as_i64()
        ),
        (Some(4), Some(1), Some(1), Some(1), Some(1))
    );
    close(&o["pqc_rate"]["value"], 0.25);
    // No dollar figure anywhere, though the seeded usage carries one.
    assert!(!m.to_string().contains("cost"), "{m}");

    // A project with no rows: every ratio is null with its reason, never 0.
    let uri = format!("/api/projects/{empty_id}/metrics/factory");
    let (status, e) = common::send(&app, "GET", &uri, Value::Null, &headers).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    for ratio in [
        &e["escalation_rate"],
        &e["verification_tax"]["ratio"],
        &e["mrp_acceptance_rate"],
        &e["outcomes"]["pqc_rate"],
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
