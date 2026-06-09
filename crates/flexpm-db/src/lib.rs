pub mod migrations;
pub mod repo;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tracing::info;

pub use repo::Repository;

/// Initialize the database connection pool with WAL mode.
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    info!(database_url, "Initializing database pool");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Enable WAL mode for better concurrent read performance
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;

    // Enable foreign keys
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;

    info!("Database pool initialized with WAL mode");
    Ok(pool)
}
