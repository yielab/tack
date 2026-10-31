//! Migration 094: a project's budgets row gets the default provisional limit; a project with no row has it too.

use tack_db::queries::budgets::provisional_tokens;
use tack_db::repo::budgets::{DEFAULT_PROVISIONAL_TOKENS, ProjectBudgets};
use tack_db::{Repository, init_pool, migrations};

#[tokio::test]
async fn migration_094_gives_an_existing_budgets_row_the_default_provisional_limit() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "092_drop_usd_per_day")
        .await
        .expect("up to 092");
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
    .expect("project");
    sqlx::query("INSERT INTO project_budgets (project_id, tokens_per_day) VALUES ('p', 5)")
        .execute(&pool)
        .await
        .expect("budgets at 092");
    migrations::run_all(&pool).await.expect("upgrade to 094");
    let repo = Repository::new(pool);

    assert_eq!(
        provisional_tokens(&repo, "p").await.unwrap(),
        Some(DEFAULT_PROVISIONAL_TOKENS)
    );
    // A project with no row has the default; one that turned it off, or set 0, has none.
    assert_eq!(
        provisional_tokens(&repo, "nobody").await.unwrap(),
        Some(DEFAULT_PROVISIONAL_TOKENS)
    );
    for off in [None, Some(0)] {
        repo.put_project_budgets(
            "p",
            &ProjectBudgets {
                provisional_tokens_per_run: off,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(provisional_tokens(&repo, "p").await.unwrap(), None);
    }
}
