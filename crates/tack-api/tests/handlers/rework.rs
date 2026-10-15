//! A rejected change goes back to the agent first. One table: the loop row
//! enqueues a follow-up on the same item, harness, profile and model carrying
//! the verifier's next steps in a fenced block; each stop condition enqueues
//! nothing. Every pack is the signed fixture with next steps, edited per row;
//! the runner and the harness are never run.

use crate::common;
use crate::mrp::{
    BASE_REVISION, ClaimedAttempt, MRP_MEDIA_TYPE, accept_and_start, completion_with_pushed_branch,
    headers_ref, operator_headers, request_and_claim, sha256_hex,
};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "e2-mrp-operator-token";
const WITH_STEPS: &str =
    include_str!("../../../../docs/contracts/mrp-v1/fixtures/signed-with-next-steps.json");

fn pack(edit: impl Fn(&mut Value)) -> Vec<u8> {
    let mut pack: Value = serde_json::from_str(WITH_STEPS).unwrap();
    edit(&mut pack);
    serde_json::to_vec(&pack).unwrap()
}

fn other_code(pack: &mut Value) {
    pack["next_steps"] =
        json!([{"code": "lint_failed", "severity": "error", "message": "fix lint"}]);
}

struct Board {
    app: axum::Router,
    repo: Repository,
    item_id: String,
}

