use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::vocabulary::VocabularyMap;
use crate::workflow::WorkflowConfig;

// ─── Workspace ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub default_vocabulary: VocabularyMap,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Project ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: VocabularyMap,
    pub workflow: WorkflowConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Software,
    Web,
    Mobile,
    Construction,
    Personal,
    Homework,
    Maintenance,
    Custom,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Software => write!(f, "software"),
            Self::Web => write!(f, "web"),
            Self::Mobile => write!(f, "mobile"),
            Self::Construction => write!(f, "construction"),
            Self::Personal => write!(f, "personal"),
            Self::Homework => write!(f, "homework"),
            Self::Maintenance => write!(f, "maintenance"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ─── Item (universal work unit) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub item_type: ItemType,
    pub status: String,
    pub priority: Priority,
    pub estimate: Option<f64>,
    pub estimate_unit: EstimateUnit,
    pub tags: Vec<String>,
    pub sort_order: i32,
    pub sprint_id: Option<Uuid>,
    pub assignee: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Epic,
    Feature,
    Task,
    Subtask,
    Bug,
    Requirement,
    Custom(String),
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Epic => write!(f, "epic"),
            Self::Feature => write!(f, "feature"),
            Self::Task => write!(f, "task"),
            Self::Subtask => write!(f, "subtask"),
            Self::Bug => write!(f, "bug"),
            Self::Requirement => write!(f, "requirement"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
    None,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
            Self::None => write!(f, "none"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EstimateUnit {
    #[default]
    StoryPoints,
    Hours,
    Days,
    Custom(String),
}

// ─── Dependency ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: Uuid,
    pub source_item_id: Uuid,
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Blocks,
    IsBlockedBy,
    RelatesTo,
    Duplicates,
}

impl std::fmt::Display for DependencyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocks => write!(f, "blocks"),
            Self::IsBlockedBy => write!(f, "is_blocked_by"),
            Self::RelatesTo => write!(f, "relates_to"),
            Self::Duplicates => write!(f, "duplicates"),
        }
    }
}

// ─── Role / Specialty ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRole {
    pub item_id: Uuid,
    pub role_id: Uuid,
}

// ─── Comment ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: Uuid,
    pub item_id: Uuid,
    pub author: Option<String>,
    pub content: String,
    pub comment_type: CommentType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommentType {
    Comment,
    StatusChange,
    Edit,
    System,
}

// ─── Attachment ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub item_id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub storage_path: String,
    pub size_bytes: u64,
    pub uploaded_at: DateTime<Utc>,
}

// ─── Sprint ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprint {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub status: SprintStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SprintStatus {
    Planning,
    Active,
    Review,
    Closed,
}

impl std::fmt::Display for SprintStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "planning"),
            Self::Active => write!(f, "active"),
            Self::Review => write!(f, "review"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

// ─── Board View Config ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub columns: Vec<BoardColumn>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardColumn {
    pub status: String,
    pub wip_limit: Option<usize>,
    pub collapsed: bool,
}

