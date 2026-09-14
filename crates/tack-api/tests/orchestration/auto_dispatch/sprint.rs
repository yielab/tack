//! Tests for `POST /api/sprints/{id}/dispatch` and
//! `GET /api/sprints/{id}/dispatch/dry-run` (the `sprint_dispatch` module).
//!
//! Covers: the off/unknown/unlinked guards, diamond-graph dependency
//! ordering and readiness gating, cross-sprint dependencies, partial
//! failure (one item's policy block doesn't abort the rest), per-item
//! trust (not one blanket value for the batch), the in-flight cap's
//! clamping and actual concurrency bound, and dry-run/real-run parity.

use crate::common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_core::models::{CreateItem, ItemSource};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/dispatch/item.rs /
// orchestration/auto_dispatch/hook.rs) ──────────────────────────────────────

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

async fn create_sprint(app: &Router, project_id: Uuid) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/sprints"),
        Some(json!({"name": "Sprint 1"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

/// Returns `(item_id, initial_status)`. Assigns the item straight into
/// `sprint_id` at creation time.
async fn create_item(app: &Router, project_id: Uuid, sprint_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title, "sprint_id": sprint_id})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn seed_github_item(
    state: &AppState,
    project_id: Uuid,
    sprint_id: Uuid,
    title: &str,
) -> Uuid {
    let item = state
        .repo
        .create_item_with_source(
            project_id,
            "Backlog",
            CreateItem {
                title: title.to_string(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: Some(sprint_id),
                assignee: None,
            },
            ItemSource::Github,
        )
        .await
        .expect("seed github item");
    item.id
}

async fn create_dependency(app: &Router, source_item_id: Uuid, target_item_id: Uuid) {
    let res = req(
        app,
        Method::POST,
        &format!("/api/items/{source_item_id}/dependencies"),
        Some(json!({"target_item_id": target_item_id, "dependency_type": "blocks"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

async fn patch_status(app: &Router, item_id: Uuid, status: &str) {
    let res = req(
        app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"status": status})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

async fn create_control_plane(app: &Router, base_url: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        Some(json!({"name": "docket-1", "base_url": base_url})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn link_project(app: &Router, project_id: Uuid, control_plane_id: Uuid, status_map: Value) {
    let res = req(
        app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": control_plane_id,
            "remote_project": "demo",
            "status_map": status_map,
        })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

async fn dry_run(app: &Router, sprint_id: Uuid, max_in_flight: Option<u32>) -> Value {
    let uri = match max_in_flight {
        Some(n) => format!("/api/sprints/{sprint_id}/dispatch/dry-run?max_in_flight={n}"),
        None => format!("/api/sprints/{sprint_id}/dispatch/dry-run"),
    };
    let res = req(app, Method::GET, &uri, None).await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    body_json(res).await
}

async fn dispatch_sprint(app: &Router, sprint_id: Uuid, max_in_flight: Option<u32>) -> Value {
    let uri = match max_in_flight {
        Some(n) => format!("/api/sprints/{sprint_id}/dispatch?max_in_flight={n}"),
        None => format!("/api/sprints/{sprint_id}/dispatch"),
    };
    let res = req(app, Method::POST, &uri, None).await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    body_json(res).await
}

/// Mocks `POST /tasks/demo` returning docket's "allow" shape — every
/// enqueue in a test gets the same task id, which is fine: `orch_tasks`'
/// PK is `(item_id, remote_task_id)`, so two different items sharing one
/// remote id never collide.
async fn mock_enqueue_allow(server: &MockServer, task_id: &str) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": task_id, "project": "demo", "status": "pending"
        })))
        .mount(server)
        .await;
}

/// Mocks `POST /tasks/demo` with an artificial delay, to prove out
/// max_in_flight's actual concurrency bound rather than just its clamping.
async fn mock_enqueue_slow(server: &MockServer, task_id: &str, delay_ms: u64) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "ok": true, "task": task_id, "project": "demo", "status": "pending"
                }))
                .set_delay(Duration::from_millis(delay_ms)),
        )
        .mount(server)
        .await;
}

async fn mock_list_tasks(server: &MockServer, task_id: &str) {
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [{
            "id": task_id, "description": "x", "priority": "normal", "status": "pending",
            "created": "2026-08-05T00:00:00Z", "source": "operator",
        }]})))
        .mount(server)
        .await;
}

