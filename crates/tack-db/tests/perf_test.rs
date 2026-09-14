mod common;

use common::{create_test_workspace, setup_test_db};
use std::time::{Duration, Instant};
use tack_core::models::{CreateProject, ItemFilter, ProjectType};
use uuid::Uuid;

// Run with: cargo nextest run --workspace --run-ignored ignored-only -E 'test(list_items_p95)'
// Skipped in normal CI to avoid ~5s wall time on every push.

/// Bulk-inserts `count` items inside a single transaction for speed.
async fn seed_items(repo: &tack_db::Repository, project_id: Uuid, status: &str, count: u32) {
    let mut tx = repo.pool().begin().await.expect("begin tx");
    for i in 0..count {
        sqlx::query(
            "INSERT INTO items
             (id, project_id, title, item_type, status, priority, estimate_unit, tags, sort_order, created_at, updated_at)
             VALUES
             (lower(hex(randomblob(16))), ?, ?, 'task', ?, 'medium', '\"story_points\"', '[]', ?, datetime('now'), datetime('now'))"
        )
        .bind(project_id.to_string())
        .bind(format!("Item {i}"))
        .bind(status)
        .bind(i as i32)
        .execute(&mut *tx)
        .await
        .expect("insert item");
    }
    tx.commit().await.expect("commit");
}

/// Runs `list_items` `runs` times back-to-back and returns sorted latencies.
async fn measure_latencies(
    repo: &tack_db::Repository,
    project_id: Uuid,
    filter: &ItemFilter,
    runs: usize,
) -> Vec<Duration> {
    let mut latencies = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let _ = repo.list_items(project_id, filter).await.expect("list");
        latencies.push(t.elapsed());
    }
    latencies.sort_unstable();
    latencies
}

#[tokio::test]
#[ignore]
async fn list_items_p95_under_100ms_at_50k() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Perf Project".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("create project");
    let initial_status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();

    seed_items(&repo, project.id, &initial_status, 50_000).await;

    // Warm up the query plan, then measure 100 back-to-back calls.
    let filter = ItemFilter {
        per_page: Some(100),
        ..Default::default()
    };
    let _ = repo.list_items(project.id, &filter).await.expect("warmup");
    let latencies = measure_latencies(&repo, project.id, &filter, 100).await;
    let (p50, p95, p99) = (latencies[49], latencies[94], latencies[98]);
    println!("list_items @ 50k items — p50={p50:?}  p95={p95:?}  p99={p99:?}");

    assert!(
        p95 < Duration::from_millis(100),
        "p95 latency {p95:?} exceeds 100 ms target"
    );
}