// ─── DTOs for creation/updates ───────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub template: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub archived: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateItem {
    #[validate(length(min = 1, max = 500, message = "title must be 1–500 characters"))]
    pub title: String,
    #[validate(length(max = 50_000, message = "description too long (max 50 000 chars)"))]
    pub description: Option<String>,
    pub item_type: Option<ItemType>,
    pub parent_id: Option<Uuid>,
    pub priority: Option<Priority>,
    #[validate(range(min = 0.0, message = "estimate must be non-negative"))]
    pub estimate: Option<f64>,
    pub estimate_unit: Option<EstimateUnit>,
    #[validate(length(max = 20, message = "too many tags (max 20)"))]
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub sprint_id: Option<Uuid>,
    #[validate(length(max = 200, message = "assignee name too long (max 200 chars)"))]
    pub assignee: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateItem {
    #[validate(length(min = 1, max = 500, message = "title must be 1–500 characters"))]
    pub title: Option<String>,
    #[validate(length(max = 50_000, message = "description too long (max 50 000 chars)"))]
    pub description: Option<String>,
    pub item_type: Option<ItemType>,
    #[validate(length(min = 1, max = 100, message = "status must be 1–100 characters"))]
    pub status: Option<String>,
    pub priority: Option<Priority>,
    #[validate(range(min = 0.0, message = "estimate must be non-negative"))]
    pub estimate: Option<f64>,
    pub estimate_unit: Option<EstimateUnit>,
    #[validate(length(max = 20, message = "too many tags (max 20)"))]
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub sprint_id: Option<Uuid>,
    pub sort_order: Option<i32>,
    #[validate(length(max = 200, message = "assignee name too long (max 200 chars)"))]
    pub assignee: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateSprint {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 2_000, message = "goal too long (max 2 000 chars)"))]
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateRole {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateComment {
    #[validate(length(min = 1, max = 10_000, message = "content must be 1–10 000 characters"))]
    pub content: String,
    #[validate(length(max = 200, message = "author name too long (max 200 chars)"))]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependency {
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Deserialize, Default)]
pub struct ItemFilter {
    pub status: Option<String>,
    pub item_type: Option<ItemType>,
    pub priority: Option<Priority>,
    pub sprint_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub assignee: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

// ─── Project Template ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: VocabularyMap,
    pub workflow: WorkflowConfig,
    pub custom_fields: Vec<CustomFieldDefinition>,
    pub default_boards: Vec<BoardTemplate>,
    pub is_builtin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardTemplate {
    pub name: String,
    pub description: Option<String>,
    pub columns: Vec<BoardColumn>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProjectTemplate {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub custom_fields: Option<Vec<CustomFieldDefinition>>,
    pub default_boards: Option<Vec<BoardTemplate>>,
}

// ─── Custom Fields ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldDefinition {
    pub id: Uuid,
    pub project_id: Option<Uuid>, // None for template-level fields
    pub name: String,
    pub field_type: CustomFieldType,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<Vec<String>>, // For select/multi-select types
    pub validation: Option<serde_json::Value>, // JSON schema or regex
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CustomFieldType {
    Text,
    Number,
    Date,
    Boolean,
    Select,      // Single select from options
    MultiSelect, // Multiple select from options
    Url,
    Email,
    LongText, // Textarea
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldValue {
    pub id: Uuid,
    pub item_id: Uuid,
    pub field_id: Uuid,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateCustomField {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: String,
    pub field_type: CustomFieldType,
    #[validate(length(max = 1_000, message = "description too long (max 1 000 chars)"))]
    pub description: Option<String>,
    pub required: Option<bool>,
    pub default_value: Option<serde_json::Value>,
    #[validate(length(max = 100, message = "too many options (max 100)"))]
    pub options: Option<Vec<String>>,
    pub validation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCustomField {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 1_000, message = "description too long (max 1 000 chars)"))]
    pub description: Option<String>,
    pub required: Option<bool>,
    pub default_value: Option<serde_json::Value>,
    #[validate(length(max = 100, message = "too many options (max 100)"))]
    pub options: Option<Vec<String>>,
    pub validation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SetCustomFieldValue {
    pub field_id: Uuid,
    pub value: serde_json::Value,
}

// ─── Board (Multiple Boards per Project) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>, // Filter criteria for this board
    pub grouping: Option<BoardGrouping>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BoardGrouping {
    Status,            // Group by status (default Kanban)
    Priority,          // Group by priority
    ItemType,          // Group by item type
    Sprint,            // Group by sprint
    Assignee,          // Group by assignee (if we add that)
    CustomField(Uuid), // Group by custom field
}

#[derive(Debug, Deserialize)]
pub struct CreateBoard {
    pub name: String,
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBoard {
    pub name: Option<String>,
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}
