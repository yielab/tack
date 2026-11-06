//! The facts every screen reads from the store: whether an item is a group or work (`node`), the
//! subtree under an item (`under`), and the work a group holds, at any depth, for its progress.

use tack_core::hierarchy::Node;
use tack_core::models::{CreateItem, Item, ItemFilter, Project};
use tack_db::{Repository, queries};
use tack_test_support::{create_test_workspace, make_project, seed, setup_test_db};
use uuid::Uuid;

async fn make(
    repo: &Repository,
    project: &Project,
    title: &str,
    item_type: &str,
    parent: Option<Uuid>,
    estimate: Option<f64>,
) -> Item {
    let status = project.workflow.initial_status().unwrap().to_string();
    repo.create_item(
        project.id,
        &status,
        CreateItem {
            title: title.into(),
            item_type: Some(item_type.into()),
            parent_id: parent,
            estimate,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

async fn fixture() -> (Repository, Project) {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    (repo, project)
}

async fn node_of(repo: &Repository, item: &Item) -> Node {
    repo.get_item_snapshot(item.id)
        .await
        .unwrap()
        .unwrap()
        .item
        .node
}

async fn titles(repo: &Repository, project: &Project, filter: ItemFilter) -> Vec<String> {
    let mut titles: Vec<String> = repo
        .list_items(project.id, &filter)
        .await
        .unwrap()
        .into_iter()
        .map(|card| card.item.title.clone())
        .collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn a_type_that_holds_work_is_a_group_with_no_children_and_work_with_children_is_a_group() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None, None).await;
    let lone = make(&repo, &project, "Lone", "task", None, None).await;
    let task = make(&repo, &project, "Pay", "task", None, None).await;
    let child = make(&repo, &project, "Receipt", "task", Some(task.id), None).await;

    assert_eq!(node_of(&repo, &epic).await, Node::Group);
    assert_eq!(node_of(&repo, &lone).await, Node::Work);
    assert_eq!(node_of(&repo, &task).await, Node::Group);
    assert_eq!(node_of(&repo, &child).await, Node::Work);

    // The list says the same, and filters by it.
    let listed = repo
        .list_items(project.id, &Default::default())
        .await
        .unwrap();
    let node_by_title = |title: &str| listed.iter().find(|c| c.title == title).unwrap().node;
    assert_eq!(node_by_title("Checkout"), Node::Group);
    assert_eq!(node_by_title("Lone"), Node::Work);
    let work = ItemFilter {
        node: Some(Node::Work),
        ..Default::default()
    };
    assert_eq!(titles(&repo, &project, work).await, ["Lone", "Receipt"]);
    let groups = ItemFilter {
        node: Some(Node::Group),
        ..Default::default()
    };
    assert_eq!(titles(&repo, &project, groups).await, ["Checkout", "Pay"]);
}

#[tokio::test]
async fn a_hidden_child_does_not_make_its_parent_a_group() {
    let (repo, project) = fixture().await;
    let task = make(&repo, &project, "Pay", "task", None, None).await;
    let anchor = make(&repo, &project, "Thread", "task", Some(task.id), None).await;
    seed::item_hidden(&repo, &anchor.id.to_string()).await;
    assert_eq!(node_of(&repo, &task).await, Node::Work);
}

#[tokio::test]
async fn under_returns_the_whole_subtree_and_not_the_item_itself() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None, None).await;
    let task = make(&repo, &project, "Pay", "task", Some(epic.id), None).await;
    let _deep = make(&repo, &project, "Receipt", "task", Some(task.id), None).await;
    let hidden = make(&repo, &project, "Thread", "task", Some(task.id), None).await;
    seed::item_hidden(&repo, &hidden.id.to_string()).await;
    let _elsewhere = make(&repo, &project, "Other", "task", None, None).await;

    let under = |id: Uuid| ItemFilter {
        under: Some(id),
        ..Default::default()
    };
    assert_eq!(
        titles(&repo, &project, under(epic.id)).await,
        ["Pay", "Receipt"]
    );
    assert_eq!(titles(&repo, &project, under(task.id)).await, ["Receipt"]);
    let count = repo.count_items(project.id, &under(epic.id)).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn a_group_counts_the_work_under_it_at_any_depth_and_never_a_group_or_a_hidden_item() {
    let (repo, project) = fixture().await;
    let outer = make(&repo, &project, "Outer", "epic", None, None).await;
    let inner = make(&repo, &project, "Inner", "epic", Some(outer.id), None).await;
    let empty = make(&repo, &project, "Empty", "epic", Some(outer.id), None).await;
    let a = make(&repo, &project, "A", "task", Some(inner.id), Some(3.0)).await;
    let _b = make(&repo, &project, "B", "task", Some(outer.id), Some(5.0)).await;
    // Work that holds work is a group: only its own work counts.
    let parent = make(
        &repo,
        &project,
        "Parent",
        "task",
        Some(inner.id),
        Some(100.0),
    )
    .await;
    let _c = make(&repo, &project, "C", "task", Some(parent.id), None).await;
    let hidden = make(&repo, &project, "Thread", "task", Some(inner.id), Some(7.0)).await;
    seed::item_hidden(&repo, &hidden.id.to_string()).await;

    let counted =
        |tallies: &[tack_core::children::Tally]| tallies.iter().map(|t| t.count).sum::<u32>();
    let all = queries::children::of_project(&repo, project.id)
        .await
        .unwrap();
    assert_eq!(counted(&all[&outer.id]), 3, "A, B and C under Outer");
    assert_eq!(counted(&all[&inner.id]), 2, "A and C under Inner");
    assert_eq!(counted(&all[&parent.id]), 1);
    assert!(!all.contains_key(&a.id), "work holds nothing");
    assert!(
        !all.contains_key(&empty.id),
        "a group with no work under it is left out"
    );
    let estimate: f64 = all[&outer.id].iter().filter_map(|t| t.estimate).sum();
    assert_eq!(
        estimate, 8.0,
        "A and B, not the Parent that holds C, not the hidden item"
    );

    let one = queries::children::of_parent(&repo, inner.id).await.unwrap();
    assert_eq!(counted(&one), 2);
    let some = queries::children::of_groups(&repo, &[inner.id, parent.id])
        .await
        .unwrap();
    assert_eq!(some.len(), 2);
    assert!(
        queries::children::of_groups(&repo, &[])
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn work_that_asks_a_person_is_told_apart() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None, None).await;
    let waiting = make(&repo, &project, "Pay", "task", Some(epic.id), None).await;
    let _quiet = make(&repo, &project, "Receipt", "task", Some(epic.id), None).await;
    sqlx::query(
        "INSERT INTO notifications (id, project_id, kind, level, key, item_id, title, cause, created_at)
         VALUES ('n1', ?, 'decision', 'needs_you', 'decision:1', ?, 'Pay', 'asked', '2026-11-05T10:00:00Z')",
    )
    .bind(project.id.to_string())
    .bind(waiting.id.to_string())
    .execute(repo.pool())
    .await
    .unwrap();

    let work = queries::children::of_parent(&repo, epic.id).await.unwrap();
    assert_eq!(work.iter().map(|t| t.needs_you).sum::<u32>(), 1);
    assert_eq!(work.iter().map(|t| t.count).sum::<u32>(), 2);
}
