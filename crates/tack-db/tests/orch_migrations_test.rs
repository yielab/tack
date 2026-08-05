//! Tests for migrations 019–024 (the Agent-Factory Control Center schema: `control_planes`,
//! `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`, `orch_approvals`).
//!
//! Covers the W0-B acceptance bar:
//!   - a fresh database migrates cleanly and ends up with all six new tables;
//!   - an existing database stopped at "018_github_links" upgrades in place when
//!     `run_all` is called again (simulating an installed `tack.db` picking up a new
//!     Tack release);
//!   - foreign-key enforcement (landed in Phase 26.3, see
//!     `test_foreign_key_rejects_orphan_item` in `integration_test.rs`) still holds for
//!     every new table that has an incoming FK. `control_planes` is the root of this
//!     schema's FK graph — it has no FK columns of its own — so there is nothing to
//!     orphan and no test for it here.

mod common;

use common::setup_test_db;
use sqlx::Row;
use tack_db::{init_pool, migrations};
use uuid::Uuid;

const NEW_TABLES: [&str; 6] = [
    "control_planes",
    "orch_links",
    "orch_tasks",
    "orch_runs",
    "orch_events",
    "orch_approvals",
];

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
    sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_optional(pool)
        .await
        .expect("query sqlite_master")
        .is_some()
}

/// Inserts a single `control_planes` row directly (no repository layer exists yet —
/// that lands in Wave 1 / A3) and returns its id, for use as a valid FK target in the
/// orphan tests below.
async fn insert_control_plane(pool: &sqlx::SqlitePool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO control_planes (id, name, kind, base_url) VALUES (?, 'Test Plane', 'docket', 'http://localhost:9999')",
    )
    .bind(id.to_string())
    .execute(pool)
    .await
    .expect("insert control_plane");
    id
}

// ─── Fresh install ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_fresh_db_migrates_all_orch_tables() {
    let repo = setup_test_db().await;

    for table in NEW_TABLES {
        assert!(
            table_exists(repo.pool(), table).await,
            "expected table {table} to exist after a fresh migration run"
        );
    }

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(
        applied.iter().any(|m| m == "024_orch_approvals"),
        "024_orch_approvals must have been applied on a fresh db"
    );
    // Not asserting 024 is the *last* migration applied: card B3 (Wave 2) added
    // 025-027 after this test was written (see orch_metrics_test.rs for their
    // coverage), and a future card will add more after those. This test's job is
    // "the six Wave-1 orch tables exist," which the loop above already checks.
}

// ─── Upgrade-in-place from an existing 18-migration database ──────────────

#[tokio::test]
async fn test_upgrade_from_018_applies_new_orch_migrations_in_place() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Simulate an installed tack.db that has only ever seen migrations 001-018.
    migrations::run_up_to(&pool, "018_github_links")
        .await
        .expect("apply migrations up to 018");

    assert!(
        table_exists(&pool, "github_links").await,
        "018_github_links should have applied"
    );
    for table in NEW_TABLES {
        assert!(
            !table_exists(&pool, table).await,
            "table {table} must not exist before the upgrade runs"
        );
    }

    // Now run the full migration set again, as `tack serve` does on every startup.
    migrations::run_all(&pool).await.expect("upgrade in place");

    for table in NEW_TABLES {
        assert!(
            table_exists(&pool, table).await,
            "table {table} must exist after upgrading an existing db in place"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(&pool)
        .await
        .expect("count migrations");
    // >= 24 rather than == 24: card B3 (Wave 2) added migrations 025-027 after this
    // test was written (see orch_metrics_test.rs), and later cards will add more.
    // The exact count isn't this test's job — "the six Wave-1 orch tables exist
    // after upgrading in place" (asserted above) is.
    assert!(
        count >= 24,
        "at least the 24 Wave-1 migrations should be recorded as applied, got {count}"
    );
}

// ─── FK enforcement on the new tables ──────────────────────────────────────

#[tokio::test]
async fn test_orch_links_rejects_orphan_project() {
    let repo = setup_test_db().await;
    let plane_id = insert_control_plane(repo.pool()).await;
    let bogus_project = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_links (project_id, control_plane_id, remote_project) VALUES (?, ?, 'demo')",
    )
    .bind(bogus_project.to_string())
    .bind(plane_id.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_link with a dangling project_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_tasks_rejects_orphan_item() {
    let repo = setup_test_db().await;
    let bogus_item = Uuid::new_v4();

    let result =
        sqlx::query("INSERT INTO orch_tasks (item_id, remote_task_id) VALUES (?, 'remote-task-1')")
            .bind(bogus_item.to_string())
            .execute(repo.pool())
            .await;

    assert!(
        result.is_err(),
        "inserting an orch_task with a dangling item_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_runs_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_runs (run_id, control_plane_id, remote_project) VALUES (?, ?, 'demo')",
    )
    .bind(run_id.to_string())
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_run with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_events_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();
    let event_id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_events (id, control_plane_id, event_type) VALUES (?, ?, 'tool_call')",
    )
    .bind(event_id.to_string())
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_event with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_approvals_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_approvals (token, control_plane_id) VALUES ('approval-token-1', ?)",
    )
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_approval with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

// ─── Multi-attempt redispatch: composite PK on orch_tasks ─────────────────

#[tokio::test]
async fn test_orch_tasks_composite_pk_allows_redispatch_same_item() {
    let repo = setup_test_db().await;
    let ws_id = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    // Same item, two different remote task ids (e.g. a retry) — must both succeed
    // because the PK is (item_id, remote_task_id), not item_id alone.
    sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-a', 1)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await
    .expect("first dispatch");

    sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-b', 2)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await
    .expect("redispatch with a new remote_task_id must succeed");

    // But the same (item_id, remote_task_id) pair twice must collide on the PK.
    let dup = sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-a', 3)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await;
    assert!(
        dup.is_err(),
        "duplicate (item_id, remote_task_id) must be rejected by the composite primary key"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_tasks WHERE item_id = ?")
        .bind(item.id.to_string())
        .fetch_one(repo.pool())
        .await
        .expect("count tasks");
    assert_eq!(
        count, 2,
        "two distinct dispatches for the same item should both persist"
    );
}
