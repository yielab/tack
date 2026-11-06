use crate::common;

use common::{create_test_workspace, setup_test_db};
use std::time::{Duration, Instant};
use tack_core::children;
use tack_core::hierarchy::Node;
use tack_core::models::{CreateProject, ItemFilter, ProjectType};
use tack_core::workflow::WorkflowConfig;
use tack_db::queries;
use uuid::Uuid;

// Run with: cargo nextest run --workspace --run-ignored ignored-only -E 'test(list_items_p95)'
// Skipped in normal CI to avoid ~5s wall time on every push.

/// Groups per level of the seeded tree: 10 at the top, 9 under each of those, 10 under each of
/// those; every group then holds [`WORK_PER_GROUP`] work items.
const FANOUT: [usize; 3] = [10, 9, 10];
const WORK_PER_GROUP: u32 = 49;

/// One row to insert: a group (`epic`) or a work item (`task`), under `parent`.
struct Row {
    item_type: &'static str,
    parent: Option<Uuid>,
}

/// The rows of a tree of 1 000 groups (three levels deep) holding 49 work items each, 50 000 items
/// in all, in the order a person adds them: a group, the work in it, then the groups it holds.
fn tree_rows(parent: Option<Uuid>, level: usize, rows: &mut Vec<(Uuid, Row)>) {
    for _ in 0..FANOUT[level] {
        let group = Uuid::new_v4();
        rows.push((
            group,
            Row {
                item_type: "epic",
                parent,
            },
        ));
        for _ in 0..WORK_PER_GROUP {
            rows.push((
                Uuid::new_v4(),
                Row {
                    item_type: "task",
                    parent: Some(group),
                },
            ));
        }
        if level + 1 < FANOUT.len() {
            tree_rows(Some(group), level + 1, rows);
        }
    }
}

/// Bulk-inserts [`tree_rows`] inside a single transaction for speed.
async fn seed_tree(repo: &tack_db::Repository, project_id: Uuid, status: &str) {
    let mut rows = Vec::with_capacity(50_000);
    tree_rows(None, 0, &mut rows);
    assert_eq!(rows.len(), 50_000);
    let mut tx = repo.pool().begin().await.expect("begin tx");
    for (sort, (id, row)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO items
             (id, project_id, parent_id, title, item_type, status, priority, estimate_unit, tags, sort_order, created_at, updated_at)
             VALUES
             (?, ?, ?, ?, ?, ?, 'medium', '\"story_points\"', '[]', ?, datetime('now'), datetime('now'))",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(row.parent.map(|p| p.to_string()))
        .bind(format!("Item {sort}"))
        .bind(row.item_type)
        .bind(status)
        .bind(sort as i32)
        .execute(&mut *tx)
        .await
        .expect("insert item");
    }
    tx.commit().await.expect("commit");
}

/// Runs what the list route does for a page, `runs` times back-to-back, and returns sorted
/// latencies: the page of cards (with `node` filled), and the progress of each group on it.
async fn measure_latencies(
    repo: &tack_db::Repository,
    project_id: Uuid,
    workflow: &WorkflowConfig,
    filter: &ItemFilter,
    runs: usize,
) -> Vec<Duration> {
    let mut latencies = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        let page = repo.list_items(project_id, filter).await.expect("list");
        let groups: Vec<Uuid> = page
            .iter()
            .filter(|card| card.node == Node::Group)
            .map(|card| card.item.id)
            .collect();
        let work = queries::children::of_groups(repo, &groups)
            .await
            .expect("work under the groups");
        let progress = work
            .values()
            .filter_map(|work| children::progress(work, workflow))
            .count();
        assert_eq!(progress, groups.len(), "every group on the page holds work");
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
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .expect("create project");
    let initial_status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();

    seed_tree(&repo, project.id, &initial_status).await;

    // Warm up the query plan, then measure 100 back-to-back calls.
    let filter = ItemFilter {
        per_page: Some(100),
        ..Default::default()
    };
    let _ = measure_latencies(&repo, project.id, &project.workflow, &filter, 1).await;
    let latencies = measure_latencies(&repo, project.id, &project.workflow, &filter, 100).await;
    let (p50, p95, p99) = (latencies[49], latencies[94], latencies[98]);
    println!(
        "list_items with node and progress @ 50k items — p50={p50:?}  p95={p95:?}  p99={p99:?}"
    );

    assert!(
        p95 < Duration::from_millis(100),
        "p95 latency {p95:?} exceeds 100 ms target"
    );
}
