//! Migrations 064-073 drop the legacy Docket bridge's tables. No export
//! step: `run_all`'s automatic pre-upgrade `VACUUM INTO` snapshot
//! (`create_pre_upgrade_backup_if_needed`) already fires before the first of
//! them, so the rows a real install has accumulated survive there.

use sqlx::ConnectOptions;
use tack_db::{init_pool, migrations};
use uuid::Uuid;

use crate::common::{create_test_workspace, make_item, make_project};

#[tokio::test]
async fn the_bridge_tables_are_dropped_and_their_rows_kept_in_the_snapshot() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("bridge-drop.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = init_pool(&db_url).await.expect("file-backed pool");
    migrations::run_up_to(&pool, "063_drop_model_profiles")
        .await
        .expect("apply pre-drop schema");

    let repo = tack_db::Repository::new(pool.clone());
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

    let plane_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO control_planes (id, name, kind, base_url) \
         VALUES (?, 'Test Plane', 'docket', 'http://127.0.0.1:9999')",
    )
    .bind(plane_id.to_string())
    .execute(&pool)
    .await
    .expect("insert control_planes row");
    sqlx::query(
        "INSERT INTO orch_metrics (id, control_plane_id, name, value) \
         VALUES (?, ?, 'agent_cost_usd', 1.5)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(plane_id.to_string())
    .execute(&pool)
    .await
    .expect("insert orch_metrics row, a control_planes child");

    migrations::run_all(&pool)
        .await
        .expect("upgrade past the bridge-table drop migrations");

    let remaining_bridge_tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND (name = 'control_planes' OR name LIKE 'orch\\_%' ESCAPE '\\')",
    )
    .fetch_all(&pool)
    .await
    .expect("query sqlite_master");
    assert!(
        remaining_bridge_tables.is_empty(),
        "bridge tables should all be dropped, found: {remaining_bridge_tables:?}"
    );

    let snapshot_path = format!("{}.before-064_drop_orch_links.sqlite", db_path.display());
    assert!(
        std::path::Path::new(&snapshot_path).is_file(),
        "the first bridge-table drop must create a durable pre-upgrade SQLite snapshot"
    );
    let mut snapshot_conn = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&snapshot_path)
        .create_if_missing(false)
        .connect()
        .await
        .expect("open pre-upgrade snapshot");
    let snapshot_planes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM control_planes WHERE id = ?")
            .bind(plane_id.to_string())
            .fetch_one(&mut snapshot_conn)
            .await
            .expect("query snapshot control_planes");
    let snapshot_metrics: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics WHERE control_plane_id = ?")
            .bind(plane_id.to_string())
            .fetch_one(&mut snapshot_conn)
            .await
            .expect("query snapshot orch_metrics");
    assert_eq!(
        snapshot_planes, 1,
        "control_planes row must survive in the snapshot"
    );
    assert_eq!(
        snapshot_metrics, 1,
        "orch_metrics row must survive in the snapshot"
    );

    let board_item: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE id = ?")
        .bind(item.id.to_string())
        .fetch_one(&pool)
        .await
        .expect("query items");
    assert_eq!(
        board_item, 1,
        "the upgrade must not touch the board's own tables"
    );
}
