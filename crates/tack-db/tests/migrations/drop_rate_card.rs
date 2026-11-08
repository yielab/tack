//! Migration 112: the rate card an operator entered is deleted; other `app_meta` keys stay.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_112_deletes_the_stored_rate_card_and_nothing_else() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "111_profile_egress_mode")
        .await
        .expect("up to 111");
    for key in ["rate_card", "other"] {
        sqlx::query("INSERT INTO app_meta (key, value) VALUES (?, '{}')")
            .bind(key)
            .execute(&pool)
            .await
            .expect("meta");
    }
    migrations::run_all(&pool).await.expect("112");
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM app_meta ORDER BY key")
        .fetch_all(&pool)
        .await
        .expect("keys");
    assert!(!keys.contains(&"rate_card".to_owned()), "{keys:?}");
    assert!(keys.contains(&"other".to_owned()), "{keys:?}");
}
