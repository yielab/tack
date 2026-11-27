//! Adding a profile reads the keys taken and then writes. Another writer that commits in between must make it wait,
//! not fail: a deferred transaction lost the upgrade at once with `database is locked`, which a person saw as
//! "Could not create agent profile" when two profiles were added at once (found by the frontend e2e suite).

use std::time::Duration;
use tack_db::repo::execution::NewAgentProfile;
use tack_test_support::{create_test_workspace, make_item, make_project, setup_file_test_db};

#[tokio::test]
async fn a_profile_added_while_another_writer_commits_waits_for_it_and_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    let repo = setup_file_test_db(&dir.path().join("profile-lock.db")).await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    // Another writer holds the write lock and has changed a row.
    let mut other = repo.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE items SET title = 'held' WHERE id = ?")
        .bind(item.id.to_string())
        .execute(&mut *other)
        .await
        .unwrap();
    let adding = {
        let repo = repo.clone();
        tokio::spawn(async move {
            repo.create_agent_profile_placed(
                NewAgentProfile {
                    id: "profile-held",
                    key: None,
                    name: "Held profile",
                    instructions: "test",
                    tool_policy: "{}",
                    limits: "{}",
                },
                None,
                None,
                None,
                None,
                chrono::Utc::now(),
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    other.commit().await.unwrap();

    adding
        .await
        .unwrap()
        .expect("the profile waited for the other writer and did not fail");
    let key: String =
        sqlx::query_scalar("SELECT key FROM agent_profiles WHERE id = 'profile-held'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(key, "held-profile");
}
