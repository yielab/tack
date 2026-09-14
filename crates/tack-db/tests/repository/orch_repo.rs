//! Integration tests for the `orch` repository module: CRUD for
//! `control_planes` and `orch_links`, plus batch upsert helpers for
//! `orch_tasks`, `orch_runs`, `orch_events`, `orch_approvals`. Covers: the
//! control-plane read DTO never carries the stored token, including
//! serialized; batch upserts of N rows are idempotent; unrecognised
//! remote-state strings are stored and returned as-is.

use crate::common::{create_test_workspace, make_item, make_project, setup_test_db};
use chrono::{DateTime, Utc};
use tack_db::repo::orch::{
    ControlPlane, CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchRun, NewOrchTask,
    OrchLink, PendingOrchApproval, UpdateControlPlane, UpsertOrchLink,
};
use uuid::Uuid;

/// A control plane named `name`, with `token` stored if given.
async fn create_plane(repo: &tack_db::Repository, name: &str, token: Option<&str>) -> ControlPlane {
    repo.create_control_plane(CreateControlPlane {
        name: name.into(),
        kind: None,
        base_url: "http://localhost:9999".into(),
        token: token.map(String::from),
    })
    .await
    .expect("create control plane")
}

/// Sets (`Some(t)`) or explicitly clears (`None`) a plane's stored token.
async fn update_token(repo: &tack_db::Repository, id: Uuid, token: Option<&str>) -> ControlPlane {
    repo.update_control_plane(
        id,
        UpdateControlPlane {
            token: Some(token.map(String::from)),
            ..Default::default()
        },
    )
    .await
    .expect("update token")
}

// ════════════════════════════════════════════════════════════════════════════════════
// control_planes
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_get_list_control_plane() {
    let repo = setup_test_db().await;

    let created = create_plane(&repo, "Primary Docket", Some("super-secret-token")).await;

    assert_eq!(created.kind, "docket", "kind defaults to docket");
    assert_eq!(created.health, "unknown");
    assert!(
        created.token_set,
        "token_set must be true once a token is stored"
    );

    let fetched = repo
        .get_control_plane(created.id)
        .await
        .expect("get control plane");
    assert_eq!(fetched.name, "Primary Docket");
    assert!(fetched.token_set);

    let listed = repo.list_control_planes().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[tokio::test]
async fn control_plane_without_token_reports_token_set_false() {
    let repo = setup_test_db().await;

    let created = create_plane(&repo, "No Token Plane", None).await;

    assert!(!created.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id)
            .await
            .expect("get token"),
        None
    );
}

/// Non-negotiable #2: the read DTO must never carry the token, in Rust or on the wire.
#[tokio::test]
async fn control_plane_read_dto_never_exposes_token() {
    let repo = setup_test_db().await;

    let created = create_plane(&repo, "Secret Plane", Some("do-not-leak-me")).await;
    let fetched = repo
        .get_control_plane(created.id)
        .await
        .expect("get control plane");

    // The struct itself has no field to hold a token — this is a compile-time
    // guarantee, not just a runtime one (no `.token` field exists on `ControlPlane`).
    // The runtime check below asserts the *serialized* form is equally clean.
    let json = serde_json::to_string(&fetched).expect("serialize control plane");
    assert!(
        !json.contains("do-not-leak-me"),
        "serialized control plane must not contain the raw token value: {json}"
    );
    assert!(
        !json.contains("\"token\""),
        "serialized control plane must not contain a `token` key: {json}"
    );
    assert!(
        json.contains("\"token_set\":true"),
        "serialized control plane must expose token_set: {json}"
    );

    // The internal-only accessor is the sole way to retrieve the real value.
    let real_token = repo
        .get_control_plane_token(created.id)
        .await
        .expect("get token");
    assert_eq!(real_token.as_deref(), Some("do-not-leak-me"));
}

