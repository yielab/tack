//! Migration 097: the built-in Planner's and Researcher's instructions name no file. A row still
//! holding the text 096 shipped gets the new text; an edited row keeps its edit; a custom profile is
//! left alone.

use tack_db::repo::execution::seed_builtin_profiles;
use tack_db::{Repository, init_pool, migrations};

const AT_096: &str = "096_planner_researcher_write_plan";

/// The text migration 096 (card M) shipped, byte for byte.
const PLANNER_096: &str = "Plan the work. Do not modify project files; the only file you write is `tack-plan.json`, at the workspace root: `{\"v\":\"1\",\"subtasks\":[{\"title\",\"description\",\"acceptance\":[\"…\"],\"depends_on\":[index…],\"estimate\":\"S|M|L\",\"type\":\"…\",\"role\":\"…\",\"team\":\"…\",\"parent\":index}]}` (`type`, `role`, `team` and `parent` are optional; `parent` is the index of an earlier entry this one belongs under). Explain the plan as the result.";
const RESEARCHER_096: &str = "Investigate the question. Do not modify project files; the only file you may write is `tack-plan.json`. Write your findings as the result, with sources. When the findings lead to work, write `tack-plan.json` at the workspace root as the Planner does.";
/// The text before 096: not covered by 097's guard.
const PLANNER_BEFORE_096: &str = "Plan the work. Do not modify project files. Write `tack-plan.json` at the workspace root: `{\"v\":\"1\",\"subtasks\":[{\"title\",\"description\",\"acceptance\":[\"…\"],\"depends_on\":[index…],\"estimate\":\"S|M|L\",\"type\":\"…\",\"role\":\"…\",\"team\":\"…\",\"parent\":index}]}` (`type`, `role`, `team` and `parent` are optional; `parent` is the index of an earlier entry this one belongs under). Explain the plan as the result.";

async fn at_096() -> sqlx::SqlitePool {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, AT_096)
        .await
        .expect("up to 096");
    pool
}

async fn insert(pool: &sqlx::SqlitePool, id: &str, kind: &str, instructions: &str, builtin: i32) {
    sqlx::query(
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, builtin, summary, created_at, updated_at) \
         VALUES (?, ?, ?, '{}', '{}', ?, ?, 's', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(instructions)
    .bind(kind)
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
        .expect("row")
}

#[tokio::test]
async fn rows_still_holding_the_096_text_get_the_text_that_names_no_file() {
    let pool = at_096().await;
    insert(&pool, "planner", "planner", PLANNER_096, 1).await;
    insert(&pool, "researcher", "researcher", RESEARCHER_096, 1).await;
    migrations::run_all(&pool).await.expect("upgrade to 097");

    for id in ["planner", "researcher"] {
        let text = instructions(&pool, id).await;
        assert!(!text.contains("tack-plan.json"), "{id}: {text}");
        assert!(!text.contains("workspace root"), "{id}: {text}");
        assert!(text.contains("\"subtasks\""), "{id}: {text}");
        assert!(text.contains("`depends_on` (the indexes"), "{id}: {text}");
    }

    // What a fresh database seeds is exactly what the migration produces.
    let fresh = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&fresh).await.expect("migrations");
    seed_builtin_profiles(&Repository::new(fresh.clone()), chrono::Utc::now())
        .await
        .expect("seed");
    for kind in ["planner", "researcher"] {
        let seeded: String =
            sqlx::query_scalar("SELECT instructions FROM agent_profiles WHERE kind = ?")
                .bind(kind)
                .fetch_one(&fresh)
                .await
                .expect("seeded");
        assert_eq!(seeded, instructions(&pool, kind).await, "{kind}");
    }
}

#[tokio::test]
async fn an_edited_row_keeps_its_edit_and_the_rest_is_left_alone() {
    let pool = at_096().await;
    insert(&pool, "planner", "planner", "Plan it my way.", 1).await;
    insert(&pool, "researcher", "researcher", RESEARCHER_096, 1).await;
    // A custom profile holding the shipped text is not a built-in.
    insert(&pool, "mine", "planner", PLANNER_096, 0).await;
    migrations::run_all(&pool).await.expect("upgrade to 097");

    assert_eq!(instructions(&pool, "planner").await, "Plan it my way.");
    assert_eq!(instructions(&pool, "mine").await, PLANNER_096);
    assert_ne!(instructions(&pool, "researcher").await, RESEARCHER_096);
}

/// A row still holding the text from before 096 is not covered: 097's guard is the 096 text.
#[tokio::test]
async fn a_row_still_holding_the_text_from_before_096_is_not_covered() {
    let pool = at_096().await;
    insert(&pool, "planner", "planner", PLANNER_BEFORE_096, 1).await;
    migrations::run_all(&pool).await.expect("upgrade to 097");
    assert_eq!(instructions(&pool, "planner").await, PLANNER_BEFORE_096);
}
