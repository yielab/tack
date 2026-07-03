use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::vocabulary::VocabularyMap;
use crate::workflow::{StatusCategory, WorkflowConfig};

// ─── Workspace ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProjectType {
    Software,
    Web,
    Mobile,
    Construction,
    Personal,
    Homework,
    Maintenance,
    Legal,
    Research,
    Event,
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
            Self::Legal => write!(f, "legal"),
            Self::Research => write!(f, "research"),
            Self::Event => write!(f, "event"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ─── Item (universal work unit) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum EstimateUnit {
    #[default]
    StoryPoints,
    Hours,
    Days,
    Custom(String),
}

// ─── Dependency ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Dependency {
    pub id: Uuid,
    pub source_item_id: Uuid,
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Role {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemRole {
    pub item_id: Uuid,
    pub role_id: Uuid,
}

// ─── Comment ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CommentType {
    Comment,
    StatusChange,
    Edit,
    System,
}

// ─── Attachment ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BoardColumn {
    pub status: String,
    pub wip_limit: Option<usize>,
    pub collapsed: bool,
}

// ─── DTOs for creation/updates ───────────────────────────────

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub template: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub archived: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

#[derive(Debug, Default, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    // Double-`Option` fields: outer `None` = key absent (leave untouched),
    // `Some(None)` = JSON `null` (clear the column), `Some(Some(v))` = set to `v`.
    // This lets the frontend clear a sprint assignment / due date / estimate unit
    // by sending `null`, which a plain `Option<T>` cannot distinguish from "absent".
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub estimate_unit: Option<Option<EstimateUnit>>,
    #[validate(length(max = 20, message = "too many tags (max 20)"))]
    pub tags: Option<Vec<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub sprint_id: Option<Option<Uuid>>,
    pub sort_order: Option<i32>,
    #[validate(length(max = 200, message = "assignee name too long (max 200 chars)"))]
    pub assignee: Option<String>,
    /// Server-only: the target status's category, populated by the update handler
    /// when the status changes so the persistence layer can maintain
    /// `started_at` / `completed_at`. Never (de)serialized from client requests.
    #[serde(skip)]
    pub status_category: Option<StatusCategory>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateSprint {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 2_000, message = "goal too long (max 2 000 chars)"))]
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateRole {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateComment {
    #[validate(length(min = 1, max = 10_000, message = "content must be 1–10 000 characters"))]
    pub content: String,
    #[validate(length(max = 200, message = "author name too long (max 200 chars)"))]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateDependency {
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
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

impl ItemFilter {
    /// Default page size when the client does not specify one.
    pub const DEFAULT_PER_PAGE: u32 = 100;
    /// Hard cap on page size so a single request can never scan the whole table.
    pub const MAX_PER_PAGE: u32 = 500;

    /// The effective, clamped page size (1..=`MAX_PER_PAGE`). Shared by the
    /// list query, the pagination envelope, and any caller that needs to know
    /// how many rows a page can hold.
    pub fn effective_per_page(&self) -> u32 {
        self.per_page
            .unwrap_or(Self::DEFAULT_PER_PAGE)
            .clamp(1, Self::MAX_PER_PAGE)
    }

    /// The effective, 1-based page number (never below 1).
    pub fn effective_page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }
}

// ─── Project Template ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BoardTemplate {
    pub name: String,
    pub description: Option<String>,
    pub columns: Vec<BoardColumn>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CustomFieldValue {
    pub id: Uuid,
    pub item_id: Uuid,
    pub field_id: Uuid,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetCustomFieldValue {
    pub field_id: Uuid,
    pub value: serde_json::Value,
}

impl CustomFieldDefinition {
    /// Validate a JSON value against this field's type, options, and `validation` rules.
    /// Returns `Err(message)` if the value is invalid.
    pub fn validate_value(&self, value: &serde_json::Value) -> Result<(), String> {
        // ── Type + options check ─────────────────────────────────
        match &self.field_type {
            CustomFieldType::Text | CustomFieldType::LongText | CustomFieldType::Email => {
                if !value.is_string() {
                    return Err(format!("field '{}' expects a string value", self.name));
                }
            }
            CustomFieldType::Url => match value.as_str() {
                Some(s) if s.starts_with("http://") || s.starts_with("https://") => {}
                Some(_) => {
                    return Err(format!(
                        "field '{}' expects a URL starting with http:// or https://",
                        self.name
                    ));
                }
                None => return Err(format!("field '{}' expects a URL string", self.name)),
            },
            CustomFieldType::Number => {
                if !value.is_number() {
                    return Err(format!("field '{}' expects a numeric value", self.name));
                }
            }
            CustomFieldType::Boolean => {
                if !value.is_boolean() {
                    return Err(format!("field '{}' expects a boolean value", self.name));
                }
            }
            CustomFieldType::Date => match value.as_str() {
                Some(s) => {
                    let ok = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
                        || chrono::DateTime::parse_from_rfc3339(s).is_ok();
                    if !ok {
                        return Err(format!(
                            "field '{}' expects an ISO 8601 date (YYYY-MM-DD or RFC3339)",
                            self.name
                        ));
                    }
                }
                None => return Err(format!("field '{}' expects a date string", self.name)),
            },
            CustomFieldType::Select => match value.as_str() {
                Some(s) => {
                    if let Some(opts) = &self.options
                        && !opts.iter().any(|o| o == s)
                    {
                        return Err(format!(
                            "field '{}': '{}' is not a valid option",
                            self.name, s
                        ));
                    }
                }
                None => return Err(format!("field '{}' expects a string value", self.name)),
            },
            CustomFieldType::MultiSelect => match value.as_array() {
                Some(arr) => {
                    for v in arr {
                        match v.as_str() {
                            Some(s) => {
                                if let Some(opts) = &self.options
                                    && !opts.iter().any(|o| o == s)
                                {
                                    return Err(format!(
                                        "field '{}': '{}' is not a valid option",
                                        self.name, s
                                    ));
                                }
                            }
                            None => {
                                return Err(format!(
                                    "field '{}' expects an array of strings",
                                    self.name
                                ));
                            }
                        }
                    }
                }
                None => {
                    return Err(format!("field '{}' expects an array of strings", self.name));
                }
            },
        }

        // ── Extra validation rules from the `validation` JSON field ──
        if let Some(rules) = &self.validation {
            self.apply_validation_rules(value, rules)?;
        }

        Ok(())
    }

    /// Apply the extra validation rules stored in the `validation` JSON field.
    ///
    /// Supported rule keys:
    /// - `pattern`    (string)  — regex applied to Text/LongText/Email/Url
    /// - `min_length` (u64)    — minimum string length
    /// - `max_length` (u64)    — maximum string length
    /// - `min`        (f64)    — minimum numeric value
    /// - `max`        (f64)    — maximum numeric value
    /// - `max_items`  (u64)    — maximum array length for MultiSelect
    fn apply_validation_rules(
        &self,
        value: &serde_json::Value,
        rules: &serde_json::Value,
    ) -> Result<(), String> {
        // pattern — regex for string values
        if let Some(pattern) = rules.get("pattern").and_then(|v| v.as_str())
            && let Some(s) = value.as_str()
        {
            match Regex::new(pattern) {
                Ok(re) => {
                    if !re.is_match(s) {
                        return Err(format!(
                            "field '{}': value does not match required pattern",
                            self.name
                        ));
                    }
                }
                Err(_) => {
                    return Err(format!(
                        "field '{}': invalid regex pattern in field definition",
                        self.name
                    ));
                }
            }
        }

        // min_length / max_length — for string values
        if let Some(s) = value.as_str() {
            let len = s.chars().count() as u64;
            if let Some(min) = rules.get("min_length").and_then(|v| v.as_u64())
                && len < min
            {
                return Err(format!(
                    "field '{}': value is too short (min {} characters)",
                    self.name, min
                ));
            }
            if let Some(max) = rules.get("max_length").and_then(|v| v.as_u64())
                && len > max
            {
                return Err(format!(
                    "field '{}': value is too long (max {} characters)",
                    self.name, max
                ));
            }
        }

        // min / max — for numeric values
        if let Some(n) = value.as_f64() {
            if let Some(min) = rules.get("min").and_then(|v| v.as_f64())
                && n < min
            {
                return Err(format!(
                    "field '{}': value {} is below minimum {}",
                    self.name, n, min
                ));
            }
            if let Some(max) = rules.get("max").and_then(|v| v.as_f64())
                && n > max
            {
                return Err(format!(
                    "field '{}': value {} exceeds maximum {}",
                    self.name, n, max
                ));
            }
        }

        // max_items — for array values (MultiSelect)
        if let Some(arr) = value.as_array()
            && let Some(max_items) = rules.get("max_items").and_then(|v| v.as_u64())
            && arr.len() as u64 > max_items
        {
            return Err(format!(
                "field '{}': too many selections (max {})",
                self.name, max_items
            ));
        }

        Ok(())
    }
}

// ─── Board (Multiple Boards per Project) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum BoardGrouping {
    Status,            // Group by status (default Kanban)
    Priority,          // Group by priority
    ItemType,          // Group by item type
    Sprint,            // Group by sprint
    Assignee,          // Group by assignee (if we add that)
    CustomField(Uuid), // Group by custom field
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBoard {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateBoard {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_field(
        field_type: CustomFieldType,
        options: Option<Vec<String>>,
    ) -> CustomFieldDefinition {
        CustomFieldDefinition {
            id: Uuid::new_v4(),
            project_id: None,
            name: "test_field".to_string(),
            field_type,
            description: None,
            required: false,
            default_value: None,
            options,
            validation: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn validate_text_accepts_string() {
        let f = make_field(CustomFieldType::Text, None);
        assert!(f.validate_value(&json!("hello")).is_ok());
    }

    #[test]
    fn validate_text_rejects_number() {
        let f = make_field(CustomFieldType::Text, None);
        assert!(f.validate_value(&json!(42)).is_err());
    }

    #[test]
    fn validate_number_accepts_int_and_float() {
        let f = make_field(CustomFieldType::Number, None);
        assert!(f.validate_value(&json!(42)).is_ok());
        assert!(f.validate_value(&json!(2.75)).is_ok());
    }

    #[test]
    fn validate_number_rejects_string() {
        let f = make_field(CustomFieldType::Number, None);
        assert!(f.validate_value(&json!("not a number")).is_err());
    }

    #[test]
    fn validate_boolean_accepts_true_and_false() {
        let f = make_field(CustomFieldType::Boolean, None);
        assert!(f.validate_value(&json!(true)).is_ok());
        assert!(f.validate_value(&json!(false)).is_ok());
    }

    #[test]
    fn validate_boolean_rejects_string() {
        let f = make_field(CustomFieldType::Boolean, None);
        assert!(f.validate_value(&json!("true")).is_err());
    }

    #[test]
    fn validate_date_accepts_ymd() {
        let f = make_field(CustomFieldType::Date, None);
        assert!(f.validate_value(&json!("2025-06-15")).is_ok());
    }

    #[test]
    fn validate_date_accepts_rfc3339() {
        let f = make_field(CustomFieldType::Date, None);
        assert!(f.validate_value(&json!("2025-06-15T12:00:00Z")).is_ok());
    }

    #[test]
    fn validate_date_rejects_freeform() {
        let f = make_field(CustomFieldType::Date, None);
        assert!(f.validate_value(&json!("June 15 2025")).is_err());
    }

    #[test]
    fn validate_url_accepts_http_and_https() {
        let f = make_field(CustomFieldType::Url, None);
        assert!(f.validate_value(&json!("https://example.com")).is_ok());
        assert!(f.validate_value(&json!("http://example.com")).is_ok());
    }

    #[test]
    fn validate_url_rejects_bare_domain() {
        let f = make_field(CustomFieldType::Url, None);
        assert!(f.validate_value(&json!("example.com")).is_err());
    }

    #[test]
    fn validate_select_accepts_valid_option() {
        let f = make_field(
            CustomFieldType::Select,
            Some(vec!["Red".into(), "Blue".into()]),
        );
        assert!(f.validate_value(&json!("Red")).is_ok());
    }

    #[test]
    fn validate_select_rejects_unlisted_option() {
        let f = make_field(
            CustomFieldType::Select,
            Some(vec!["Red".into(), "Blue".into()]),
        );
        assert!(f.validate_value(&json!("Green")).is_err());
    }

    #[test]
    fn validate_multiselect_accepts_valid_array() {
        let f = make_field(
            CustomFieldType::MultiSelect,
            Some(vec!["A".into(), "B".into(), "C".into()]),
        );
        assert!(f.validate_value(&json!(["A", "C"])).is_ok());
    }

    #[test]
    fn validate_multiselect_rejects_unlisted_element() {
        let f = make_field(
            CustomFieldType::MultiSelect,
            Some(vec!["A".into(), "B".into()]),
        );
        assert!(f.validate_value(&json!(["A", "Z"])).is_err());
    }

    #[test]
    fn validate_multiselect_rejects_non_array() {
        let f = make_field(CustomFieldType::MultiSelect, None);
        assert!(f.validate_value(&json!("A")).is_err());
    }

    fn field_with_validation(
        field_type: CustomFieldType,
        options: Option<Vec<String>>,
        validation: serde_json::Value,
    ) -> CustomFieldDefinition {
        let mut f = make_field(field_type, options);
        f.validation = Some(validation);
        f
    }

    #[test]
    fn validation_pattern_accepts_matching_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"pattern": r"^\d{4}$"}));
        assert!(f.validate_value(&json!("1234")).is_ok());
    }

    #[test]
    fn validation_pattern_rejects_non_matching_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"pattern": r"^\d{4}$"}));
        assert!(f.validate_value(&json!("abc")).is_err());
    }

    #[test]
    fn validation_min_length_accepts_long_enough_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"min_length": 3}));
        assert!(f.validate_value(&json!("hello")).is_ok());
    }

    #[test]
    fn validation_min_length_rejects_short_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"min_length": 3}));
        assert!(f.validate_value(&json!("hi")).is_err());
    }

    #[test]
    fn validation_max_length_accepts_short_enough_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"max_length": 5}));
        assert!(f.validate_value(&json!("hello")).is_ok());
    }

    #[test]
    fn validation_max_length_rejects_too_long_string() {
        let f = field_with_validation(CustomFieldType::Text, None, json!({"max_length": 5}));
        assert!(f.validate_value(&json!("toolong")).is_err());
    }

    #[test]
    fn validation_min_accepts_value_at_boundary() {
        let f = field_with_validation(CustomFieldType::Number, None, json!({"min": 0}));
        assert!(f.validate_value(&json!(0)).is_ok());
    }

    #[test]
    fn validation_min_rejects_value_below_boundary() {
        let f = field_with_validation(CustomFieldType::Number, None, json!({"min": 0}));
        assert!(f.validate_value(&json!(-1)).is_err());
    }

    #[test]
    fn validation_max_accepts_value_at_boundary() {
        let f = field_with_validation(CustomFieldType::Number, None, json!({"max": 100}));
        assert!(f.validate_value(&json!(100)).is_ok());
    }

    #[test]
    fn validation_max_rejects_value_above_boundary() {
        let f = field_with_validation(CustomFieldType::Number, None, json!({"max": 100}));
        assert!(f.validate_value(&json!(101)).is_err());
    }

    #[test]
    fn validation_max_items_accepts_array_within_limit() {
        let f = field_with_validation(
            CustomFieldType::MultiSelect,
            Some(vec!["A".into(), "B".into(), "C".into()]),
            json!({"max_items": 2}),
        );
        assert!(f.validate_value(&json!(["A", "B"])).is_ok());
    }

    #[test]
    fn validation_max_items_rejects_array_exceeding_limit() {
        let f = field_with_validation(
            CustomFieldType::MultiSelect,
            Some(vec!["A".into(), "B".into(), "C".into()]),
            json!({"max_items": 2}),
        );
        assert!(f.validate_value(&json!(["A", "B", "C"])).is_err());
    }

    // 26.1 — the double-`Option` PATCH fields must distinguish "absent" (leave
    // untouched) from "null" (clear) from an explicit value.
    #[test]
    fn update_item_double_option_distinguishes_absent_null_and_value() {
        // Absent → outer None.
        let absent: UpdateItem = serde_json::from_value(json!({})).unwrap();
        assert_eq!(absent.sprint_id, None);
        assert_eq!(absent.due_date, None);
        assert_eq!(absent.estimate_unit, None);

        // Explicit null → Some(None) (clear).
        let nulled: UpdateItem =
            serde_json::from_value(json!({"sprint_id": null, "due_date": null})).unwrap();
        assert_eq!(nulled.sprint_id, Some(None));
        assert_eq!(nulled.due_date, Some(None));

        // Concrete value → Some(Some(_)).
        let id = Uuid::new_v4();
        let set: UpdateItem =
            serde_json::from_value(json!({"sprint_id": id.to_string()})).unwrap();
        assert_eq!(set.sprint_id, Some(Some(id)));

        // status_category is server-only: never populated from client JSON.
        let with_cat: UpdateItem =
            serde_json::from_value(json!({"status_category": "done"})).unwrap();
        assert_eq!(with_cat.status_category, None);
    }
}
