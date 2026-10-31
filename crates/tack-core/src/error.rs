use tack_protocol::execution::StableErrorCode;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Item not found: {0}")]
    ItemNotFound(Uuid),

    #[error("Project not found: {0}")]
    ProjectNotFound(Uuid),

    #[error("Sprint not found: {0}")]
    SprintNotFound(Uuid),

    #[error("Role not found: {0}")]
    RoleNotFound(Uuid),

    #[error("Invalid status transition from '{from}' to '{to}'")]
    InvalidTransition { from: String, to: String },

    #[error("WIP limit exceeded for column '{column}': limit is {limit}, current is {current}")]
    WipLimitExceeded {
        column: String,
        limit: usize,
        current: usize,
    },

    #[error("Dependency cycle detected involving item {0}")]
    DependencyCycle(Uuid),

    #[error("Duplicate dependency between {source_id} and {target_id}")]
    DuplicateDependency { source_id: Uuid, target_id: Uuid },

    #[error("Invalid vocabulary key: {0}")]
    InvalidVocabularyKey(String),

    #[error("Workflow has no statuses defined")]
    EmptyWorkflow,

    #[error("Cannot delete item {0}: it has {1} children")]
    HasChildren(Uuid, usize),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid workflow: {0}")]
    InvalidWorkflow(String),
}

impl CoreError {
    /// The code of each rule: one per rule, so a client can tell a full column from a forbidden
    /// move without reading the sentence. [`StableErrorCode::status`] gives the HTTP status.
    pub fn code(&self) -> StableErrorCode {
        match self {
            CoreError::ItemNotFound(_)
            | CoreError::ProjectNotFound(_)
            | CoreError::SprintNotFound(_)
            | CoreError::RoleNotFound(_) => StableErrorCode::NotFound,
            CoreError::InvalidTransition { .. } => StableErrorCode::InvalidStatusTransition,
            CoreError::WipLimitExceeded { .. } => StableErrorCode::WipLimitExceeded,
            CoreError::DependencyCycle(_) => StableErrorCode::DependencyCycle,
            CoreError::DuplicateDependency { .. } => StableErrorCode::DuplicateDependency,
            CoreError::InvalidVocabularyKey(_) => StableErrorCode::InvalidVocabularyKey,
            CoreError::EmptyWorkflow => StableErrorCode::EmptyWorkflow,
            CoreError::HasChildren(_, _) => StableErrorCode::HasChildren,
            CoreError::Validation(_) => StableErrorCode::ValidationFailed,
            CoreError::InvalidWorkflow(_) => StableErrorCode::InvalidWorkflow,
        }
    }
}
