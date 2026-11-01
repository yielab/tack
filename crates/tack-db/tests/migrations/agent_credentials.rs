//! Migration 099: agent credentials, and the columns that record who wrote an item, a comment
//! and a dependency. Rows that exist keep NULL; a name is unique only among credentials not revoked.

use chrono::{Duration, Utc};
use tack_core::principal::{Capability, Scope};
use tack_db::{Repository, init_pool, migrations};

#[tokio::test]
async fn migration_099_keeps_existing_rows_and_leaves_the_writer_unrecorded() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "098_project_on_start_status")
        .await
        .expect("up to 098");
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES ('w', 'W', '{}')")
        .execute(&pool)
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, project_type, created_at, updated_at) \
         VALUES ('p', 'w', 'P', 'software', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("project at 098");
    sqlx::query(
        "INSERT INTO items (id, project_id, title, item_type, status, created_at, updated_at) \
         VALUES ('i', 'p', 'T', 'task', 'To Do', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("item at 098");
    sqlx::query("INSERT INTO comments (id, item_id, content) VALUES ('c', 'i', 'old')")
        .execute(&pool)
        .await
        .expect("comment at 098");

    migrations::run_all(&pool).await.expect("upgrade to 099");

    let (created, updated): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT created_by, updated_by FROM items WHERE id = 'i'")
            .fetch_one(&pool)
            .await
            .expect("item");
    assert_eq!((created, updated), (None, None));
    let principal: Option<String> =
        sqlx::query_scalar("SELECT principal FROM comments WHERE id = 'c'")
            .fetch_one(&pool)
            .await
            .expect("comment");
    assert_eq!(principal, None);
    let credentials: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_credentials")
        .fetch_one(&pool)
        .await
        .expect("table");
    assert_eq!(credentials, 0);
}

#[tokio::test]
async fn a_name_is_unique_among_credentials_not_revoked() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let now = Utc::now();
    let scopes = [Scope {
        all_projects: true,
        projects: vec![],
        capabilities: vec![Capability::Read],
    }];
    let later = now + Duration::days(90);

    let first = repo
        .create_agent_credential("Bot", "hash-1", &scopes, now, later)
        .await
        .expect("first");
    assert!(
        repo.create_agent_credential("Bot", "hash-2", &scopes, now, later)
            .await
            .is_err(),
        "a second credential with the same name"
    );
    repo.revoke_agent_credential(first.id, now)
        .await
        .expect("revoke");
    repo.create_agent_credential("Bot", "hash-3", &scopes, now, later)
        .await
        .expect("a revoked name is free");
}