#[tokio::test]
async fn update_control_plane_name_and_base_url() {
    let repo = setup_test_db().await;
    let created = create_plane(&repo, "Old Name", None).await;

    let updated = repo
        .update_control_plane(
            created.id,
            UpdateControlPlane {
                name: Some("New Name".into()),
                base_url: Some("http://new:9999".into()),
                token: None,
            },
        )
        .await
        .expect("update");

    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.base_url, "http://new:9999");
    assert!(!updated.token_set, "token untouched, was never set");
}

#[tokio::test]
async fn update_control_plane_token_set_then_clear() {
    let repo = setup_test_db().await;
    let created = create_plane(&repo, "Plane", None).await;
    assert!(!created.token_set);

    // Absent `token` field (None) preserves the stored value (still unset).
    let unchanged = repo
        .update_control_plane(created.id, UpdateControlPlane::default())
        .await
        .expect("no-op update");
    assert!(!unchanged.token_set);

    // Some(Some(t)) sets it.
    let with_token = update_token(&repo, created.id, Some("fresh-token")).await;
    assert!(with_token.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id).await.unwrap(),
        Some("fresh-token".to_string())
    );

    // Some(None) explicitly clears it.
    let cleared = update_token(&repo, created.id, None).await;
    assert!(!cleared.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn update_control_plane_health_state_machine_persists() {
    let repo = setup_test_db().await;
    let created = create_plane(&repo, "Plane", None).await;

    let seen_at = Utc::now();
    repo.update_control_plane_health(created.id, "degraded", None, 3, None)
        .await
        .expect("record failed poll");

    let after_failure = repo.get_control_plane(created.id).await.unwrap();
    assert_eq!(after_failure.health, "degraded");
    assert_eq!(after_failure.consecutive_failures, 3);
    // last_seen_at untouched by a failed poll (we passed None).
    assert_eq!(after_failure.last_seen_at, None);

    repo.update_control_plane_health(created.id, "healthy", Some(seen_at), 0, Some("2"))
        .await
        .expect("record recovery");

    let after_recovery = repo.get_control_plane(created.id).await.unwrap();
    assert_eq!(after_recovery.health, "healthy");
    assert_eq!(after_recovery.consecutive_failures, 0);
    assert!(after_recovery.last_seen_at.is_some());
    assert_eq!(after_recovery.api_version.as_deref(), Some("2"));
}

#[tokio::test]
async fn delete_control_plane() {
    let repo = setup_test_db().await;
    let created = create_plane(&repo, "Plane", None).await;

    assert!(repo.delete_control_plane(created.id).await.expect("delete"));
    assert!(
        !repo
            .delete_control_plane(created.id)
            .await
            .expect("delete again")
    );
    assert!(repo.get_control_plane(created.id).await.is_err());
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_links
// ════════════════════════════════════════════════════════════════════════════════════

async fn make_control_plane(repo: &tack_db::Repository) -> Uuid {
    create_plane(repo, "Plane", None).await.id
}

#[allow(clippy::too_many_arguments)]
async fn upsert_link(
    repo: &tack_db::Repository,
    project_id: Uuid,
    plane_id: Uuid,
    remote_project: &str,
    pipeline_file: Option<&str>,
    blueprint: Option<&str>,
    auto_dispatch: bool,
    budget_usd: Option<f64>,
    status_map: serde_json::Value,
) -> OrchLink {
    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane_id,
            remote_project: remote_project.into(),
            pipeline_file: pipeline_file.map(String::from),
            blueprint: blueprint.map(String::from),
            auto_dispatch,
            budget_usd,
            status_map,
        },
    )
    .await
    .expect("upsert link")
}

/// `upsert_link` with pipeline/blueprint/status_map wired to whether the
/// link auto-dispatches (matching the two shapes this file exercises).
async fn simple_link(
    repo: &tack_db::Repository,
    project_id: Uuid,
    plane_id: Uuid,
    remote_project: &str,
    auto_dispatch: bool,
    budget_usd: Option<f64>,
) -> OrchLink {
    let (pipeline, blueprint, status_map) = if auto_dispatch {
        (None, None, serde_json::json!({}))
    } else {
        (
            Some("pipeline.yaml"),
            Some("software"),
            serde_json::json!({"dispatch_from": ["Ready"]}),
        )
    };
    upsert_link(
        repo,
        project_id,
        plane_id,
        remote_project,
        pipeline,
        blueprint,
        auto_dispatch,
        budget_usd,
        status_map,
    )
    .await
}

