/// What a store call can fail with. `NotFound` and `Conflict` are raised by the
/// store itself, where the rows are read; a caller never guesses them from a
/// `sqlx::Error`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl StoreError {
    /// A row or stored document that does not have the shape this layer wrote.
    pub(crate) fn malformed(message: impl Into<String>) -> Self {
        StoreError::Sql(sqlx::Error::Protocol(message.into()))
    }
}
