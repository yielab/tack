//! Tests for the item/project agent-activity endpoints:
//! `GET /api/items/{id}/agent-activity` and
//! `GET /api/projects/{id}/agent-activity`.
//!
//! Covers: the off/unknown-item guards; the bulk endpoint's inner join and
//! "latest attempt" tie-break; attempts newest-first; `remote_run_id`
//! correlating the right `orch_runs`/`orch_events` rows; honesty fields
//! (`run.error` is `""` not `null`, `pricing_snapshot_at` is always `null`);
//! approvals (pending and decided, newest-first); and `events_truncated`
//! reflecting the retention cutoff.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::{NewOrchApproval, NewOrchEvent, NewOrchRun, NewOrchTask};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers (mirrors orchestration/control_plane/resource.rs's app_with_state) ─

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    }
}

async fn app_with_state(config: AppConfig) -> (Router, AppState) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..config
    };
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };

    (build_router(state.clone()), state)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn create_item(app: &Router, project_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

fn new_task(
    item_id: Uuid,
    remote_task_id: &str,
    attempt: i64,
    dispatched_at: chrono::DateTime<Utc>,
) -> NewOrchTask {
    NewOrchTask {
        item_id,
        remote_task_id: remote_task_id.to_string(),
        remote_run_id: None,
        remote_status: "running".to_string(),
        attempt,
        tokens_in: 100,
        tokens_out: 200,
        cost_usd_estimated: Some(0.01),
        dispatched_at,
        trusted: true,
    }
}

fn new_run(item_id: Uuid, run_id: &str, source: &str) -> NewOrchRun {
    NewOrchRun {
        run_id: run_id.to_string(),
        item_id: Some(item_id),
        remote_project: "my-remote-project".to_string(),
        source: source.to_string(),
        state: "running".to_string(),
        started_at: Some(Utc::now()),
        ended_at: None,
        error: None,
    }
}

fn new_approval(
    item_id: Uuid,
    token: &str,
    action: &str,
    requested_at: chrono::DateTime<Utc>,
    decided_at: Option<chrono::DateTime<Utc>>,
) -> NewOrchApproval {
    NewOrchApproval {
        token: token.to_string(),
        item_id: Some(item_id),
        remote_task_id: None,
        agent: Some("builder".to_string()),
        action: Some(action.to_string()),
        state: if decided_at.is_some() {
            "granted"
        } else {
            "pending"
        }
        .to_string(),
        requested_at,
        decided_at,
    }
}

fn new_tool_call_event(item_id: Uuid, run_id: &str) -> NewOrchEvent {
    NewOrchEvent {
        id: Uuid::new_v4(),
        item_id: Some(item_id),
        run_id: Some(run_id.to_string()),
        event_type: "tool_call".to_string(),
        payload: json!({"tool": "git"}),
        occurred_at: Utc::now(),
    }
}

/// A fresh app plus one item in a fresh project — the setup nearly every
/// test in this file starts from.
async fn setup_item(config: AppConfig, title: &str) -> (Router, AppState, Uuid, Uuid) {
    let (app, state) = app_with_state(config).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, title).await;
    (app, state, project_id, item_id)
}

async fn create_control_plane(state: &AppState) -> Uuid {
    state
        .repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://docket.local".to_string(),
            token: None,
        })
        .await
        .expect("create plane")
        .id
}

// ─── 404s ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn item_agent_activity_404s_for_unknown_item() {
    let (app, _) = app_with_state(orch_config()).await;
    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{}/agent-activity", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn item_agent_activity_empty_for_item_with_no_dispatches() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Untouched item").await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["attempts"], json!([]));
    assert_eq!(v["approvals"], json!([]));
    assert_eq!(v["events_truncated"], false);
}

// ─── Bulk badge endpoint: inner join + latest-attempt tie-break ───────────

