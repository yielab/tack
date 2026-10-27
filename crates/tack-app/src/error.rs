use tack_core::CoreError;
use tack_db::StoreError;
use tack_protocol::execution::StableErrorCode;

/// What a use case can fail with. A use case raises the first four itself, where it checks the
/// rule; the last two come from the layers below through `?`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid transition from '{from}' to '{to}'")]
    InvalidTransition { from: String, to: String },
    /// A runner presented a fence older than the one the attempt holds.
    #[error("stale lease on attempt {attempt_id}: fence {fence} is no longer current")]
    StaleLease { attempt_id: String, fence: i64 },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl AppError {
    /// The runner-v1 code this failure carries on the wire. Only the codes the use cases can
    /// raise appear here; everything else a caller sends is its own to name.
    pub fn code(&self) -> StableErrorCode {
        match self {
            AppError::NotFound | AppError::Store(StoreError::NotFound { .. }) => {
                StableErrorCode::NotFound
            }
            AppError::Conflict(_) | AppError::Store(StoreError::Conflict(_)) => {
                StableErrorCode::Conflict
            }
            AppError::InvalidTransition { .. } => StableErrorCode::InvalidTransition,
            AppError::StaleLease { .. } => StableErrorCode::StaleLease,
            AppError::Store(StoreError::Sql(_) | StoreError::Corrupt { .. })
            | AppError::Core(_) => StableErrorCode::InternalError,
        }
    }
}
