//! Machine-generated OpenAPI 3.1 contract for the Tack HTTP API.
//!
//! The document is assembled at compile time by `utoipa` from the
//! `#[utoipa::path(...)]` annotations on the handlers plus the `#[derive(ToSchema)]`
//! DTOs in `tack-core` (behind its `openapi` feature) and the local response
//! envelopes below. It is served verbatim at `GET /api/openapi.json` and a copy
//! is committed to `docs/openapi.json`; a drift-gate test
//! (`tests/openapi_contract.rs`) fails CI if the two diverge.
//!
//! ## Known imprecise / manual spots (handoffs for a fully-precise spec)
//! - **`Json<serde_json::Value>` handlers.** Phase 29.2 deferred typed-DTO
//!   conversion for most handlers. Where a concrete DTO exists it is declared as
//!   the response `body`; where the JSON is genuinely ad-hoc — `{"deleted": true}`
//!   / `{"updated": true}`, import counters, backup manifests, the masked backup
//!   settings view — the response is modelled as a free-form `Object`. Those are
//!   accurate about *what the endpoint returns today*, not aspirational.
//! - **Multipart upload.** `POST /api/items/{item_id}/attachments` takes
//!   `multipart/form-data`; utoipa cannot infer that from the `Multipart`
//!   extractor, so its request body is hand-declared.
//! - **Undocumented on purpose.** The WebSocket upgrade
//!   `GET /api/projects/{id}/boards/live`, the Alexa webhook `POST /api/alexa`
//!   (Amazon-defined request envelope, skill-ID auth), and the SPA fallback are
//!   omitted.
//! - **Untagged enum variants.** `ItemType::Custom(String)`,
//!   `EstimateUnit::Custom(String)` and `BoardGrouping::CustomField(Uuid)` are
//!   externally-tagged, so they render as a `oneOf` mixing bare strings with
//!   single-key objects — faithful to the serde output but noisier than a plain
//!   string enum.

use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use tack_core::models::{
    Attachment, Board, BoardColumn, BoardGrouping, BoardView, Comment, CommentType, CreateBoard,
    CreateComment, CreateCustomField, CreateDependency, CreateItem, CreateProject,
    CreateProjectTemplate, CreateRole, CreateSprint, CustomFieldDefinition, CustomFieldType,
    CustomFieldValue, Dependency, DependencyType, EstimateUnit, Item, ItemRole, ItemSource,
    ItemType, Priority,
    Project, ProjectTemplate, ProjectType, Role, SetCustomFieldValue, Sprint, SprintStatus,
    UpdateBoard, UpdateCustomField, UpdateItem, UpdateProject, Workspace,
};
use tack_core::workflow::{StatusCategory, StatusDef, Transition, WorkflowConfig, WorkflowType};

use crate::handlers;

/// Structured error envelope returned by every failing endpoint — see
/// `crate::error::ApiError`. The body is always `{ "error": { status, message } }`.
#[derive(Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    /// HTTP status code, duplicated in the body for convenience.
    #[schema(example = 404)]
    pub status: u16,
    /// Human-readable, end-user-facing message.
    #[schema(example = "Item not found")]
    pub message: String,
}

