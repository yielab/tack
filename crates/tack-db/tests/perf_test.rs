mod common;

use common::{create_test_workspace, setup_test_db};
use std::time::{Duration, Instant};
use tack_core::models::{CreateProject, ItemFilter, ProjectType};

// Run with: cargo test -p tack-db -- --ignored
// Skipped in normal CI to avoid ~5s wall time on every push.

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

    // Bulk-insert 50 000 items inside a single transaction for speed.
    let mut tx = repo.pool().begin().await.expect("begin tx");
    for i in 0..50_000_u32 {
        sqlx::query(
            "INSERT INTO items
             (id, project_id, title, item_type, status, priority, estimate_unit, tags, sort_order, created_at, updated_at)
             VALUES
             (lower(hex(randomblob(16))), ?, ?, 'task', ?, 'medium', '\"story_points\"', '[]', ?, datetime('now'), datetime('now'))"
        )
        .bind(project.id.to_string())
        .bind(format!("Item {i}"))
        .bind(&initial_status)
        .bind(i as i32)
        .execute(&mut *tx)
        .await
        .expect("insert item");
    }
    tx.commit().await.expect("commit");

    // Warm up the query plan.
    let filter = ItemFilter {
        per_page: Some(100),
        ..Default::default()
    };
    let _ = repo.list_items(project.id, &filter).await.expect("warmup");

    // Measure 100 back-to-back calls; assert p95 < 100 ms.
    const RUNS: usize = 100;
    let mut latencies: Vec<Duration> = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let t = Instant::now();
        let _ = repo.list_items(project.id, &filter).await.expect("list");
        latencies.push(t.elapsed());
    }

    latencies.sort_unstable();
    let p50 = latencies[49];
    let p95 = latencies[94];
    let p99 = latencies[98];
    println!("list_items @ 50k items — p50={p50:?}  p95={p95:?}  p99={p99:?}");

    assert!(
        p95 < Duration::from_millis(100),
        "p95 latency {p95:?} exceeds 100 ms target"
    );
}