/// The bulk endpoint is an inner join on `orch_tasks`: with nothing
/// dispatched anywhere, `rows` is empty; with one of two items dispatched,
/// `rows` has exactly that one item — the untouched item never appears as
/// a null-status row.
#[tokio::test]
async fn project_agent_activity_is_inner_join_on_orch_tasks() {
    for seed_dispatched_task in [false, true] {
        let (app, state) = app_with_state(orch_config()).await;
        let project_id =
            common::create_project(&app, "Agent Activity Test Project", "software").await;
        let dispatched_item = create_item(&app, project_id, "Dispatched").await;
        let _untouched_item = create_item(&app, project_id, "Never dispatched").await;
        if seed_dispatched_task {
            state
                .repo
                .upsert_orch_tasks(&[new_task(dispatched_item, "task-1", 1, Utc::now())])
                .await
                .expect("seed task");
        }

        let uri = format!("/api/projects/{project_id}/agent-activity");
        let res = req(&app, Method::GET, &uri, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        let rows = v["rows"].as_array().unwrap();
        if seed_dispatched_task {
            assert_eq!(
                rows.len(),
                1,
                "an item with zero orch_tasks rows must not appear at all (inner join, not a null-status row): {rows:?}"
            );
            assert_eq!(rows[0]["item_id"], dispatched_item.to_string());
        } else {
            assert_eq!(
                rows,
                &Vec::<Value>::new(),
                "no activity anywhere: rows must be empty"
            );
        }
    }
}

/// The "latest attempt" tie-break: the highest `attempt` number wins
/// regardless of `dispatched_at` ordering; on an attempt-number tie, the
/// row with the later `dispatched_at` wins.
#[tokio::test]
async fn project_agent_activity_latest_attempt_wins_tie_break() {
    // (task-a, task-b, expected winning attempt, expected winning status).
    type Task = (i64, Duration, &'static str);
    let cases: [(Task, Task, i64, &str); 2] = [
        (
            (1, Duration::hours(2), "failed"),
            (2, Duration::hours(1), "running"),
            2,
            "running",
        ),
        (
            (1, Duration::hours(2), "failed"),
            (1, Duration::minutes(1), "done"),
            1,
            "done",
        ),
    ];

    for (a, b, expect_attempt, expect_status) in cases {
        let (app, state, project_id, item_id) =
            setup_item(orch_config(), "Redispatched item").await;
        let now = Utc::now();
        let task_for = |task_id: &str, (attempt, age, status): Task| {
            let mut t = new_task(item_id, task_id, attempt, now - age);
            t.remote_status = status.to_string();
            t
        };
        state
            .repo
            .upsert_orch_tasks(&[task_for("task-a", a), task_for("task-b", b)])
            .await
            .expect("seed tasks");
        let uri = format!("/api/projects/{project_id}/agent-activity");
        let v = body_json(req(&app, Method::GET, &uri, None).await).await;
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "one row per item, not one per attempt");
        assert_eq!(rows[0]["attempt"], expect_attempt);
        assert_eq!(rows[0]["remote_status"], expect_status);
    }
}

// ─── Item detail endpoint: ordering, run/event correlation, honesty fields ─

#[tokio::test]
async fn item_agent_activity_attempts_are_newest_first() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Multi-attempt item").await;

    let now = Utc::now();
    state
        .repo
        .upsert_orch_tasks(&[
            new_task(item_id, "task-1", 1, now - Duration::hours(2)),
            new_task(item_id, "task-2", 2, now - Duration::hours(1)),
            new_task(item_id, "task-3", 3, now),
        ])
        .await
        .expect("seed tasks");

    let uri = format!("/api/items/{item_id}/agent-activity");
    let v = body_json(req(&app, Method::GET, &uri, None).await).await;
    let attempts = v["attempts"].as_array().unwrap();
    let attempt_numbers: Vec<i64> = attempts
        .iter()
        .map(|a| a["attempt"].as_i64().unwrap())
        .collect();
    assert_eq!(
        attempt_numbers,
        vec![3, 2, 1],
        "must be newest attempt first"
    );
    for a in attempts {
        assert!(
            a["pricing_snapshot_at"].is_null(),
            "no pricing-snapshot mechanism exists yet — must always be null"
        );
    }
}

