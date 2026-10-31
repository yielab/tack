//! Migration 096: the built-in Planner and Researcher get Claude Code's `Write` (and the sentence that
//! says the plan file is the only file they write) when their rows still hold what shipped; an edited
//! row keeps its edit.

use tack_db::repo::execution::seed_builtin_profiles;
use tack_db::{Repository, init_pool, migrations};

const OLD_PLANNER: &str = "Plan the work. Do not modify project files. Write `tack-plan.json` at the workspace root: `{\"v\":\"1\",\"subtasks\":[{\"title\",\"description\",\"acceptance\":[\"…\"],\"depends_on\":[index…],\"estimate\":\"S|M|L\",\"type\":\"…\",\"role\":\"…\",\"team\":\"…\",\"parent\":index}]}` (`type`, `role`, `team` and `parent` are optional; `parent` is the index of an earlier entry this one belongs under). Explain the plan as the result.";
const OLD_RESEARCHER: &str = "Investigate the question. Do not modify project files. Write your findings as the result, with sources. When the findings lead to work, write `tack-plan.json` at the workspace root as the Planner does.";

async fn insert(
    pool: &sqlx::SqlitePool,
    id: &str,
    kind: &str,
    instructions: &str,
    claude_code: &str,
) {
    let policy = format!(
        r#"{{"tools":{{"claude-code":{claude_code},"codex":["*"],"docket":["*"],"opencode":["*"]}}}}"#
    );
    sqlx::query(
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, builtin, summary, created_at, updated_at) \
         VALUES (?, ?, ?, ?, '{}', ?, 1, 's', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(instructions)
    .bind(policy)
    .bind(kind)
    .execute(pool)
    .await
    .expect("profile");
}

async fn row(pool: &sqlx::SqlitePool, id: &str) -> (String, serde_json::Value) {
    let (instructions, policy): (String, String) =
        sqlx::query_as("SELECT instructions, tool_policy FROM agent_profiles WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("row");
    (instructions, serde_json::from_str(&policy).expect("policy"))
}

fn claude_code(policy: &serde_json::Value) -> Vec<&str> {
    policy["tools"]["claude-code"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|tool| tool.as_str())
        .collect()
}

async fn at_095() -> sqlx::SqlitePool {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "095_profile_delivers")
        .await
        .expect("up to 095");
    pool
}

#[tokio::test]
async fn rows_that_still_hold_what_shipped_get_write_and_the_new_instructions() {
    let pool = at_095().await;
    insert(
        &pool,
        "planner",
        "planner",
        OLD_PLANNER,
        r#"["Read","Grep","Glob","WebFetch"]"#,
    )
    .await;
    insert(
        &pool,
        "researcher",
        "researcher",
        OLD_RESEARCHER,
        r#"["Read","Grep","Glob","WebFetch","WebSearch"]"#,
    )
    .await;
    migrations::run_up_to(&pool, "096_planner_researcher_write_plan")
        .await
        .expect("upgrade to 096");

    let (instructions, policy) = row(&pool, "planner").await;
    assert_eq!(
        claude_code(&policy),
        ["Read", "Grep", "Glob", "WebFetch", "Write"]
    );
    assert!(
        instructions.contains("the only file you write is `tack-plan.json`"),
        "{instructions}"
    );
    // The other agents' lists stay as they were.
    assert_eq!(policy["tools"]["codex"], serde_json::json!(["*"]));
    assert_eq!(policy["tools"]["opencode"], serde_json::json!(["*"]));
    assert_eq!(policy["tools"]["docket"], serde_json::json!(["*"]));

    let (instructions, policy) = row(&pool, "researcher").await;
    assert_eq!(
        claude_code(&policy),
        ["Read", "Grep", "Glob", "WebFetch", "WebSearch", "Write"]
    );
    assert!(
        instructions.contains("the only file you may write is `tack-plan.json`"),
        "{instructions}"
    );

    // What a fresh database seeds has the migration's tool lists (its instructions moved on in 097,
    // see `plan_instructions_name_no_file.rs`).
    let fresh = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&fresh).await.expect("migrations");
    let repo = Repository::new(fresh.clone());
    seed_builtin_profiles(&repo, chrono::Utc::now())
        .await
        .expect("seed");
    for kind in ["planner", "researcher"] {
        let seeded: (String, String) =
            sqlx::query_as("SELECT instructions, tool_policy FROM agent_profiles WHERE kind = ?")
                .bind(kind)
                .fetch_one(&fresh)
                .await
                .expect("seeded");
        let seeded_policy: serde_json::Value = serde_json::from_str(&seeded.1).expect("policy");
        // Codex and opencode get every tool, which selects their writable sandbox and edit tool.
        assert_eq!(
            seeded_policy["tools"]["codex"],
            serde_json::json!(["*"]),
            "{kind}"
        );
        assert_eq!(
            seeded_policy["tools"]["opencode"],
            serde_json::json!(["*"]),
            "{kind}"
        );
        let migrated = row(&pool, kind).await;
        let seeded: serde_json::Value = serde_json::from_str(&seeded.1).expect("policy");
        assert_eq!(seeded, migrated.1, "{kind} tool policy");
    }
}

#[tokio::test]
async fn a_row_someone_edited_keeps_its_edit() {
    let pool = at_095().await;
    // A Planner whose list gained Bash, with the shipped instructions.
    insert(
        &pool,
        "planner",
        "planner",
        OLD_PLANNER,
        r#"["Read","Grep","Glob","WebFetch","Bash"]"#,
    )
    .await;
    // A Researcher with the shipped list and reworded instructions.
    insert(
        &pool,
        "researcher",
        "researcher",
        "Investigate it my way.",
        r#"["Read","Grep","Glob","WebFetch","WebSearch"]"#,
    )
    .await;
    migrations::run_up_to(&pool, "096_planner_researcher_write_plan")
        .await
        .expect("upgrade to 096");

    let (instructions, policy) = row(&pool, "planner").await;
    assert_eq!(
        claude_code(&policy),
        ["Read", "Grep", "Glob", "WebFetch", "Bash"]
    );
    assert!(
        instructions.contains("the only file you write is `tack-plan.json`"),
        "instructions are unedited, so they move"
    );

    let (instructions, policy) = row(&pool, "researcher").await;
    assert_eq!(instructions, "Investigate it my way.");
    assert_eq!(
        claude_code(&policy),
        ["Read", "Grep", "Glob", "WebFetch", "WebSearch", "Write"],
        "the list is unedited, so it moves"
    );
}

#[tokio::test]
async fn a_custom_profile_of_the_same_shape_is_left_alone() {
    let pool = at_095().await;
    sqlx::query(
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, builtin, created_at, updated_at) \
         VALUES ('mine', 'Mine', ?, ?, '{}', 'planner', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(OLD_PLANNER)
    .bind(r#"{"tools":{"claude-code":["Read","Grep","Glob","WebFetch"]}}"#)
    .execute(&pool)
    .await
    .expect("custom");
    migrations::run_up_to(&pool, "096_planner_researcher_write_plan")
        .await
        .expect("upgrade to 096");
    let (instructions, policy) = row(&pool, "mine").await;
    assert_eq!(instructions, OLD_PLANNER);
    assert_eq!(claude_code(&policy), ["Read", "Grep", "Glob", "WebFetch"]);
}
