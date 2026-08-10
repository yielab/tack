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
use utoipa::openapi::path::{
    HttpMethod, OperationBuilder, Parameter, ParameterBuilder, ParameterIn, PathItem, PathsBuilder,
};
use utoipa::openapi::request_body::RequestBodyBuilder;
use utoipa::openapi::response::ResponseBuilder;
use utoipa::openapi::schema::{SchemaType, Type};
use utoipa::openapi::{
    ContentBuilder, Info, ObjectBuilder, Ref, RefOr, Required, Response, Schema,
};
use utoipa::{OpenApi, PartialSchema, ToSchema};

use tack_core::models::{
    Attachment, Board, BoardColumn, BoardGrouping, BoardView, Comment, CommentType, CreateBoard,
    CreateComment, CreateCustomField, CreateDependency, CreateItem, CreateProject,
    CreateProjectTemplate, CreateRole, CreateSprint, CustomFieldDefinition, CustomFieldType,
    CustomFieldValue, Dependency, DependencyType, EstimateUnit, Item, ItemRole, ItemSource,
    ItemType, OrchBlueprint, Priority, Project, ProjectTemplate, ProjectType, Role,
    SetCustomFieldValue, Sprint, SprintStatus, TemplateOrchestration, TemplateStatusMap,
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
    /// Stable, machine-readable error code. Present on a narrow set of
    /// responses where a caller needs to branch on *why* without parsing
    /// `message` — e.g. `orchestration_disabled` on the 409 every
    /// orchestration route returns while the feature is switched off (see
    /// `handlers::orch::require_orch_enabled`). Absent on ordinary errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "orchestration_disabled")]
    pub code: Option<String>,
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

// ─────────────────────────────────────────────────────────────────────────
// Card C5: operator execution/fleet routes (card C1) + runner protocol v1
// (card C2).
//
// Both `handlers::executions`/`handlers::runner_admin` and
// `handlers::runner_protocol` return raw `Json<serde_json::Value>` with no
// `#[utoipa::path(...)]` annotation, and per III.2 rule 2 those files are
// owned by other Part III cards — C5 may create modules and mount routes,
// but may not edit a card-local handler file to add one. Building small
// `utoipa::OpenApi`-implementing document fragments here (this file, C5's
// own) and composing them into `ApiDoc` via `#[openapi(nest(...))]` below is
// the only way to document the real, mounted route surface without
// touching an unowned file — `OpenApi::nest`'s path-prefixing is exactly
// what turns each fragment's relative paths (e.g. `/executions`, `/enroll`)
// into the real mounted paths (`/api/executions`, `/api/runner/v1/enroll`).
//
// Bodies use the same free-form `serde_json::Value` schema this file
// already uses for every other ad hoc JSON handler (see the module doc's
// "`Json<serde_json::Value>` handlers" note above) — this is *not* a
// second, hand-maintained shape for the runner-v1 wire format. That
// contract remains solely governed by `docs/contracts/runner-v1/`
// (III.1.6: "hand-written feature DTOs are not another authority"); the
// per-operation `description` below points back to it instead of
// re-specifying field shapes OpenAPI can't independently verify against
// the frozen fixtures. `x-tack-principal` is deliberately **not**
// documented as a request header anywhere below: it is stripped and
// server-injected (`crate::middleware::inject_operator_principal`), never
// something a caller may set, so documenting it as a settable input would
// misrepresent the security model.
// ─────────────────────────────────────────────────────────────────────────

fn json_value_schema() -> RefOr<Schema> {
    <serde_json::Value as PartialSchema>::schema()
}

fn json_content() -> utoipa::openapi::Content {
    ContentBuilder::new()
        .schema(Some(json_value_schema()))
        .build()
}

fn error_envelope_content() -> utoipa::openapi::Content {
    ContentBuilder::new()
        .schema(Some(Ref::from_schema_name("ErrorEnvelope")))
        .build()
}

fn ok_response(description: &str) -> Response {
    ResponseBuilder::new()
        .description(description)
        .content("application/json", json_content())
        .build()
}

fn error_response(description: &str) -> Response {
    ResponseBuilder::new()
        .description(description)
        .content("application/json", error_envelope_content())
        .build()
}

fn string_path_param(name: &'static str, description: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .description(Some(description))
        .schema(Some(
            ObjectBuilder::new().schema_type(SchemaType::Type(Type::String)),
        ))
        .build()
}

fn json_request_body(description: &str) -> utoipa::openapi::request_body::RequestBody {
    RequestBodyBuilder::new()
        .description(Some(description))
        .required(Some(Required::True))
        .content("application/json", json_content())
        .build()
}

/// Standard error responses shared by both the operator and runner-v1
/// operations below — every stable v1 error code
/// (`docs/contracts/runner-v1/errors/*.json`) maps to one of these HTTP
/// statuses; `ok_description`/`request_body` are the only per-operation
/// specifics.
fn json_operation(
    tag: &str,
    summary: &str,
    description: &str,
    params: Vec<Parameter>,
    request_body: Option<&str>,
    ok_description: &str,
) -> OperationBuilder {
    let mut op = OperationBuilder::new()
        .tag(tag)
        .summary(Some(summary))
        .description(Some(description))
        .response("200", ok_response(ok_description))
        .response("400", error_response("invalid_request"))
        .response("401", error_response("unauthorized"))
        .response("403", error_response("forbidden / runner_revoked"))
        .response("404", error_response("not_found"))
        .response(
            "409",
            error_response("conflict / idempotency_conflict / invalid_transition / stale_lease"),
        );
    if !params.is_empty() {
        op = op.parameters(Some(params));
    }
    if let Some(desc) = request_body {
        op = op.request_body(Some(json_request_body(desc)));
    }
    op
}

// Card III-E6 (Wave 4 integrator): the operator execution/fleet/runner/
// profile routes used to be documented here as a hand-built `OperatorApiDoc`
// fragment with every body typed as free-form JSON (`json_operation`'s
// `json_content()`) — the reason E2/E3/E4/E5 each independently found this
// domain's `docs/openapi.json` schemas empty (`{}`). C1's handler files
// (`handlers::executions`, `handlers::runner_admin`) are no longer
// off-limits to this card (III.3: C5 for runner/execution wiring, this card
// for the Wave 4 integration boundary), so every one of their handlers now
// carries its own `#[utoipa::path(...)]` annotation referencing real,
// `ToSchema`-derived request/response DTOs, exactly like every other
// domain in this file (`handlers::orch`, `handlers::items`, …) — listed
// directly in `ApiDoc`'s `paths(...)`/`components(schemas(...))` below
// instead of through a separate nested fragment.

const RUNNER_TAG: &str = "runner-protocol-v1";
const RUNNER_PROTOCOL_NOTE: &str = "Authenticated by a hashed `Authorization: Bearer` runner \
    credential (`runner_bearer_credential` per docs/contracts/runner-v1/protocol.json) — never \
    the operator token, and never substitutable for it. Every field, limit and error shape is \
    frozen by docs/contracts/runner-v1/ (protocol.json, limits.json, lifecycle-transitions.json, \
    and this exchange's paired *.request.json/*.response.json fixtures); this document \
    deliberately does not re-specify them as a second, driftable shape.";

/// Card C2's runner-protocol v1 routes, documented relative to
/// `handlers::runner_protocol::routes` — nested at
/// `docs/contracts/runner-v1/protocol.json`'s `base_path`
/// (`/api/runner/v1`) below (see `router.rs::runner_protocol_routes`).
struct RunnerProtocolApiDoc;

impl OpenApi for RunnerProtocolApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        let attempt_id =
            || string_path_param("attempt_id", "Attempt ID, issued at claim time (opaque)");
        let op = |summary: &str, params: Vec<Parameter>, ok: &str| {
            json_operation(
                RUNNER_TAG,
                summary,
                RUNNER_PROTOCOL_NOTE,
                params,
                Some("protocol_version: 1, plus this exchange's fixture-frozen fields"),
                ok,
            )
        };
        let paths = PathsBuilder::new()
            .path(
                "/enroll",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Exchange a single-use enrollment token for a runner identity and bearer credential",
                        vec![],
                        "Runner enrolled; the raw bearer credential is returned exactly once",
                    ),
                ),
            )
            .path(
                "/refresh",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Refresh reported capabilities and optionally rotate the runner's bearer credential",
                        vec![],
                        "Capabilities accepted; a rotated credential, if requested, is returned exactly once",
                    ),
                ),
            )
            .path(
                "/claim",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Claim the next eligible execution request for this runner or its fleet",
                        vec![],
                        "A fenced lease and the immutable request snapshot, or no_eligible_work",
                    ),
                ),
            )
            .path(
                "/heartbeat",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report liveness, capacity, and active-attempt state in one fenced batch",
                        vec![],
                        "Renewed lease facts per reported attempt",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/accept",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt entering `preparing`", vec![attempt_id()], "Transition accepted or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/start",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt entering `running`", vec![attempt_id()], "Transition accepted or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/events",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Append a fenced, checkpointed batch of execution events",
                        vec![attempt_id()],
                        "Batch committed (accepted/duplicate event ids, committed checkpoint)",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/decisions",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Create a decision for later out-of-band operator resolution",
                        vec![attempt_id()],
                        "Decision recorded",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/decisions/poll",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Poll for decision resolutions since a given timestamp",
                        vec![attempt_id()],
                        "Resolved decisions since `after`, plus the new `next_after` cursor",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/artifacts",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Submit an artifact manifest (content upload/download is a separate, \
                         out-of-scope endpoint per the C2 handoff)",
                        vec![attempt_id()],
                        "Manifest accepted; per-artifact upload URLs issued",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/completion",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt's terminal outcome", vec![attempt_id()], "Completion committed or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/cancellation-observation",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report the observed effect of a requested cancellation",
                        vec![attempt_id()],
                        "Cancellation observation committed or replayed",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/recovery-observation",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report a post-restart recovery observation for an attempt (additive v1 \
                         operation; exact path fixed by protocol.json)",
                        vec![attempt_id()],
                        "Recovery observation committed or replayed; server-authoritative disposition returned",
                    ),
                ),
            );
        utoipa::openapi::OpenApi::new(Info::new("runner-protocol-v1", "1"), paths)
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tack API",
        description = "REST + WebSocket API for Tack, a lightweight, workflow-agnostic \
            project-management tool. This contract is generated from the Rust handlers \
            and domain models; it is the single source of truth for the wire format. \
            All failing responses share the `{ \"error\": { \"status\", \"message\" } }` \
            envelope, with an additional `code` field on a narrow set of responses \
            (e.g. `orchestration_disabled`) where a caller needs to branch on the \
            reason without parsing `message`.",
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
        handlers::settings::get_orch_settings,
        handlers::settings::put_orch_settings,
        // ── Orchestration (Agent-Factory Control Center, Phase 33+) ────────
        handlers::orch::create_control_plane,
        handlers::orch::list_control_planes,
        handlers::orch::get_control_plane,
        handlers::orch::update_control_plane,
        handlers::orch::delete_control_plane,
        handlers::orch::get_orch_link,
        handlers::orch::put_orch_link,
        handlers::orch::get_fleet,
        handlers::orch::get_orch_budget,
        handlers::orch::get_metrics,
        handlers::orch::get_orch_policy,
        handlers::orch::get_item_agent_activity,
        handlers::orch::get_project_agent_activity,
        handlers::orch::dispatch_item,
        handlers::orch::dispatch_sprint,
        handlers::orch::dry_run_sprint_dispatch,
        handlers::orch::list_pending_approvals,
        handlers::orch::decide_approval,
        handlers::provisioning::create_project_with_pod,
        handlers::economics::get_economics_summary,
        handlers::economics::get_economics_items,
        // ── Harness-agnostic runner fleet: operator execution API (Part III,
        // card C1; typed OpenAPI documentation wired here by card III-E6) ──
        handlers::executions::create_execution,
        handlers::executions::list_executions,
        handlers::executions::get_execution,
        handlers::executions::list_execution_attempts,
        handlers::executions::list_execution_attempt_events,
        handlers::executions::request_cancellation,
        handlers::executions::requeue_needs_operator,
        // ── Harness-agnostic runner fleet: operator fleet/runner/profile API
        // (Part III, card C1; typed OpenAPI documentation wired by III-E6) ──
        handlers::runner_admin::create_fleet,
        handlers::runner_admin::list_fleets,
        handlers::runner_admin::list_runners,
        handlers::runner_admin::revoke_runner,
        handlers::runner_admin::create_pending_runner,
        handlers::runner_admin::revoke_enrollment_token,
        handlers::runner_admin::create_profile,
        handlers::runner_admin::list_profiles,
        handlers::runner_admin::create_model_profile,
        handlers::runner_admin::list_model_profiles,
    ),
    components(schemas(
        // Local response/request envelopes
        ErrorEnvelope,
        ErrorBody,
        handlers::executions::RunnerV1ErrorEnvelope,
        handlers::executions::RunnerV1Error,
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
        handlers::settings::UpdateOrchSettings,
        handlers::orch::ControlPlaneResponse,
        handlers::orch::CapabilitiesResponse,
        handlers::orch::SupportLevel,
        handlers::orch::EventScopeLevel,
        handlers::orch::DecisionSupportLevel,
        handlers::orch::UsageSupportLevel,
        handlers::orch::ModelSelectionLevel,
        handlers::orch::SupportCapability,
        handlers::orch::EventScopeCapability,
        handlers::orch::DecisionsCapability,
        handlers::orch::UsageCapability,
        handlers::orch::ModelSelectionCapability,
        handlers::orch::CreateControlPlaneRequest,
        handlers::orch::UpdateControlPlaneRequest,
        handlers::orch::OrchLinkResponse,
        handlers::orch::OrchLinkView,
        handlers::orch::UpsertOrchLinkRequest,
        handlers::orch::StatusMap,
        handlers::orch::FleetEntry,
        handlers::orch::FleetListResponse,
        handlers::orch::FleetRosterMember,
        handlers::orch::OrchBudgetResponse,
        handlers::orch::OrchPolicyResponse,
        handlers::orch::ToolCallEntry,
        handlers::orch::PolicyHitEntry,
        handlers::orch::ApprovalChannelEntry,
        handlers::orch::ItemAgentEventResponse,
        handlers::orch::ItemAgentRunResponse,
        handlers::orch::ItemAgentAttemptResponse,
        handlers::orch::ItemAgentApprovalResponse,
        handlers::orch::ItemAgentActivityResponse,
        handlers::orch::AgentBadgeRowResponse,
        handlers::orch::AgentBadgeResponse,
        handlers::orch::DispatchedTaskResponse,
        handlers::orch::DispatchItemResponse,
        handlers::orch::SprintDispatchItemResponse,
        handlers::orch::SprintDispatchSummary,
        handlers::orch::DryRunSprintDispatchResponse,
        handlers::orch::SprintDispatchResponse,
        handlers::orch::PendingApprovalResponse,
        handlers::orch::PendingApprovalListResponse,
        handlers::orch::ApprovalDecisionAction,
        handlers::orch::DecideApprovalRequest,
        handlers::orch::DecideApprovalResponse,
        handlers::provisioning::ProvisionPodRequest,
        handlers::provisioning::CreateProjectWithPodRequest,
        handlers::provisioning::ProvisionedPodMemberResponse,
        handlers::provisioning::ProvisioningOutcome,
        handlers::provisioning::CreateProjectWithPodResponse,
        handlers::economics::LeadTimeStat,
        handlers::economics::ReworkStat,
        handlers::economics::EconomicsSlice,
        handlers::economics::EconomicsSummaryResponse,
        handlers::economics::EconomicsPopulation,
        handlers::economics::EconomicsItemResponse,
        handlers::economics::EconomicsItemsResponse,
        // ── Harness-agnostic runner fleet: operator execution API DTOs ──────
        handlers::executions::CreateExecution,
        handlers::executions::CreateExecutionResponse,
        handlers::executions::ExecutionSummary,
        handlers::executions::ExecutionListResponse,
        handlers::executions::ExecutionDetailResponse,
        handlers::executions::AttemptSummary,
        handlers::executions::AttemptListResponse,
        handlers::executions::EventSummary,
        handlers::executions::EventListResponse,
        handlers::executions::CancellationRequestedResponse,
        handlers::executions::RecoveryConfirmation,
        handlers::executions::RequeueResponse,
        // ── Harness-agnostic runner fleet: operator fleet/runner/profile API
        // DTOs ───────────────────────────────────────────────────────────
        handlers::runner_admin::CreateFleet,
        handlers::runner_admin::CreateFleetResponse,
        handlers::runner_admin::FleetSummary,
        handlers::runner_admin::FleetListResponse,
        handlers::runner_admin::RunnerSummary,
        handlers::runner_admin::RunnerListResponse,
        handlers::runner_admin::RevokeRunnerResponse,
        handlers::runner_admin::CreatePendingRunner,
        handlers::runner_admin::CreatePendingRunnerResponse,
        handlers::runner_admin::RevokeEnrollmentTokenResponse,
        handlers::runner_admin::CreateProfile,
        handlers::runner_admin::CreateProfileResponse,
        handlers::runner_admin::AgentProfileSummary,
        handlers::runner_admin::AgentProfileListResponse,
        handlers::runner_admin::CreateModelProfile,
        handlers::runner_admin::CreateModelProfileResponse,
        handlers::runner_admin::ModelProfileSummary,
        handlers::runner_admin::ModelProfileListResponse,
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
        TemplateOrchestration,
        TemplateStatusMap,
        OrchBlueprint,
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
        (name = "execution-operator", description = "Harness-agnostic runner fleet (Part III): PM-side \
            execution-request/fleet/runner-enrollment/agent-profile/model-profile management. \
            Authenticated the same way as the rest of this API (operator session or API token); scopes \
            idempotency and audit actor to the server-derived `x-tack-principal`, which a client cannot \
            set (see `crate::middleware::inject_operator_principal`)."),
        (name = "runner-protocol-v1", description = "Harness-agnostic runner fleet (Part III): the pull \
            protocol a `tack-runner` process speaks at `/api/runner/v1` (enroll, claim, heartbeat, \
            report). Authenticated by a distinct, per-runner hashed bearer credential — never the \
            operator token, and never substitutable for it \
            (docs/contracts/runner-v1/protocol.json: `credentials_are_not_substitutable`). Every wire \
            shape is frozen by docs/contracts/runner-v1/, not independently re-specified here."),
    ),
    nest(
        (path = "/api/runner/v1", api = RunnerProtocolApiDoc),
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
