//! Shared fixtures for `runs.rs` and `traces.rs`: both stand up a real,
//! migrated `tack_db::Repository` with one workspace/project/item, mount the
//! same `/health` and `/status.json` wiremock responses, and link a control
//! plane to the seeded project through the same `TestRepoStore` —
//! a test-only `ControlPlaneStore` impl wrapping `Repository` directly. It
//! cannot be the real `tack-api::orch_store::RepoControlPlaneStore` —
//! `tack-orch` must never depend on `tack-api` (see this crate's
//! `Cargo.toml` header comment) — but it is deliberately written to the
//! exact same mechanical shape `orch_store.rs` needs (a thin pass-through
//! per method, no correlation logic), so a passing test here is strong
//! evidence the trait is straightforward to implement for real. `retention.rs`
//! doesn't use any of this — it drives the spawned retention/health-watch
//! tasks directly against its own repository seed.

use std::sync::Arc;

use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_core::models::{CreateItem, CreateProject, ItemType, Priority, ProjectType};
use tack_core::vocabulary;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun, UpsertOrchLink,
};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::reconciler::{ControlPlaneStore, HealthRecord, RegisteredPlane};
use tack_orch::{ControlPlane, OrchError};

pub(crate) async fn setup_repo() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

pub(crate) async fn seed_workspace(repo: &Repository) -> Uuid {
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

pub(crate) async fn seed_project(
    repo: &Repository,
    workspace_id: Uuid,
) -> tack_core::models::Project {
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

pub(crate) async fn seed_item(
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

pub(crate) struct TestRepoStore {
    pub(crate) repo: Repository,
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
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_events(control_plane_id, events)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }
}

pub(crate) const HEALTH_BODY: &str = r#"{"status":"ok","gateway":0}"#;
pub(crate) const STATUS_BODY: &str = r#"{"apiVersion":"2","timestamp":"2026-08-04T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#;
pub(crate) const EMPTY_APPROVALS_BODY: &str = r#"{"pending":[]}"#;

pub(crate) async fn mount_health_and_status(server: &MockServer) {
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

pub(crate) async fn seed_control_plane_and_link(
    repo: &Repository,
    project_id: Uuid,
    base_url: &str,
) -> Uuid {
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