#[tokio::test]
async fn upsert_get_delete_orch_link() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let plane_id = make_control_plane(&repo).await;

    let pid = project.id;
    assert_eq!(repo.get_orch_link(pid).await.unwrap(), None);

    let link = simple_link(&repo, pid, plane_id, "demo-remote", false, Some(50.0)).await;
    assert_eq!(link.project_id, pid);
    assert_eq!(link.remote_project, "demo-remote");
    assert_eq!(link.budget_usd, Some(50.0));

    // Re-upsert (ON CONFLICT(project_id)) replaces the row, not duplicates it.
    let updated = simple_link(&repo, pid, plane_id, "demo-remote-renamed", true, None).await;
    assert_eq!(updated.remote_project, "demo-remote-renamed");
    assert!(updated.auto_dispatch);
    assert_eq!(updated.budget_usd, None);

    let for_plane = repo.list_orch_links_for_plane(plane_id).await.unwrap();
    assert_eq!(for_plane.len(), 1, "still exactly one link, not two");

    assert!(repo.delete_orch_link(project.id).await.unwrap());
    assert_eq!(repo.get_orch_link(project.id).await.unwrap(), None);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_tasks — batch upsert idempotency + composite PK
// ════════════════════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
fn task(
    item_id: Uuid,
    remote_task_id: &str,
    remote_run_id: Option<&str>,
    status: &str,
    attempt: i64,
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: Option<f64>,
    trusted: bool,
) -> NewOrchTask {
    NewOrchTask {
        item_id,
        remote_task_id: remote_task_id.into(),
        remote_run_id: remote_run_id.map(String::from),
        remote_status: status.to_string(),
        attempt,
        tokens_in,
        tokens_out,
        cost_usd_estimated,
        dispatched_at: Utc::now(),
        trusted,
    }
}

async fn task_count(repo: &tack_db::Repository, item_id: Uuid) -> usize {
    repo.list_orch_tasks_for_item(item_id).await.unwrap().len()
}

/// A task in the two-row batch below: same attempt (1) and run ("run-1"),
/// varying only id/tokens/cost/trust.
fn batch_task(
    item_id: Uuid,
    status: &str,
    id: &str,
    tokens_in: i64,
    tokens_out: i64,
    cost: Option<f64>,
    trusted: bool,
) -> NewOrchTask {
    task(
        item_id,
        id,
        Some("run-1"),
        status,
        1,
        tokens_in,
        tokens_out,
        cost,
        trusted,
    )
}