/// Mocks `POST /tasks/demo`, asserting `description`/`trusted` in the body.
async fn mock_enqueue_trusted(
    server: &MockServer,
    description: &str,
    expect_trusted: bool,
    task_id: &str,
) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(
            json!({"description": description, "trusted": expect_trusted}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": task_id, "project": "demo", "status": "pending"
        })))
        .mount(server)
        .await;
}

/// Mocks the follow-up `GET /tasks/demo` listing several pending tasks.
async fn mock_list_tasks_many(server: &MockServer, task_ids: &[&str]) {
    let tasks: Vec<Value> = task_ids
        .iter()
        .map(|id| {
            json!({
                "id": id, "description": "x", "priority": "normal", "status": "pending",
                "created": "2026-08-05T00:00:00Z", "source": "operator",
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": tasks })))
        .mount(server)
        .await;
}

fn item_by_id(items: &[Value], id: Uuid) -> &Value {
    items
        .iter()
        .find(|i| i["item_id"] == id.to_string())
        .unwrap_or_else(|| panic!("no item {id} in response: {items:?}"))
}

fn order_of(items: &[Value], id: Uuid) -> u64 {
    item_by_id(items, id)["order"].as_u64().unwrap()
}

/// The `blocked_by` array of `item`, as owned strings.
fn blocked_by_ids(item: &Value) -> Vec<String> {
    item["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

// ─── Not found / not linked ─────────────────────────────────────────────

#[tokio::test]
async fn dispatching_a_ghost_sprint_id_yields_404() {
    let (app, _) = app_with_state(orch_config()).await;
    let res = req(
        &app,
        Method::POST,
        &format!("/api/sprints/{}/dispatch", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sprint_dispatch_409s_until_the_project_gets_linked() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/sprints/{sprint_id}/dispatch"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

// ─── Empty sprint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn dry_run_reports_empty_plan_for_sprint_with_no_items() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, None).await;
    assert_eq!(v["items"], json!([]));
    assert_eq!(v["summary"]["total"], 0);
}

// ─── Diamond dependency graph ──────────────────────────────────────────────
//
//        A
//       / \
//      B   C
//       \ /
//        D
//
// A blocks B and C; B and C both block D.

async fn seed_diamond(app: &Router, project_id: Uuid, sprint_id: Uuid) -> [Uuid; 4] {
    let a = create_item(app, project_id, sprint_id, "A").await;
    let b = create_item(app, project_id, sprint_id, "B").await;
    let c = create_item(app, project_id, sprint_id, "C").await;
    let d = create_item(app, project_id, sprint_id, "D").await;
    create_dependency(app, a, b).await; // A blocks B
    create_dependency(app, a, c).await; // A blocks C
    create_dependency(app, b, d).await; // B blocks D
    create_dependency(app, c, d).await; // C blocks D
    [a, b, c, d]
}

#[tokio::test]
async fn dry_run_diamond_orders_topologically_and_gates_downstream() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);

    // A strictly before B and C; B and C strictly before D.
    assert!(order_of(items, a) < order_of(items, b));
    assert!(order_of(items, a) < order_of(items, c));
    assert!(order_of(items, b) < order_of(items, d));
    assert!(order_of(items, c) < order_of(items, d));

    // A has no unmet dependency and is in `dispatch_from` — a real run
    // would dispatch it.
    assert_eq!(item_by_id(items, a)["decision"], "would_dispatch");

    // B and C both directly depend on A, which has not reached a
    // Done-category status yet — held back regardless of A's own
    // eligibility.
    assert_eq!(item_by_id(items, b)["decision"], "waiting_on_dependencies");
    assert_eq!(item_by_id(items, b)["blocked_by"], json!([a.to_string()]));
    assert_eq!(item_by_id(items, c)["decision"], "waiting_on_dependencies");
    assert_eq!(item_by_id(items, c)["blocked_by"], json!([a.to_string()]));

    // D directly depends on B and C (not A) — its own `blocked_by` names
    // exactly its direct blockers, not the whole transitive ancestry.
    assert_eq!(item_by_id(items, d)["decision"], "waiting_on_dependencies");
    let d_blocked_by = blocked_by_ids(item_by_id(items, d));
    assert_eq!(d_blocked_by.len(), 2);
    assert!(d_blocked_by.contains(&b.to_string()));
    assert!(d_blocked_by.contains(&c.to_string()));
}

#[tokio::test]
async fn completed_dependency_unblocks_direct_dependents_only() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Simulate A having already finished (e.g. dispatched and completed in
    // an earlier sprint-dispatch run).
    patch_status(&app, a, "Done").await;

    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    let item = |id: Uuid| {
        items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap()
    };

    // A is Done, so it's outside `dispatch_from` now — not_eligible, not an
    // error.
    assert_eq!(item(a)["decision"], "not_eligible");
    // B and C's only dependency is satisfied — ready to dispatch.
    assert_eq!(item(b)["decision"], "would_dispatch");
    assert_eq!(item(c)["decision"], "would_dispatch");
    // D still depends on B and C directly, and neither has finished yet.
    assert_eq!(item(d)["decision"], "waiting_on_dependencies");
}

#[tokio::test]
async fn real_dispatch_matches_dry_run_order_and_skip_set() {
    let server = MockServer::start().await;
    mock_enqueue_allow(&server, "task-diamond").await;
    mock_list_tasks(&server, "task-diamond").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;
    patch_status(&app, a, "Done").await; // B and C are ready; D is not.

    let planned = dry_run(&app, sprint_id, None).await;
    let real = dispatch_sprint(&app, sprint_id, None).await;

    let planned_items = planned["items"].as_array().unwrap();
    let real_items = real["items"].as_array().unwrap();
    assert_eq!(planned_items.len(), real_items.len());

    for id in [a, b, c, d] {
        let p = item_by_id(planned_items, id);
        let r = item_by_id(real_items, id);
        assert_eq!(p["order"], r["order"], "order diverged for item {id}");
        match p["decision"].as_str().unwrap() {
            // A dry-run "would_dispatch" preview becomes a real "dispatched"
            // outcome once docket is actually called.
            "would_dispatch" => assert_eq!(r["decision"], "dispatched"),
            // Every other decision (not_eligible, waiting_on_dependencies)
            // is identical between the preview and the real run — neither
            // touches docket.
            other => assert_eq!(r["decision"], other),
        }
    }

    assert_eq!(real["summary"]["dispatched"], 2); // B and C
    assert_eq!(real["summary"]["waiting_on_dependencies"], 1); // D
    assert_eq!(real["summary"]["not_eligible"], 1); // A (already Done)
}

// ─── Cross-sprint / cross-project dependency (decision 2) ─────────────────

#[tokio::test]
async fn dependency_outside_the_sprint_gates_readiness_the_same_way() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let other_sprint_id = create_sprint(&app, project_id).await;

    // `blocker` is in the same project but a *different* sprint — never
    // dispatched by this call at all.
    let blocker = create_item(&app, project_id, other_sprint_id, "Outside blocker").await;
    let dependent = create_item(&app, project_id, sprint_id, "Inside dependent").await;
    create_dependency(&app, blocker, dependent).await;

    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Not done yet — held back, and `blocked_by` still names the external
    // item.
    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "the blocker is outside the sprint, not part of the plan"
    );
    assert_eq!(items[0]["decision"], "waiting_on_dependencies");
    assert_eq!(items[0]["blocked_by"], json!([blocker.to_string()]));

    // Once the external blocker finishes, the in-sprint item becomes ready.
    patch_status(&app, blocker, "Done").await;
    let v2 = dry_run(&app, sprint_id, None).await;
    assert_eq!(
        v2["items"].as_array().unwrap()[0]["decision"],
        "would_dispatch"
    );
}

// ─── Partial failure: one item's policy block doesn't abort the sprint ────

#[tokio::test]
async fn policy_block_on_one_item_does_not_abort_the_sprint() {
    let server = MockServer::start().await;
    // "Blocked Item" is refused by docket's pre_input policy.
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(json!({"description": "Blocked Item"})))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "ok": false,
            "error": "task rejected by guardrail policy 'prompt-injection' at enqueue: looks unsafe"
        })))
        .mount(&server)
        .await;
    // Everything else is allowed.
    mock_enqueue_allow(&server, "task-ok").await;
    mock_list_tasks(&server, "task-ok").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    // Two independent items — no dependency between them.
    let blocked = create_item(&app, project_id, sprint_id, "Blocked Item").await;
    let ok = create_item(&app, project_id, sprint_id, "OK Item").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dispatch_sprint(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    let blocked_row = item_by_id(items, blocked);
    let ok_row = item_by_id(items, ok);

    assert_eq!(blocked_row["decision"], "blocked");
    assert_eq!(blocked_row["policy_id"], "prompt-injection");
    assert_eq!(ok_row["decision"], "dispatched");
    assert_eq!(v["summary"]["blocked"], 1);
    assert_eq!(v["summary"]["dispatched"], 1);
}

