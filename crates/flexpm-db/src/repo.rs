pub mod projects;
pub mod items;
pub mod sprints;
pub mod roles;
pub mod comments;
pub mod dependencies;

use sqlx::SqlitePool;

/// Central repository providing access to all data operations.
/// Holds a reference to the database pool.
#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
