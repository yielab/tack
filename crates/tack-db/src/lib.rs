pub mod migrations;
pub mod repo;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tracing::info;

pub use repo::Repository;

/// Initialize the database connection pool with WAL mode.
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    info!(database_url, "Initializing database pool");

    // `foreign_keys` is a *per-connection* SQLite setting — running a one-off
    // `PRAGMA foreign_keys=ON` only arms the first connection the pool hands out,
    // leaving the other pooled connections free to insert orphan rows. Setting it
    // on the connect options makes every connection the pool opens enforce FKs.
    let options = SqliteConnectOptions::from_str(database_url)?.foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // WAL is a persistent, database-level setting stored in the file header, so
    // applying it once via the pool is sufficient for all connections.
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;

    info!("Database pool initialized with WAL mode");
    Ok(pool)
}
