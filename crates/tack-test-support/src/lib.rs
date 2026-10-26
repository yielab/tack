//! Shared test-only building blocks for Tack crates below the API layer: a migrated
//! in-memory pool, seed helpers for a bare workspace/project/item, a controllable clock,
//! and fixture-file loading.
//!
//! Depends on `tack-core` and `tack-db` only, so no crate ends up compiled twice through a
//! dev-dependency cycle — `tack-api/tests/common` needs test-app/server helpers this crate
//! has no business owning, so it stays where it is instead of moving here.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use tack_core::{
    models::{CreateItem, CreateProject, ItemType, Priority, ProjectType},
    vocabulary,
};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

pub mod seed;

mod poll;
pub use poll::{poll_until, poll_until_sync};

/// Create an in-memory SQLite pool with all migrations applied.
pub async fn setup_test_db() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

/// Insert a bare workspace row; returns its ID.
pub async fn create_test_workspace(repo: &Repository) -> Uuid {
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

/// Create a software project in the given workspace; returns the Project.
pub async fn make_project(repo: &Repository, workspace_id: Uuid) -> tack_core::models::Project {
    repo.create_project(
        workspace_id,
        CreateProject {
            name: "Test Project".into(),
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
    .expect("create project")
}

/// Create a minimal task item in the project's initial workflow status.
pub async fn make_item(
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
            recurrence: None,
            recurrence_of: None,
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
            source_artifact_id: None,
            assignee: None,
        },
    )
    .await
    .expect("create item")
}

/// A shared, settable point in time for tests that must control "now" instead of reading
/// the real clock. Fake-clock types — each implementing `tack-protocol`'s `Clock` or
/// `tack-runner`'s own — wrap one of these in a newtype rather than hand-rolling the
/// `Arc<Mutex<_>>` storage again.
#[derive(Clone)]
pub struct ControllableClock(Arc<Mutex<DateTime<Utc>>>);

impl ControllableClock {
    /// Start the clock at `now`.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    /// The current time.
    pub fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock mutex poisoned")
    }

    /// Jump to an exact time.
    pub fn set(&self, now: DateTime<Utc>) {
        *self.0.lock().expect("clock mutex poisoned") = now;
    }

    /// Move the clock forward (or backward, given a negative duration) by `delta`.
    pub fn advance(&self, delta: Duration) {
        let mut guard = self.0.lock().expect("clock mutex poisoned");
        *guard += delta;
    }
}

/// Create a file-backed SQLite store at `path` with all migrations applied, for a test that
/// needs real write contention between connections, which `sqlite::memory:` cannot give.
pub async fn setup_file_test_db(path: &Path) -> Repository {
    let pool = init_pool(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}
