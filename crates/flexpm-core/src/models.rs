use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateItem {
    pub title: String,
    pub description: Option<String>,
    pub item_type: Option<ItemType>,
    pub parent_id: Option<Uuid>,
    pub priority: Option<Priority>,
    pub estimate: Option<f64>,
    pub estimate_unit: Option<EstimateUnit>,
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub sprint_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItem {
    pub title: Option<String>,
    pub description: Option<String>,
    pub item_type: Option<ItemType>,
    pub status: Option<String>,
    pub priority: Option<Priority>,
    pub estimate: Option<f64>,
    pub estimate_unit: Option<EstimateUnit>,
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub sprint_id: Option<Uuid>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSprint {
    pub name: String,
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRole {
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateComment {
    pub content: String,
    pub author: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependency {
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Deserialize)]
pub struct ItemFilter {
    pub status: Option<String>,
    pub item_type: Option<ItemType>,
    pub priority: Option<Priority>,
    pub sprint_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}