#[tokio::test]
async fn upsert_orch_tasks_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

    let make_batch = |status: &str| {
        vec![
            batch_task(item.id, status, "task-1", 100, 50, Some(0.01), true),
            batch_task(item.id, status, "task-2", 10, 5, None, false),
        ]
    };

    let first = repo.upsert_orch_tasks(&make_batch("running")).await;
    first.expect("first upsert");
    assert_eq!(task_count(&repo, item.id).await, 2);

    // Re-upsert the identical (item_id, remote_task_id) pairs with different content —
    // must update in place, not duplicate, and must not error.
    let second = repo.upsert_orch_tasks(&make_batch("done")).await;
    second.expect("idempotent second upsert");
    let after_second = repo.list_orch_tasks_for_item(item.id).await.unwrap();
    assert_eq!(after_second.len(), 2, "no duplicate rows from re-upserting");
    assert!(
        after_second.iter().all(|t| t.remote_status == "done"),
        "re-upsert should have refreshed remote_status in place"
    );

    // Composite PK: a *different* remote_task_id for the same item is a new row (a
    // redispatch), not an update of an existing one.
    let retry = task(item.id, "task-3", None, "pending", 2, 0, 0, None, true);
    repo.upsert_orch_tasks(&[retry])
        .await
        .expect("redispatch as new row");
    assert_eq!(task_count(&repo, item.id).await, 3);

    repo.upsert_orch_tasks(&[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn get_and_find_orch_task() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

    assert_eq!(
        repo.get_orch_task(item.id, "nonexistent").await.unwrap(),
        None
    );

    // An unrecognised docket status string must round-trip untouched — this
    // layer never validates/rejects it.
    let unrecognised = "some_future_status_tack_has_never_seen";
    let t = task(item.id, "task-abc", None, unrecognised, 1, 0, 0, None, true);
    repo.upsert_orch_tasks(&[t]).await.expect("upsert");

    let fetched = repo
        .get_orch_task(item.id, "task-abc")
        .await
        .unwrap()
        .expect("task exists");
    assert_eq!(fetched.remote_status, unrecognised);

    let found = repo
        .find_orch_task_by_remote_task_id("task-abc")
        .await
        .unwrap()
        .expect("found by remote_task_id alone");
    assert_eq!(found.item_id, item.id);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_runs — batch upsert idempotency, unattributed runs
// ════════════════════════════════════════════════════════════════════════════════════

/// Builds a `NewOrchRun` with `remote_project: "demo"` and `error: None`
/// pinned — the fields this file never varies.
fn run(
    run_id: &str,
    item_id: Option<Uuid>,
    source: &str,
    state: &str,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
) -> NewOrchRun {
    NewOrchRun {
        run_id: run_id.into(),
        item_id,
        remote_project: "demo".into(),
        source: source.into(),
        state: state.into(),
        started_at,
        ended_at,
        error: None,
    }
}

#[tokio::test]
async fn orch_runs_upsert_idempotent_keeps_unattributed_runs() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    // dispatched from the docket CLI, not via Tack — normal case for run-cli-1.
    let unrecognised_source = "totally_new_source_docket_invented";
    let now = Some(Utc::now());
    let batch = vec![
        run("run-cli-1", None, "cli", "queued", None, None),
        run("run-cli-2", None, unrecognised_source, "running", now, None),
    ];

    repo.upsert_orch_runs(plane_id, &batch)
        .await
        .expect("first upsert");
    assert!(repo.get_orch_run("run-cli-1").await.unwrap().is_some());

    // Re-upload the same batch with an updated state — idempotent, no duplicate rows
    // (run_id is the PK), no error.
    let mut updated_batch = batch;
    updated_batch[0].state = "succeeded".into();
    updated_batch[0].ended_at = Some(Utc::now());
    repo.upsert_orch_runs(plane_id, &updated_batch)
        .await
        .expect("idempotent second upsert");

    let run = repo.get_orch_run("run-cli-1").await.unwrap().unwrap();
    assert_eq!(run.state, "succeeded");
    assert!(run.item_id.is_none(), "still unattributed");

    let run2 = repo.get_orch_run("run-cli-2").await.unwrap().unwrap();
    assert_eq!(
        run2.source, unrecognised_source,
        "unrecognised source stored as-is"
    );

    repo.upsert_orch_runs(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn orch_run_attribution_is_never_unlearned() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    // First poll: no item_id known yet.
    let poll1 = run("run-x", None, "cli", "running", None, None);
    repo.upsert_orch_runs(plane_id, &[poll1]).await.unwrap();

    // A later poll learns the attribution.
    let poll2 = run("run-x", Some(item.id), "cli", "running", None, None);
    repo.upsert_orch_runs(plane_id, &[poll2]).await.unwrap();
    assert_eq!(
        repo.get_orch_run("run-x").await.unwrap().unwrap().item_id,
        Some(item.id)
    );

    // A subsequent poll that (correctly) omits item_id must not clear it back to NULL.
    let poll3 = run("run-x", None, "cli", "succeeded", None, Some(Utc::now()));
    repo.upsert_orch_runs(plane_id, &[poll3]).await.unwrap();
    let run = repo.get_orch_run("run-x").await.unwrap().unwrap();
    assert_eq!(run.state, "succeeded");
    assert_eq!(run.item_id, Some(item.id), "known attribution must persist");

    assert_eq!(
        repo.list_orch_runs_for_item(item.id).await.unwrap().len(),
        1
    );
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_events — batch upsert idempotency
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn upsert_orch_events_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let event_id = Uuid::new_v4();
    let batch = vec![NewOrchEvent {
        id: event_id,
        item_id: Some(item.id),
        run_id: Some("run-1".into()),
        event_type: "an_event_type_from_the_future".into(),
        payload: serde_json::json!({"hop": 1}),
        occurred_at: Utc::now(),
    }];

    repo.upsert_orch_events(plane_id, &batch)
        .await
        .expect("first upsert");
    let first = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(first.len(), 1);

    // Re-upserting the same event id (simulating a re-poll of an overlapping cursor
    // window) must not duplicate the row.
    repo.upsert_orch_events(plane_id, &batch)
        .await
        .expect("second upsert must be idempotent");
    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(events.len(), 1, "no duplicate rows from re-upserting");
    assert_eq!(events[0].event_type, "an_event_type_from_the_future");
    assert_eq!(events[0].payload, serde_json::json!({"hop": 1}));

    repo.upsert_orch_events(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn list_orch_events_for_item_respects_limit_and_order() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let base = Utc::now();
    let batch: Vec<NewOrchEvent> = (0..5)
        .map(|i| NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: Some(item.id),
            run_id: None,
            event_type: format!("hop_{i}"),
            payload: serde_json::json!({}),
            occurred_at: base + chrono::Duration::seconds(i),
        })
        .collect();

    repo.upsert_orch_events(plane_id, &batch).await.unwrap();

    let all = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all[0].event_type, "hop_0", "chronological, oldest first");
    assert_eq!(all[4].event_type, "hop_4");

    let limited = repo
        .list_orch_events_for_item(item.id, Some(2))
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_approvals — batch upsert idempotency, uncorrelated records
// ════════════════════════════════════════════════════════════════════════════════════

/// Builds a `NewOrchApproval`; the fields this file varies per case.
#[allow(clippy::too_many_arguments)]
fn approval(
    token: &str,
    item_id: Option<Uuid>,
    remote_task_id: Option<&str>,
    agent: Option<&str>,
    action: Option<&str>,
    state: &str,
    requested_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
) -> NewOrchApproval {
    NewOrchApproval {
        token: token.into(),
        item_id,
        remote_task_id: remote_task_id.map(String::from),
        agent: agent.map(String::from),
        action: action.map(String::from),
        state: state.into(),
        requested_at,
        decided_at,
    }
}

async fn pending_with_context(repo: &tack_db::Repository) -> Vec<PendingOrchApproval> {
    repo.list_pending_orch_approvals_with_context()
        .await
        .unwrap()
}

/// A pending, uncorrelated (`item_id: None`) approval requesting `action`.
fn simple_approval(token: &str, action: &str, requested_at: DateTime<Utc>) -> NewOrchApproval {
    approval(
        token,
        None,
        None,
        None,
        Some(action),
        "pending",
        requested_at,
        None,
    )
}

/// Same as [`simple_approval`] but with an `agent` attributed.
fn agent_approval(
    token: &str,
    agent: &str,
    action: &str,
    requested_at: DateTime<Utc>,
) -> NewOrchApproval {
    approval(
        token,
        None,
        None,
        Some(agent),
        Some(action),
        "pending",
        requested_at,
        None,
    )
}

/// An approval on `item_id` from "builder", requesting "git push" — the
/// fixed shape the correlated-approval tests below share, varying only
/// `state`/`decided_at`.
fn correlated_approval(
    item_id: Uuid,
    state: &str,
    decided_at: Option<DateTime<Utc>>,
) -> NewOrchApproval {
    approval(
        "correlated",
        Some(item_id),
        Some("task-1"),
        Some("builder"),
        Some("git push"),
        state,
        Utc::now(),
        decided_at,
    )
}

#[tokio::test]
async fn upsert_orch_approvals_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    let a = approval(
        "approval-token-1",
        None,
        Some("task-1"),
        Some("builder"),
        Some("git push"),
        "pending",
        Utc::now(),
        None,
    );
    let batch = vec![a];

    repo.upsert_orch_approvals(plane_id, &batch)
        .await
        .expect("first upsert");
    assert!(
        repo.get_orch_approval("approval-token-1")
            .await
            .unwrap()
            .is_some()
    );

    repo.upsert_orch_approvals(plane_id, &batch)
        .await
        .expect("second upsert must be idempotent");
    let pending = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending.len(), 1, "no duplicate rows from re-upserting");

    repo.upsert_orch_approvals(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn uncorrelated_approvals_still_appear_in_pending_inbox() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let requested = Utc::now() - chrono::Duration::seconds(10);
    let uncorrelated = simple_approval("uncorrelated", "rm -rf /tmp/build", requested);
    let correlated_pending = correlated_approval(item.id, "pending", None);
    let both = [uncorrelated, correlated_pending];
    repo.upsert_orch_approvals(plane_id, &both)
        .await
        .expect("upsert both");

    let pending = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending.len(), 2);
    // Oldest first.
    assert_eq!(pending[0].token, "uncorrelated");
    assert!(pending[0].item_id.is_none());
    assert_eq!(pending[1].token, "correlated");
    assert_eq!(pending[1].item_id, Some(item.id));

    // A decision (granted/denied) removes it from the *pending* inbox but the row
    // (and its final state) survives.
    let decided_approval = correlated_approval(item.id, "granted", Some(Utc::now()));
    repo.upsert_orch_approvals(plane_id, &[decided_approval])
        .await
        .unwrap();

    let pending_after = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending_after.len(), 1);
    let decided = repo.get_orch_approval("correlated").await.unwrap().unwrap();
    assert_eq!(decided.state, "granted");
    assert!(decided.decided_at.is_some());
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_metrics
// ════════════════════════════════════════════════════════════════════════════════════

