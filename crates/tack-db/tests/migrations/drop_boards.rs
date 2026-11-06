//! Migration 108: the saved boards and the boards a template carried are gone, and what sat
//! beside them is untouched.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_108_drops_the_boards_table_and_the_templates_board_column() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "107_item_types")
        .await
        .expect("up to 107");
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
        .fetch_all(&pool)
        .await
        .expect("tables");
    assert!(tables.contains(&"boards".to_owned()));

    migrations::run_all(&pool).await.expect("upgrade to 108");

    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
        .fetch_all(&pool)
        .await
        .expect("tables");
    assert!(!tables.contains(&"boards".to_owned()));
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('project_templates')")
            .fetch_all(&pool)
            .await
            .expect("columns");
    assert!(!columns.contains(&"default_boards".to_owned()));
    assert!(columns.contains(&"item_types".to_owned()));
}