#[tokio::test]
async fn item_agent_activity_correlates_run_and_events_by_run_id() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Correlated item").await;
    let plane_id = create_control_plane(&state).await;

    let mut task = new_task(item_id, "task-with-run", 1, Utc::now());
    task.remote_run_id = Some("run-abc".to_string());
    state
        .repo
        .upsert_orch_tasks(&[task])
        .await
        .expect("seed task");

    state
        .repo
        .upsert_orch_runs(plane_id, &[new_run(item_id, "run-abc", "webhook")])
        .await
        .expect("seed run");
    state
        .repo
        .upsert_orch_events(plane_id, &[new_tool_call_event(item_id, "run-abc")])
        .await
        .expect("seed event");

    let uri = format!("/api/items/{item_id}/agent-activity");
    let v = body_json(req(&app, Method::GET, &uri, None).await).await;
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    let run = &attempts[0]["run"];
    assert_eq!(run["run_id"], "run-abc");
    assert_eq!(run["source"], "webhook");
    assert_eq!(
        run["error"], "",
        "error must be an empty string, never null, when the run has no error"
    );
    let events = attempts[0]["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "tool_call");
}

#[tokio::test]
async fn item_agent_activity_run_is_null_when_run_id_unresolved() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Queued item").await;

    let mut task = new_task(item_id, "task-no-run-yet", 1, Utc::now());
    task.remote_run_id = None;
    state
        .repo
        .upsert_orch_tasks(&[task])
        .await
        .expect("seed task");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let attempts = v["attempts"].as_array().unwrap();
    assert!(attempts[0]["run"].is_null());
    assert_eq!(attempts[0]["events"], json!([]));
}

#[tokio::test]
async fn item_agent_activity_includes_pending_and_decided_approvals() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Approvals item").await;
    let plane_id = create_control_plane(&state).await;

    let now = Utc::now();
    let older = new_approval(
        item_id,
        "tok-older",
        "git push",
        now - Duration::hours(2),
        Some(now - Duration::hours(1)),
    );
    let newer = new_approval(item_id, "tok-newer", "rm -rf", now, None);
    state
        .repo
        .upsert_orch_approvals(plane_id, &[older, newer])
        .await
        .expect("seed approvals");

    let uri = format!("/api/items/{item_id}/agent-activity");
    let v = body_json(req(&app, Method::GET, &uri, None).await).await;
    let approvals = v["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 2, "both pending and decided must appear");
    assert_eq!(approvals[0]["token"], "tok-newer", "newest-requested first");
    assert_eq!(approvals[1]["token"], "tok-older");
    assert!(approvals[1]["decided_at"].is_string());
}

// ─── events_truncated honesty signal ───────────────────────────────────────

/// `events_truncated` flags whether any attempt predates the retention
/// cutoff: false for a fresh dispatch under the default window, true once
/// an attempt is older than a (here, shortened) retention window.
#[tokio::test]
async fn events_truncated_reflects_retention_cutoff() {
    // (retention_days override, task age, expect events_truncated).
    let cases = [
        (None, Duration::zero(), false),
        (Some(1), Duration::days(30), true),
    ];

    for (retention_days, task_age, expect_truncated) in cases {
        let config = match retention_days {
            Some(orch_event_retention_days) => AppConfig {
                orch_event_retention_days,
                ..orch_config()
            },
            None => orch_config(),
        };
        let (app, state, _, item_id) = setup_item(config.clone(), "Retention item").await;

        state
            .repo
            .upsert_orch_tasks(&[new_task(item_id, "task-1", 1, Utc::now() - task_age)])
            .await
            .expect("seed task");

        let uri = format!("/api/items/{item_id}/agent-activity");
        let v = body_json(req(&app, Method::GET, &uri, None).await).await;
        assert_eq!(
            v["events_truncated"], expect_truncated,
            "{retention_days:?}"
        );
        assert_eq!(
            v["events_retention_days"],
            json!(config.orch_event_retention_days)
        );
    }
}