// Covered by `orch_metrics.rs`, not here.

// ════════════════════════════════════════════════════════════════════════════════════
// orch_trace_cursors (migration 028)
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn set_and_list_trace_cursors_scoped_per_plane() {
    let repo = setup_test_db().await;
    let plane_a = make_control_plane(&repo).await;
    let plane_b = make_control_plane(&repo).await;

    assert!(repo.list_trace_cursors(plane_a).await.unwrap().is_empty());

    repo.set_trace_cursor(plane_a, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .expect("set cursor for plane_a/demo");
    repo.set_trace_cursor(plane_a, "other-project", "2026-08-04T00:00:00Z:0")
        .await
        .expect("set cursor for plane_a/other-project");
    // A different plane's cursor for the *same* remote_project name must not
    // collide — the PK is (control_plane_id, remote_project), not remote_project
    // alone (two docket instances can both happen to have a "demo" project).
    repo.set_trace_cursor(plane_b, "demo", "2026-08-03T00:00:00Z:5")
        .await
        .expect("set cursor for plane_b/demo");

    let for_a = repo.list_trace_cursors(plane_a).await.unwrap();
    assert_eq!(for_a.len(), 2, "both of plane_a's projects, not plane_b's");
    let demo_a = for_a.iter().find(|c| c.remote_project == "demo").unwrap();
    assert_eq!(demo_a.cursor, "2026-08-04T19:52:27Z:1");

    let for_b = repo.list_trace_cursors(plane_b).await.unwrap();
    assert_eq!(for_b.len(), 1);
    assert_eq!(for_b[0].cursor, "2026-08-03T00:00:00Z:5");
}

#[tokio::test]
async fn set_trace_cursor_upserts_in_place() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .unwrap();
    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:40Z:2")
        .await
        .unwrap();

    let cursors = repo.list_trace_cursors(plane_id).await.unwrap();
    assert_eq!(
        cursors.len(),
        1,
        "re-setting the same pair updates, not duplicates"
    );
    assert_eq!(cursors[0].cursor, "2026-08-04T19:52:40Z:2");
}

