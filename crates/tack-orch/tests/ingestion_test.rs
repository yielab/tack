//! End-to-end integration tests for runs + approvals ingestion: a real
//! `tack_db::Repository` (in-memory
//! SQLite, real migrations), a real `DocketAdapter` pointed at a `wiremock`
//! stand-in for docket, and the real `reconciler::spawn_reconcilers` loop —
//! proving the whole chain (fetch → correlate → persist) composes
//! correctly, not just that each piece compiles against the others' types.
//!
//! `TestRepoStore` below is a local, test-only `ControlPlaneStore` impl
//! wrapping `Repository` directly. It cannot be the real
//! `tack-api::orch_store::RepoControlPlaneStore` — `tack-orch` must never
//! depend on `tack-api` (see this crate's `Cargo.toml` header comment) —
//! but it is deliberately written to the exact same
//! mechanical shape `orch_store.rs` needs (a thin pass-through per method,
//! no correlation logic), so a passing test here is strong evidence the
//! trait is straightforward to implement for real.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use tack_core::models::{CreateItem, CreateProject, ItemType, Priority, ProjectType};
use tack_core::vocabulary;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchMetric, NewOrchRun, NewOrchTask, UpsertOrchLink,
};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, ReconcilerConfig, RegisteredPlane, spawn_reconcilers,
};
use tack_orch::{ControlPlane, OrchError};

// ---------------------------------------------------------------------------
// Test fixtures / setup
// ---------------------------------------------------------------------------

async fn setup_repo() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

async fn seed_workspace(repo: &Repository) -> Uuid {
    let id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test Workspace', ?)",
    )
    .bind(id.to_string())
    .bind(&vocab)
    .execute(repo.pool())
    .await
    .expect("insert workspace");
    id
}

async fn seed_project(repo: &Repository, workspace_id: Uuid) -> tack_core::models::Project {
    repo.create_project(
        workspace_id,
        CreateProject {
            name: "Test Project".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        },
    )
    .await
    .expect("create project")
}

async fn seed_item(
    repo: &Repository,
    project: &tack_core::models::Project,
) -> tack_core::models::Item {
    let status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();
    repo.create_item(
        project.id,
        &status,
        CreateItem {
            title: "Test Item".into(),
            description: None,
            item_type: Some(ItemType::Task),
            parent_id: None,
            priority: Some(Priority::Medium),
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
            assignee: None,
        },
    )
    .await
    .expect("create item")
}

/// A [`ControlPlaneStore`] backed directly by a real `Repository` — the
/// test-only stand-in for `tack-api::orch_store::RepoControlPlaneStore`
/// (see the module doc for why this can't just import that type).
struct TestRepoStore {
    repo: Repository,
}

#[async_trait::async_trait]
impl ControlPlaneStore for TestRepoStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        let rows = self
            .repo
            .list_control_planes()
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;

        let mut planes = Vec::new();
        for row in rows {
            if row.kind != "docket" {
                continue;
            }
            let token = self
                .repo
                .get_control_plane_token(row.id)
                .await
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            let adapter = DocketAdapter::new(row.base_url.clone(), token)
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            planes.push(RegisteredPlane {
                id: row.id,
                control_plane: Arc::new(adapter) as Arc<dyn ControlPlane>,
            });
        }
        Ok(planes)
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.repo
            .update_control_plane_health(
                control_plane_id,
                record.health.as_str(),
                record.last_seen_at,
                record.consecutive_failures,
                record.api_version.as_deref(),
            )
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError> {
        let links = self
            .repo
            .list_orch_links_for_plane(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(links.into_iter().map(|l| l.remote_project).collect())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        let task = self
            .repo
            .find_orch_task_by_remote_task_id(remote_task_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(task.map(|t| t.item_id))
    }

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_runs(control_plane_id, runs)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_approvals(control_plane_id, approvals)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    // Added when `ControlPlaneStore` grew
    // `upsert_metrics`, mirroring the exact same mechanical-pass-through shape
    // every other method here already has. This test file's own tests don't
    // exercise metrics ingestion (see orch_metrics_test.rs / reconciler.rs's
    // own unit tests for that) — this impl exists purely so `TestRepoStore`
    // keeps satisfying the trait.
    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_metrics(control_plane_id, metrics)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    // Added when `ControlPlaneStore` grew
    // three more methods for `orch_trace_cursors`/`orch_events`, same
    // mechanical-pass-through shape as everything above. This test file's
    // own tests don't exercise trace ingestion (see
    // `traces_ingestion_test.rs`/`reconciler.rs`'s own unit tests for that)
    // — this impl exists purely so `TestRepoStore` keeps satisfying the
    // trait.
    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, OrchError> {
        let cursors = self
            .repo
            .list_trace_cursors(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(cursors
            .into_iter()
            .map(|c| (c.remote_project, c.cursor))
            .collect())
    }

    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.repo
            .set_trace_cursor(control_plane_id, remote_project, cursor)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[tack_db::repo::orch::NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_events(control_plane_id, events)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }
}

const HEALTH_BODY: &str = r#"{"status":"ok","gateway":0}"#;
const STATUS_BODY: &str = r#"{"apiVersion":"2","timestamp":"2026-08-04T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#;
const EMPTY_APPROVALS_BODY: &str = r#"{"pending":[]}"#;

fn run_json(id: &str, task_ids: &str) -> String {
    format!(
        r#"{{"id":"{id}","source":"cli","project":"demo","state":"succeeded","taskIds":{task_ids},
        "error":"","created":"2026-08-04T19:50:43.129083+00:00",
        "startedAt":"2026-08-04T19:50:43.129674+00:00",
        "finishedAt":"2026-08-04T19:50:43.130194+00:00","pids":[],"variables":{{}}}}"#
    )
}

