pub mod attachments;
pub mod boards;
pub mod comments;
pub mod custom_fields;
pub mod dependencies;
pub mod items;
pub mod projects;
pub mod roles;
pub mod sprints;
pub mod templates;

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