#[tokio::test]
async fn orch_trace_cursor_cascades_on_control_plane_delete() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .unwrap();

    assert!(repo.delete_control_plane(plane_id).await.unwrap());
    assert!(
        repo.list_trace_cursors(plane_id).await.unwrap().is_empty(),
        "ON DELETE CASCADE from control_planes must remove its trace cursors too"
    );
}

// ════════════════════════════════════════════════════════════════════════════════════
// list_pending_orch_approvals_with_context / mark_orch_approval_decided
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pending_approvals_with_context_include_uncorrelated_rows() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let requested = Utc::now() - chrono::Duration::seconds(30);
    let uncorrelated = agent_approval("uncorrelated", "cli-agent", "rm -rf /tmp/build", requested);
    let correlated = correlated_approval(item.id, "pending", None);
    let both = [uncorrelated, correlated];
    let upserted = repo.upsert_orch_approvals(plane_id, &both).await;
    upserted.expect("upsert both");
    let rows = pending_with_context(&repo).await;
    assert_eq!(rows.len(), 2);

    // Oldest first, same ordering guarantee as list_pending_orch_approvals.
    let uncorrelated = &rows[0];
    assert_eq!(uncorrelated.token, "uncorrelated");
    // The whole point of this query: an uncorrelated approval still surfaces.
    assert!(uncorrelated.item_id.is_none());
    assert!(uncorrelated.item_title.is_none());
    assert!(uncorrelated.project_id.is_none());
    assert!(uncorrelated.project_name.is_none());
    assert_eq!(uncorrelated.agent.as_deref(), Some("cli-agent"));
    assert_eq!(uncorrelated.action.as_deref(), Some("rm -rf /tmp/build"));
    // control_plane_name is still populated even when the item is unknown.
    assert_eq!(uncorrelated.control_plane_name, "Plane");

    let correlated = &rows[1];
    let item_status = item.status.as_str();
    let project_name = project.name.as_str();
    assert_eq!(correlated.token, "correlated");
    assert_eq!(correlated.item_id, Some(item.id));
    assert_eq!(correlated.item_title.as_deref(), Some(item.title.as_str()));
    assert_eq!(correlated.item_status.as_deref(), Some(item_status));
    assert_eq!(correlated.project_id, Some(project.id));
    assert_eq!(correlated.project_name.as_deref(), Some(project_name));
}

#[tokio::test]
async fn mark_orch_approval_decided_removes_from_pending_inbox() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    let a = agent_approval("apr-decide-1", "builder", "deploy", Utc::now());
    repo.upsert_orch_approvals(plane_id, &[a]).await.unwrap();

    let pending = pending_with_context(&repo).await;
    assert_eq!(pending.len(), 1);

    let decided_at = Utc::now();
    repo.mark_orch_approval_decided("apr-decide-1", "granted", decided_at)
        .await
        .expect("mark decided");

    let pending_after = pending_with_context(&repo).await;
    assert!(
        pending_after.is_empty(),
        "a decided approval must disappear from the pending inbox"
    );

    let row = repo
        .get_orch_approval("apr-decide-1")
        .await
        .unwrap()
        .expect("row still exists");
    assert_eq!(row.state, "granted");
    assert!(row.decided_at.is_some());
}

#[tokio::test]
async fn mark_orch_approval_decided_on_unknown_token_is_a_no_op() {
    let repo = setup_test_db().await;
    // No row exists for this token at all — must not error (defensive path,
    // see the function's own doc comment).
    repo.mark_orch_approval_decided("does-not-exist", "granted", Utc::now())
        .await
        .expect("unknown token must be a no-op, not an error");
}
