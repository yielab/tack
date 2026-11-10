//! Migration 114: a Reviewer's word on a run has a table of its own, and the built-in Reviewer's
//! instructions gain the verdict line unless somebody edited them.

use tack_db::{init_pool, migrations};

const OLD: &str = "Review the task against its acceptance criteria. Do not modify files. Report findings, each with file and line, most severe first.";

async fn insert(pool: &sqlx::SqlitePool, id: &str, builtin: i64, instructions: &str) {
    sqlx::query(
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, builtin, created_at, updated_at) \
         VALUES (?1, ?1, ?2, '{}', '{}', 'reviewer', ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(instructions)
    .bind(builtin)
    .execute(pool)
    .await
    .expect("profile");
}

async fn instructions(pool: &sqlx::SqlitePool, id: &str) -> String {
    sqlx::query_scalar("SELECT instructions FROM agent_profiles WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("instructions")
}

#[tokio::test]
async fn migration_114_adds_the_verdict_line_to_the_untouched_built_in_reviewer_only() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "113_default_expiry")
        .await
        .expect("up to 112");
    insert(&pool, "builtin", 1, OLD).await;
    insert(&pool, "edited", 1, "Review hard.").await;
    insert(&pool, "custom", 0, OLD).await;
    migrations::run_all(&pool).await.expect("114");
    assert!(instructions(&pool, "builtin").await.starts_with(&format!(
        "{OLD} End the result with one line, `Verdict: merge`"
    )));
    assert_eq!(instructions(&pool, "edited").await, "Review hard.");
    assert_eq!(instructions(&pool, "custom").await, OLD);
}

#[tokio::test]
async fn a_reviewer_verdict_is_one_of_the_three_words() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("all");
    let refused = sqlx::query(
        "INSERT INTO reviewer_verdicts (attempt_id, decision, reviewer_attempt_id, recorded_at) \
         VALUES ('a1', 'approve', 'r', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect_err("an unknown word is not stored");
    assert!(refused.to_string().contains("CHECK"), "{refused}");
}
