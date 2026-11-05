//! A change set is applied in one transaction: a failure in the third operation leaves nothing,
//! not the two writes before it, and the change set is as it was.

use std::collections::HashMap;

use chrono::Utc;
use tack_core::change_set::{
    ChangeSetStatus, CreateOp, ItemRef, OpState, Operation, Origin, Resolution,
};
use tack_core::models::Priority;
use tack_db::repo::change_sets::{ApplyError, ApplyPlan, NewChangeSet};
use tack_test_support::{create_test_workspace, make_item, make_project, setup_test_db};

fn create(title: &str) -> Operation {
    Operation::Create(CreateOp {
        title: title.into(),
        description: None,
        acceptance: vec!["done".into()],
        depends_on: Vec::new(),
        item_type: None,
        role: None,
        team: None,
        profile_id: None,
        parent: None,
    })
}

#[tokio::test]
async fn a_failure_in_the_third_operation_leaves_nothing() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let item = make_item(&repo, &project).await;
    let now = Utc::now();
    let (set, made) = repo
        .insert_change_set(&NewChangeSet {
            project: project.id,
            principal: "agent:x",
            origin: Origin::Mcp,
            origin_ref: None,
            reason: "test",
            idempotency_key: None,
            anchor: None,
            artifact_id: None,
            operations: vec![
                create("made"),
                Operation::Priority {
                    item: ItemRef::Existing(item.id),
                    priority: Priority::Critical,
                },
                // The status does not exist: the check that precedes an application would have
                // refused it, so this stands for what changed after the check.
                Operation::Move {
                    item: ItemRef::Existing(item.id),
                    status: "Nowhere".into(),
                },
            ],
            now,
        })
        .await
        .unwrap();
    assert!(made);
    let failed = repo
        .apply_change_set(
            set.id,
            &ApplyPlan {
                workflow: &project.workflow,
                parent_gate: &tack_core::hierarchy::Hierarchy,
                selected: &[0, 1, 2],
                edits: &[],
                resolutions: &HashMap::from([(0, Resolution::default())]),
                writer: "agent:x",
                author: None,
                decided_by: "operator:local",
                now,
                auto_creates: 0,
            },
        )
        .await;
    assert!(matches!(failed, Err(ApplyError::Refused(_))), "{failed:?}");

    let items = repo.get_item_tree(project.id).await.unwrap();
    assert_eq!(items.len(), 1, "the create was rolled back");
    assert_eq!(
        repo.get_item(item.id).await.unwrap().unwrap().priority,
        item.priority,
        "the priority change was rolled back"
    );
    let kept = repo.get_change_set(set.id).await.unwrap().unwrap();
    assert_eq!(kept.status, ChangeSetStatus::Waiting);
    assert!(kept.ops.iter().all(|op| op.state == OpState::Waiting));
    assert!(kept.decided_at.is_none());
}

#[tokio::test]
async fn the_same_key_on_a_project_is_the_same_change_set() {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    let new = |reason: &'static str| NewChangeSet {
        project: project.id,
        principal: "agent:x",
        origin: Origin::Mcp,
        origin_ref: None,
        reason,
        idempotency_key: Some("once"),
        anchor: None,
        artifact_id: None,
        operations: vec![create("a")],
        now: Utc::now(),
    };
    let (first, made_first) = repo.insert_change_set(&new("first")).await.unwrap();
    let (second, made_second) = repo.insert_change_set(&new("second")).await.unwrap();
    assert!(made_first && !made_second);
    assert_eq!(second.id, first.id);
    assert_eq!(second.reason, "first");
}
