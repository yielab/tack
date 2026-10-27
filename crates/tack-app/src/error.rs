use tack_core::CoreError;
use tack_db::StoreError;
use tack_protocol::execution::StableErrorCode;

use crate::runs::start_run::Refusal;

/// What a use case can fail with, and the one family every layer above reads: each variant
/// answers [`AppError::code`], and the code decides the HTTP status. A use case raises the first
/// five itself, where it checks the rule; the last two come from the layers below through `?`.
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
    /// A run that was not started, with the code and structured details it answers.
    #[error(transparent)]
    Refused(#[from] Refusal),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl AppError {
    /// The code this failure carries on the wire; [`StableErrorCode::status`] gives its HTTP
    /// status. A rule of the core has a code of its own.
    pub fn code(&self) -> StableErrorCode {
        match self {
            AppError::Refused(refusal) => refusal.code,
            AppError::NotFound => StableErrorCode::NotFound,
            AppError::Conflict(_) => StableErrorCode::Conflict,
            AppError::InvalidTransition { .. } => StableErrorCode::InvalidTransition,
            AppError::StaleLease { .. } => StableErrorCode::StaleLease,
            AppError::Store(failure) => failure.code(),
            AppError::Core(rule) => rule.code(),
        }
    }
}
