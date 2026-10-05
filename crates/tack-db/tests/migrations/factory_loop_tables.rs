//! Migrations 078–081: every table and column the factory loop's readers
//! expect exists on a fresh pool.

use crate::common;

#[tokio::test]
async fn migrations_078_to_081_create_their_tables_and_columns() {
    let repo = common::setup_test_db().await;
    let cases: &[(&str, &[&str])] = &[
        (
            "item_briefs",
            &[
                "item_id",
                "acceptance",
                "constraints",
                "definition_of_done",
                "risk",
                "created_at",
                "updated_at",
            ],
        ),
        ("execution_decisions", &["recommendation", "viewed_at"]),
        (
            "mrp_reviews",
            &[
                "attempt_id",
                "artifact_id",
                "verdict",
                "reason",
                "viewed_at",
                "reviewed_at",
                "reviewed_by",
            ],
        ),
        (
            "pull_requests",
            &[
                "attempt_id",
                "repo",
                "number",
                "url",
                "state",
                "opened_at",
                "merged_at",
                "closed_at",
                "reverted_by_number",
            ],
        ),
    ];
    for (table, expected) in cases {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("table_info({table}): {e}"));
        for column in *expected {
            assert!(
                columns.iter().any(|c| c == column),
                "{table} is missing column {column}; has {columns:?}"
            );
        }
    }
}
