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
}