async fn mount_common(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEALTH_BODY))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(STATUS_BODY))
        .mount(server)
        .await;
}

async fn seed_control_plane_and_link(repo: &Repository, project_id: Uuid, base_url: &str) -> Uuid {
    let plane = repo
        .create_control_plane(CreateControlPlane {
            name: "Test Docket".into(),
            kind: None,
            base_url: base_url.to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane.id,
            remote_project: "demo".into(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map: serde_json::json!({}),
        },
    )
    .await
    .expect("create orch link");

    plane.id
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn correlated_and_uncorrelated_runs_and_approvals_mirror_idempotently() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;
    let item = seed_item(&repo, &project).await;

    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id: item.id,
        remote_task_id: "task-1".into(),
        remote_run_id: None,
        remote_status: "pending".into(),
        attempt: 1,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("seed orch task");

    let server = MockServer::start().await;
    mount_common(&server).await;

    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"runs":[{},{}]}}"#,
            run_json("run-1", r#"["task-1"]"#),
            run_json("run-cli-only", "[]"),
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"pending":[
                {"token":"apr-1","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:50Z",
                 "context":{"taskId":"task-1","pipelineIndex":0}},
                {"token":"apr-uncorrelated","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:51Z","context":{}}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let control_plane_id = seed_control_plane_and_link(&repo, project.id, &server.uri()).await;

    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // One tick is enough for the first poll's correlation to land.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let run1 = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 must be mirrored");
    assert_eq!(run1.item_id, Some(item.id), "run-1's task_ids correlate");

    let run_cli = repo
        .get_orch_run("run-cli-only")
        .await
        .expect("query")
        .expect("run-cli-only must still be mirrored, unattributed");
    assert_eq!(
        run_cli.item_id, None,
        "an empty task_ids run must land unattributed, not be dropped or error"
    );

    let apr1 = repo
        .get_orch_approval("apr-1")
        .await
        .expect("query")
        .expect("apr-1 must be mirrored");
    assert_eq!(apr1.item_id, Some(item.id));
    assert_eq!(apr1.remote_task_id.as_deref(), Some("task-1"));

    let apr_uncorrelated = repo
        .get_orch_approval("apr-uncorrelated")
        .await
        .expect("query")
        .expect("an uncorrelated approval must still surface, not be dropped");
    assert_eq!(apr_uncorrelated.item_id, None);

    // Let at least one more tick happen (poll_secs=1, jittered 0.8–1.2s) and
    // confirm re-polling the exact same docket state is idempotent: no
    // duplicate rows, same content.
    tokio::time::sleep(Duration::from_millis(1_400)).await;
    for h in handles {
        h.abort();
    }

    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(repo.pool())
        .await
        .expect("count runs");
    assert_eq!(run_count, 2, "re-polling must not duplicate orch_runs rows");

    let approval_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_approvals")
        .fetch_one(repo.pool())
        .await
        .expect("count approvals");
    assert_eq!(
        approval_count, 2,
        "re-polling must not duplicate orch_approvals rows"
    );

    // Control-plane health must still read healthy — an /approvals or /runs
    // failure never happened here, but this also proves the ingestion
    // machinery didn't somehow interfere with the health persistence path.
    let plane = repo
        .get_control_plane(control_plane_id)
        .await
        .expect("get control plane");
    assert_eq!(plane.health, "healthy");
}

/// A `Respond` impl that returns a different body on each successive call,
/// repeating the last body forever once the list is exhausted — used to
/// simulate a run's `taskIds` becoming known and then (unrealistically, but
/// this is exactly the case the DB-layer COALESCE guards against) reverting
/// to unknown on a later poll.
struct SequentialBody {
    bodies: Vec<String>,
    calls: AtomicUsize,
}

impl Respond for SequentialBody {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        let body = self
            .bodies
            .get(idx)
            .or_else(|| self.bodies.last())
            .cloned()
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_string(body)
    }
}

#[tokio::test]
async fn a_later_poll_does_not_erase_an_earlier_run_attribution() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;
    let item = seed_item(&repo, &project).await;

    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id: item.id,
        remote_task_id: "task-1".into(),
        remote_run_id: None,
        remote_status: "pending".into(),
        attempt: 1,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("seed orch task");

    let server = MockServer::start().await;
    mount_common(&server).await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_APPROVALS_BODY))
        .mount(&server)
        .await;

    // First poll: task_ids known, correlates. Every poll after: task_ids
    // empty again — simulating a poll that "forgot" the attribution. The
    // repo's ON CONFLICT ... COALESCE(excluded.item_id, item_id) must keep
    // the first poll's attribution regardless.
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(SequentialBody {
            bodies: vec![format!(
                r#"{{"runs":[{}]}}"#,
                run_json("run-1", r#"["task-1"]"#)
            )],
            calls: AtomicUsize::new(0),
        })
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"{{"runs":[{}]}}"#, run_json("run-1", "[]"))),
        )
        .mount(&server)
        .await;

    let control_plane_id = seed_control_plane_and_link(&repo, project.id, &server.uri()).await;
    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // First tick: correlated.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let run_after_first_poll = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 mirrored");
    assert_eq!(run_after_first_poll.item_id, Some(item.id));

    // Two more ticks, each returning an empty task_ids list.
    tokio::time::sleep(Duration::from_millis(2_400)).await;
    for h in handles {
        h.abort();
    }

    let run_after_later_polls = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 still mirrored");
    assert_eq!(
        run_after_later_polls.item_id,
        Some(item.id),
        "a later poll that doesn't know the attribution must never erase one already learned"
    );

    let _ = control_plane_id;
}
