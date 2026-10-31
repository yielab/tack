use std::error::Error;

use tack_protocol::execution::StableErrorCode;

/// What a store call can fail with. `NotFound` and `Conflict` are raised by the
/// store itself, where the rows are read; a caller never guesses them from a
/// `sqlx::Error`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No row of this kind has the id (or the pair of ids) the call named.
    /// `entity` is the singular noun a person would use: "item", "attempt".
    #[error("{entity} not found")]
    NotFound { entity: &'static str },
    #[error("conflict: {0}")]
    Conflict(String),
    /// A value the database holds that this layer cannot read back: text that
    /// is no id, no instant, no variant of its enum, or no document of its
    /// type. It names the place the value was read from and keeps why it
    /// failed; it never carries the value, which can be a person's text.
    #[error("the stored {column} of a {table} row cannot be decoded")]
    Corrupt {
        table: &'static str,
        column: &'static str,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl StoreError {
    /// The code this failure is answered with: the store's own `NotFound` and `Conflict`, and
    /// `internal_error` for what the caller cannot act on (the database's error, a value that
    /// cannot be decoded).
    pub fn code(&self) -> StableErrorCode {
        match self {
            StoreError::NotFound { .. } => StableErrorCode::NotFound,
            StoreError::Conflict(_) => StableErrorCode::Conflict,
            StoreError::Sql(_) | StoreError::Corrupt { .. } => StableErrorCode::InternalError,
        }
    }

    /// The row of kind `entity` the call named is not there.
    pub fn not_found(entity: &'static str) -> Self {
        StoreError::NotFound { entity }
    }

    /// The stored `column` of a `table` row failed to decode, for `source`.
    pub fn corrupt(
        table: &'static str,
        column: &'static str,
        source: impl Into<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        StoreError::Corrupt {
            table,
            column,
            source: source.into(),
        }
    }

    /// A value this layer was asked to write cannot be written as the column
    /// needs it (an integer out of range, a document that does not serialize).
    pub(crate) fn unwritable(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        StoreError::Sql(sqlx::Error::Encode(source.into()))
    }
}