// ─── Trust is threaded per item, not a blanket value for the batch ────────

#[tokio::test]
async fn trust_is_threaded_per_item_not_one_blanket_value() {
    let server = MockServer::start().await;
    mock_enqueue_trusted(&server, "Manual Item", true, "task-manual").await;
    mock_enqueue_trusted(&server, "GitHub Item", false, "task-github").await;
    mock_list_tasks_many(&server, &["task-manual", "task-github"]).await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let manual = create_item(&app, project_id, sprint_id, "Manual Item").await;
    let github = seed_github_item(&state, project_id, sprint_id, "GitHub Item").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dispatch_sprint(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    // If the wrong `trusted` value had been sent, the matching mock above
    // would never have matched and `dispatch_item` would have gotten no
    // response at all — a hung/failed request, not a wrong-but-present
    // outcome. Reaching "dispatched" for both proves both values landed on
    // the wire correctly.
    assert_eq!(item_by_id(items, manual)["decision"], "dispatched");
    assert_eq!(item_by_id(items, github)["decision"], "dispatched");
}

// ─── In-flight cap: reported, clamped, and actually bounds concurrency ────

#[tokio::test]
async fn max_in_flight_is_clamped_into_range_and_reported_back() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, Some(0)).await;
    assert_eq!(v["max_in_flight"], 1, "0 clamps up to the floor of 1");

    let v = dry_run(&app, sprint_id, Some(999)).await;
    assert_eq!(
        v["max_in_flight"], 20,
        "999 clamps down to MAX_MAX_IN_FLIGHT"
    );

    let v = dry_run(&app, sprint_id, Some(7)).await;
    assert_eq!(
        v["max_in_flight"], 7,
        "an in-range value passes through unchanged"
    );
}

