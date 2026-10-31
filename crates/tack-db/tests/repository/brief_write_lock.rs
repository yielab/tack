//! Saving a brief reads the stored one and then writes. Another writer that commits in between must make it wait, not fail:
//! a deferred transaction lost the upgrade at once with `database is locked`, which a person saw as "Internal server error"
//! on the checklist they had just typed (found by the live suite, card G4).

use std::time::Duration;
use tack_core::models::UpsertItemBrief;
use tack_test_support::{create_test_workspace, make_item, make_project, setup_file_test_db};

#[tokio::test]
async fn a_brief_saved_while_another_writer_commits_waits_for_it_and_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    let repo = setup_file_test_db(&dir.path().join("brief-lock.db")).await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    // Another writer holds the write lock and has changed the item.
    let mut other = repo.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE items SET title = 'held' WHERE id = ?")
        .bind(item.id.to_string())
        .execute(&mut *other)
        .await
        .unwrap();
    let saving = {
        let repo = repo.clone();
        tokio::spawn(async move {
            repo.upsert_item_brief(
                item.id,
                UpsertItemBrief {
                    acceptance: vec![],
                    constraints: vec![],
                    definition_of_done: Some("done".into()),
                    risk: None,
                },
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    other.commit().await.unwrap();

    let stored = saving
        .await
        .unwrap()
        .expect("the save waited for the other writer and did not fail");
    assert_eq!(stored.definition_of_done.as_deref(), Some("done"));
}