/// Pagination envelope for the item-list endpoint (Phase 29.1). `total` is the
/// unpaginated match count so clients can render "N of M".
#[derive(Serialize, ToSchema)]
pub struct PaginatedItems {
    pub data: Vec<Item>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

/// Detail envelope for `GET /api/items/{id}` — the item plus its assigned roles
/// and dependency edges.
#[derive(Serialize, ToSchema)]
pub struct ItemDetail {
    pub item: Item,
    pub roles: Vec<Role>,
    pub dependencies: Vec<Dependency>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tack API",
        description = "REST + WebSocket API for Tack, a lightweight, workflow-agnostic \
            project-management tool. This contract is generated from the Rust handlers \
            and domain models; it is the single source of truth for the wire format. \
            All failing responses share the `{ \"error\": { \"status\", \"message\" } }` \
            envelope.",
        license(name = "MIT", identifier = "MIT"),
        contact(name = "Tack", email = "info@yielab.com"),
    ),
    paths(
        // ── System / debug ────────────────────────────────────────────────
        crate::debug::health,
        crate::debug::debug_info,
        crate::debug::db_stats,
        // ── Projects ──────────────────────────────────────────────────────
        handlers::projects::create_project,
        handlers::projects::list_projects,
        handlers::projects::get_project,
        handlers::projects::update_project,
        handlers::projects::delete_project,
        // ── Export / import ───────────────────────────────────────────────
        handlers::export::export_project,
        handlers::export::import_project,
        handlers::export::import_csv,
        handlers::import_github::import_github,
        handlers::import_linear::import_linear,
        // ── Items ─────────────────────────────────────────────────────────
        handlers::items::create_item,
        handlers::items::list_items,
        handlers::items::get_item_tree,
        handlers::items::search_items,
        handlers::items::search_items_global,
        handlers::items::get_item,
        handlers::items::update_item,
        handlers::items::delete_item,
        // ── Sprints ───────────────────────────────────────────────────────
        handlers::sprints::create_sprint,
        handlers::sprints::list_sprints,
        handlers::sprints::get_sprint,
        handlers::sprints::update_sprint_status,
        // ── Roles ─────────────────────────────────────────────────────────
        handlers::roles::create_role,
        handlers::roles::list_roles,
        handlers::roles::delete_role,
        handlers::roles::assign_role,
        handlers::roles::remove_role,
        // ── Comments ──────────────────────────────────────────────────────
        handlers::comments::create_comment,
        handlers::comments::list_comments,
        // ── Dependencies ──────────────────────────────────────────────────
        handlers::dependencies::create_dependency,
        handlers::dependencies::list_dependencies,
        handlers::dependencies::delete_dependency,
        // ── Attachments ───────────────────────────────────────────────────
        handlers::attachments::upload_attachment,
        handlers::attachments::list_attachments,
        handlers::attachments::download_attachment,
        handlers::attachments::delete_attachment,
        // ── Templates ─────────────────────────────────────────────────────
        handlers::templates::create_template,
        handlers::templates::list_templates,
        handlers::templates::get_template,
        handlers::templates::delete_template,
        handlers::templates::create_project_from_template,
        handlers::templates::save_project_as_template,
        // ── Custom fields ─────────────────────────────────────────────────
        handlers::custom_fields::create_field,
        handlers::custom_fields::list_fields,
        handlers::custom_fields::get_field,
        handlers::custom_fields::update_field,
        handlers::custom_fields::delete_field,
        handlers::custom_fields::set_field_value,
        handlers::custom_fields::get_field_value,
        handlers::custom_fields::delete_field_value,
        handlers::custom_fields::get_all_field_values,
        // ── Boards ────────────────────────────────────────────────────────
        handlers::boards_multi::create_board,
        handlers::boards_multi::list_boards,
        handlers::boards_multi::get_board,
        handlers::boards_multi::update_board,
        handlers::boards_multi::delete_board,
        handlers::boards_multi::get_board_view,
        // ── Backup / restore ──────────────────────────────────────────────
        handlers::backup::get_backup,
        handlers::backup::post_restore,
        handlers::backup::post_remote_backup,
        handlers::backup::get_remote_backups,
        handlers::backup::post_remote_restore,
        handlers::backup::post_remote_verify,
        // ── Settings ──────────────────────────────────────────────────────
        handlers::settings::get_backup_settings,
        handlers::settings::put_backup_settings,
        // ── Orchestration (Agent-Factory Control Center, Phase 33+) ────────
        handlers::orch::create_control_plane,
        handlers::orch::list_control_planes,
        handlers::orch::get_control_plane,
        handlers::orch::update_control_plane,
        handlers::orch::delete_control_plane,
        handlers::orch::get_orch_link,
        handlers::orch::put_orch_link,
        handlers::orch::get_fleet,
        handlers::orch::get_metrics,
        handlers::orch::get_item_agent_activity,
        handlers::orch::get_project_agent_activity,
        handlers::orch::dispatch_item,
    ),
    components(schemas(
        // Local response/request envelopes
        ErrorEnvelope,
        ErrorBody,
        PaginatedItems,
        ItemDetail,
        handlers::boards_multi::BoardViewResponse,
        handlers::boards_multi::BoardColumnWithItems,
        handlers::sprints::UpdateSprintStatus,
        handlers::templates::CreateProjectFromTemplate,
        handlers::templates::SaveAsTemplateRequest,
        handlers::import_github::GitHubImportRequest,
        handlers::import_linear::LinearImportRequest,
        handlers::backup::RestoreRemoteRequest,
        handlers::settings::UpdateBackupSettings,
        handlers::orch::ControlPlaneResponse,
        handlers::orch::CreateControlPlaneRequest,
        handlers::orch::UpdateControlPlaneRequest,
        handlers::orch::OrchLinkResponse,
        handlers::orch::OrchLinkView,
        handlers::orch::UpsertOrchLinkRequest,
        handlers::orch::StatusMap,
        handlers::orch::FleetEntry,
        handlers::orch::FleetListResponse,
        handlers::orch::FleetRosterMember,
        handlers::orch::ItemAgentEventResponse,
        handlers::orch::ItemAgentRunResponse,
        handlers::orch::ItemAgentAttemptResponse,
        handlers::orch::ItemAgentApprovalResponse,
        handlers::orch::ItemAgentActivityResponse,
        handlers::orch::AgentBadgeRowResponse,
        handlers::orch::AgentBadgeResponse,
        handlers::orch::DispatchedTaskResponse,
        handlers::orch::DispatchItemResponse,
        // Core domain models + DTOs
        Workspace,
        Project,
        ProjectType,
        Item,
        ItemType,
        ItemSource,
        Priority,
        EstimateUnit,
        Dependency,
        DependencyType,
        Role,
        ItemRole,
        Comment,
        CommentType,
        Attachment,
        Sprint,
        SprintStatus,
        BoardView,
        BoardColumn,
        Board,
        BoardGrouping,
        ProjectTemplate,
        CustomFieldDefinition,
        CustomFieldType,
        CustomFieldValue,
        CreateProject,
        UpdateProject,
        CreateItem,
        UpdateItem,
        CreateSprint,
        CreateRole,
        CreateComment,
        CreateDependency,
        CreateProjectTemplate,
        CreateCustomField,
        UpdateCustomField,
        SetCustomFieldValue,
        CreateBoard,
        UpdateBoard,
        WorkflowConfig,
        WorkflowType,
        StatusDef,
        StatusCategory,
        Transition,
    )),
    tags(
        (name = "system", description = "Health and debug probes."),
        (name = "projects", description = "Projects: the top-level container for work."),
        (name = "items", description = "Items: the universal work unit (epics, tasks, bugs, …)."),
        (name = "sprints", description = "Sprints / iterations within a project."),
        (name = "roles", description = "Roles / specialties and their assignment to items."),
        (name = "comments", description = "Comments on items."),
        (name = "dependencies", description = "Directed dependency edges between items."),
        (name = "attachments", description = "File attachments on items."),
        (name = "boards", description = "Saved board views and their grouped item layout."),
        (name = "custom-fields", description = "Per-project custom field definitions and values."),
        (name = "templates", description = "Reusable project templates."),
        (name = "import", description = "Import from JSON/YAML/CSV, GitHub Issues, and Linear."),
        (name = "export", description = "Project export to JSON / YAML / CSV."),
        (name = "search", description = "Full-text search within a project or globally."),
        (name = "backup", description = "Local and S3-compatible cloud backup / restore."),
        (name = "settings", description = "Runtime-editable server settings (cloud backup)."),
        (name = "orchestration", description = "Agent-Factory Control Center: control-plane registration, \
            per-project links, and the Fleet view aggregate. Every route is disabled — 404 — unless \
            TACK_ORCH_ENABLE is set."),
    ),
)]
pub struct ApiDoc;

/// `GET /api/openapi.json` — serve the generated OpenAPI 3.1 document.
///
/// Public: reading the schema requires no auth (the token gate exempts this
/// path in `crate::middleware`).
pub async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