/// A project + sprint with 4 items, linked to a control plane whose mock
/// enqueue is deliberately slow (150ms) — the fixture `max_in_flight`'s
/// concurrency-bound cases share, varying only the cap.
async fn setup_slow_dispatch_sprint() -> (Router, Uuid) {
    let server = MockServer::start().await;
    mock_enqueue_slow(&server, "task-slow", 150).await;
    mock_list_tasks(&server, "task-slow").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    for i in 0..4 {
        create_item(&app, project_id, sprint_id, &format!("Item {i}")).await;
    }
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;
    (app, sprint_id)
}

/// A tight cap (2, under the 4-item count) forces at least two sequential
/// enqueue batches at 150ms each — comfortably over 260ms; a generous cap
/// (4, at the item count) lets every independent item dispatch essentially
/// at once, comfortably under it. Same fixture, same 150ms-delayed mock,
/// opposite elapsed-time bound — proving the cap actually gates concurrency,
/// not just that it's echoed back (see `max_in_flight_is_clamped_into_range_and_reported_back`
/// for that).
#[tokio::test]
async fn max_in_flight_bounds_actual_dispatch_concurrency() {
    // (cap, elapsed must be at_least the bound (else less than it), bound, note).
    let cases = [
        (
            2,
            true,
            Duration::from_millis(260),
            "cap=2 over 4 items with a 150ms enqueue delay should take at least \
             two sequential batches (~300ms)",
        ),
        (
            4,
            false,
            Duration::from_millis(280),
            "cap=4 over 4 independent items with a 150ms enqueue delay should \
             run concurrently in ~150ms",
        ),
    ];

    for (cap, at_least, bound, note) in cases {
        let (app, sprint_id) = setup_slow_dispatch_sprint().await;

        let start = Instant::now();
        let v = dispatch_sprint(&app, sprint_id, Some(cap)).await;
        let elapsed = start.elapsed();
        assert_eq!(v["summary"]["dispatched"], 4);
        if at_least {
            assert!(elapsed >= bound, "{note}, took {elapsed:?}");
        } else {
            assert!(elapsed < bound, "{note}, took {elapsed:?}");
        }
    }
}