async fn board(storage: &std::path::Path, loop_cap: i64) -> Board {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_all(&pool).await.unwrap();
    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'L', '{}')")
        .bind(workspace_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let repo = Repository::new(pool);
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            storage_dir: storage.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
    };
    let app = common::with_default_host(build_router(state));
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "L rework".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
                code_origin: Some(tack_core::models::CodeOrigin::Url),
                repository: Some("https://example.test/e2.git".into()),
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
            tack_core::models::CreateItem {
                recurrence: None,
                recurrence_of: None,
                title: "Add sub".into(),
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
    sqlx::query("UPDATE projects SET verify_loop_max_attempts = ? WHERE id = ?")
        .bind(loop_cap)
        .bind(project.id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();
    Board {
        app,
        repo,
        item_id: item.id.to_string(),
    }
}

/// Uploads `bytes` as the attempt's pack, then completes it as succeeded.
async fn finish(b: &Board, a: &ClaimedAttempt, bytes: &[u8]) {
    accept_and_start(&b.app, a).await;
    let auth = headers_ref(&a.auth);
    let (status, manifest, _) = common::send_with_raw(
        &b.app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", a.attempt_id),
        json!({
            "protocol_version": 1, "runner_id": a.runner_id, "attempt_id": a.attempt_id,
            "fencing_token": a.fencing_token,
            "artifacts": [{"artifact_id": "mrp-1", "kind": "report", "name": "mrp.json",
                "media_type": MRP_MEDIA_TYPE, "size_bytes": bytes.len(),
                "sha256": sha256_hex(bytes), "content_disposition": "inline_upload", "metadata": {}}],
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{manifest}");
    let response = b
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/runner/v1/attempts/{}/artifacts/mrp-1/content",
                    a.attempt_id
                ))
                .header("authorization", a.auth[0].1.clone())
                .header("x-tack-fencing-token", a.fencing_token.to_string())
                .header("content-type", MRP_MEDIA_TYPE)
                .body(Body::from(bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let (status, body, _) = common::send_with_raw(
        &b.app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", a.attempt_id),
        completion_with_pushed_branch(a),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// The runner that ran `first` claims the next queued request.
async fn claim_next(b: &Board, first: &ClaimedAttempt) -> ClaimedAttempt {
    let (status, claimed, _) = common::send_with_raw(
        &b.app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": first.runner_id, "claim_request_id": "l-claim-2", "available_capacity": 1, "wait_ms": 0}),
        &headers_ref(&first.auth),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    ClaimedAttempt {
        request_id: claimed["request"]["request_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{claimed}"))
            .to_owned(),
        attempt_id: claimed["lease"]["attempt_id"].as_str().unwrap().to_owned(),
        runner_id: first.runner_id.clone(),
        fencing_token: claimed["lease"]["fencing_token"].as_i64().unwrap(),
        auth: first.auth.clone(),
    }
}

async fn follow_ups(b: &Board, original: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT id, metadata, request_snapshot FROM execution_requests \
         WHERE json_extract(metadata, '$.rework_of') = ? ORDER BY created_at, rowid",
    )
    .bind(original)
    .fetch_all(b.repo.pool())
    .await
    .unwrap()
}

/// The row: loop cap, the first pack, and optionally a second run (its pack,
/// and whether the follow-up the first one made is cancelled and replaced by
/// one the operator queued by hand before it runs).
struct Row {
    name: &'static str,
    cap: i64,
    first: Vec<u8>,
    second: Option<(Vec<u8>, bool)>,
    follow_ups: usize,
}

#[tokio::test]
async fn a_rejected_change_goes_back_to_the_agent_until_a_stop_condition() {
    let rows = [
        Row {
            name: "loop",
            cap: 2,
            first: pack(|_| {}),
            second: None,
            follow_ups: 1,
        },
        Row {
            name: "cap is zero",
            cap: 0,
            first: pack(|_| {}),
            second: None,
            follow_ups: 0,
        },
        Row {
            name: "no next steps: the failed criteria go back",
            cap: 2,
            first: pack(|p| {
                p.as_object_mut().unwrap().remove("next_steps");
            }),
            second: None,
            follow_ups: 1,
        },
        Row {
            name: "nothing named to fix",
            cap: 2,
            first: pack(|p| {
                p.as_object_mut().unwrap().remove("next_steps");
                p["criteria"] = json!([]);
            }),
            second: None,
            follow_ups: 0,
        },
        Row {
            name: "review",
            cap: 2,
            first: pack(|p| p["recommendation"]["decision"] = json!("review")),
            second: None,
            follow_ups: 0,
        },
        Row {
            name: "no progress",
            cap: 3,
            first: pack(|_| {}),
            second: Some((pack(|_| {}), false)),
            follow_ups: 1,
        },
        Row {
            name: "previous follow-up cancelled",
            cap: 3,
            first: pack(|_| {}),
            second: Some((pack(other_code), true)),
            follow_ups: 2,
        },
    ];
    for row in rows {
        let root = tempfile::tempdir().unwrap();
        let b = board(root.path(), row.cap).await;
        let a1 = request_and_claim(&b.app, &b.item_id, "l1", None).await;
        finish(&b, &a1, &row.first).await;
        if let Some((second, cancel_first)) = &row.second {
            let next = if *cancel_first {
                let made = follow_ups(&b, &a1.request_id).await;
                assert_eq!(
                    made.len(),
                    1,
                    "{}: the first run made its follow-up",
                    row.name
                );
                sqlx::query("UPDATE execution_requests SET state = 'cancelled' WHERE id = ?")
                    .bind(&made[0].0)
                    .execute(b.repo.pool())
                    .await
                    .unwrap();
                let snapshot: Value = serde_json::from_str(&made[0].2).unwrap();
                let (status, created, _) = common::send_with_raw(
                    &b.app,
                    "POST",
                    "/api/executions",
                    json!({
                        "item_id": b.item_id, "idempotency_key": "l-by-hand",
                        "selector_kind": "exact_runner", "selector_id": a1.runner_id,
                        "agent_profile_id": snapshot["agent_profile_id"],
                        "requested_harness_kind": "codex", "requested_model": "opaque/model-e2",
                        "repository_snapshot": {"kind": "git", "remote": "https://example.test/e2.git", "base_revision": BASE_REVISION, "subdirectory": null},
                        "permission_policy": {"tools": ["shell"], "network": false},
                        "timeout_seconds": 60, "budgets": {}, "environment": {},
                        "metadata": {"rework_of": a1.request_id},
                    }),
                    &operator_headers(),
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{created}");
                claim_next(&b, &a1).await
            } else {
                claim_next(&b, &a1).await
            };
            finish(&b, &next, second).await;
        }
        let made = follow_ups(&b, &a1.request_id).await;
        assert_eq!(made.len(), row.follow_ups, "{}: follow-ups", row.name);
        if row.name == "loop" {
            let snapshot: Value = serde_json::from_str(&made[0].2).unwrap();
            let original: Value = sqlx::query_scalar::<_, String>(
                "SELECT request_snapshot FROM execution_requests WHERE id = ?",
            )
            .bind(&a1.request_id)
            .fetch_one(b.repo.pool())
            .await
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap();
            assert_eq!(snapshot["item_id"], b.item_id);
            for field in [
                "agent_profile_id",
                "requested_harness_kind",
                "requested_model",
            ] {
                assert_eq!(snapshot[field], original[field], "{field}");
            }
            let instructions = snapshot["resolved_agent_profile"]["instructions"]
                .as_str()
                .unwrap();
            assert!(
                instructions.starts_with(
                    original["resolved_agent_profile"]["instructions"]
                        .as_str()
                        .unwrap()
                )
            );
            for line in [
                "Build failed with compilation error",
                "tests/unit.rs:42",
                "cargo build",
            ] {
                assert!(instructions.contains(line), "{line}: {instructions}");
            }
            let fence = instructions.find("<<<card-text id=").unwrap();
            assert!(instructions.find("cargo build").unwrap() > fence);
            assert_eq!(
                serde_json::from_str::<Value>(&made[0].1).unwrap()["rework_of"],
                a1.request_id
            );
            // The branch the rejected attempt pushed is where the follow-up starts.
            assert_eq!(snapshot["metadata"]["rework_base"], "tack/attempt-1");
            assert_eq!(
                serde_json::from_str::<Value>(&made[0].1).unwrap()["rework_base"],
                "tack/attempt-1"
            );
            let queued: String =
                sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
                    .bind(&made[0].0)
                    .fetch_one(b.repo.pool())
                    .await
                    .unwrap();
            assert_eq!(queued, "queued");
        }
    }
}
