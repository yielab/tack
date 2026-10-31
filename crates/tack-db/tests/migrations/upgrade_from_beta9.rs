//! A database written by the published `v0.1.0-beta.9` binary is opened by this one: every
//! migration since runs over real data, nothing it held is lost, and the schema ends up the one a
//! new install gets. See `tests/fixtures/README.md` for how the file was made.

use std::path::Path;
use tack_db::{init_pool, migrations};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/tack-0.1.0-beta.9.sqlite"
);

/// The fixture copied into `dir` and opened, not yet migrated.
async fn open_copy(dir: &Path) -> sqlx::SqlitePool {
    let db = dir.join("tack.db");
    std::fs::copy(FIXTURE, &db).expect("copy the fixture");
    init_pool(&format!("sqlite:{}?mode=rwc", db.display()))
        .await
        .expect("open the copy")
}

/// The fixture copy brought to now by the repository's own entry point.
async fn upgraded(dir: &Path) -> sqlx::SqlitePool {
    let pool = open_copy(dir).await;
    migrations::run_all(&pool).await.expect("upgrade to now");
    pool
}

async fn count(pool: &sqlx::SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect(sql)
}

async fn names(pool: &sqlx::SqlitePool, sql: &str) -> Vec<String> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_all(pool)
        .await
        .expect(sql)
}

#[tokio::test]
async fn the_beta9_database_migrates_to_now_and_keeps_what_it_held() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let pool = open_copy(dir.path()).await;
    let beta9_migrations = count(&pool, "SELECT COUNT(*) FROM _migrations").await;
    migrations::run_all(&pool).await.expect("upgrade to now");

    assert!(
        count(&pool, "SELECT COUNT(*) FROM _migrations").await > beta9_migrations,
        "the migrations written after beta.9 ran"
    );

    // What the fixture's creator made through the beta.9 API.
    for (table, expected) in [
        ("workspaces", 1),
        ("projects", 1),
        ("items", 5),
        ("comments", 3),
        ("dependencies", 1),
        ("sprints", 1),
        ("roles", 1),
        ("item_roles", 1),
        ("agent_profiles", 1),
        ("agent_fleets", 1),
        ("agent_runners", 1),
        ("execution_requests", 1),
    ] {
        assert_eq!(
            count(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
            expected,
            "{table}"
        );
    }

    assert_eq!(
        names(&pool, "SELECT name FROM projects").await,
        ["Upgrade fixture"]
    );
    assert_eq!(
        names(&pool, "SELECT title FROM items ORDER BY title").await,
        [
            "Cut the tag",
            "Plan the release",
            "Tag the commit",
            "Verify the install",
            "Write the changelog"
        ]
    );
    assert_eq!(
        names(&pool, "SELECT content FROM comments ORDER BY content").await,
        [
            "Check the checksum too",
            "Draft the entries from the git log",
            "Second pass done"
        ]
    );
    // The child kept its parent, and the item in the sprint kept its sprint and its role.
    assert_eq!(
        names(
            &pool,
            "SELECT p.title FROM items c JOIN items p ON p.id = c.parent_id WHERE c.title = 'Tag the commit'"
        )
        .await,
        ["Plan the release"]
    );
    assert_eq!(
        names(
            &pool,
            "SELECT s.name || '/' || r.name FROM items i JOIN sprints s ON s.id = i.sprint_id \
             JOIN item_roles ir ON ir.item_id = i.id JOIN roles r ON r.id = ir.role_id \
             WHERE i.title = 'Write the changelog'"
        )
        .await,
        ["Sprint 1/Release manager"]
    );
    assert_eq!(
        names(&pool, "SELECT dependency_type FROM dependencies").await,
        ["is_blocked_by"]
    );
    // The run is still queued, on the profile and fleet it named.
    assert_eq!(
        names(&pool, "SELECT state FROM execution_requests").await,
        ["queued"]
    );
    assert_eq!(
        names(
            &pool,
            "SELECT p.name FROM execution_requests e JOIN agent_profiles p ON p.id = e.agent_profile_id"
        )
        .await,
        ["Fixture profile"]
    );
    // The project saved as a template is there beside the built-in ones.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM project_templates WHERE name = 'Release checklist' AND is_builtin = 0"
        )
        .await,
        1
    );
    assert!(
        count(
            &pool,
            "SELECT COUNT(*) FROM project_templates WHERE is_builtin = 1"
        )
        .await
            > 0,
        "the built-in templates are still there"
    );
}

/// What `sqlite_master` holds, by name: the schema as a new install and an upgraded one see it.
async fn schema(pool: &sqlx::SqlitePool) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT type, name, COALESCE(sql, '') FROM sqlite_master \
         WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("read sqlite_master")
}

#[tokio::test]
async fn an_upgraded_database_has_the_schema_of_a_new_one() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let old = upgraded(dir.path()).await;

    let fresh = init_pool("sqlite::memory:").await.expect("fresh pool");
    migrations::run_all(&fresh).await.expect("fresh migrations");

    let (old, fresh) = (schema(&old).await, schema(&fresh).await);
    let differing: Vec<_> = old
        .iter()
        .zip(&fresh)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.1.as_str())
        .collect();
    assert_eq!(
        old.iter().map(|o| &o.1).collect::<Vec<_>>(),
        fresh.iter().map(|o| &o.1).collect::<Vec<_>>(),
        "the same tables, indexes, triggers and views exist"
    );
    assert!(
        differing.is_empty(),
        "these objects are defined differently after an upgrade: {differing:?}"
    );
}
