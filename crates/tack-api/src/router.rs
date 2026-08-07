use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::handler::Handler;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{delete, get, patch, post, put};
use axum::{Router, middleware};
use tokio::sync::broadcast;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use tack_db::Repository;

use crate::config::AppConfig;
use crate::debug;
#[cfg(feature = "embed-spa")]
use crate::handlers::spa;
use crate::handlers::{
    alexa, attachments, backup, boards_multi, comments, custom_fields, dependencies, export,
    import_github, import_linear, items, orch, projects, provisioning, roles, settings, sprints,
    templates, websocket,
};
use crate::middleware::require_token;
use crate::orch_runtime::OrchRuntime;
use crate::webhook::WebhookClient;

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    /// Broadcast channel for real-time WebSocket updates
    pub broadcast_tx: broadcast::Sender<websocket::BoardEvent>,
    /// Optional outbound webhook client (None when TACK_WEBHOOK_URL is unset)
    pub webhook: Option<WebhookClient>,
    /// Toggleable handle to the orchestration reconciler (card E1). Cheap to
    /// clone (`Arc` underneath) — every handler gets the same live runtime,
    /// so `PUT /api/settings/orchestration` starts/stops the exact tasks
    /// `server.rs` spawned (or didn't) at boot.
    pub orch_runtime: OrchRuntime,
}

impl AppState {
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.repo.pool()
    }
}

const ATTACH_LIMIT: usize = 50 * 1024 * 1024; // 50 MB for file uploads

/// Agent-Factory Control Center routes (Phases 33–38). Every route this cycle
/// needs is batched into this one function (card A4 / TODO.md §2: `router.rs`
/// is a chokepoint file, touched once) — later-wave agents add their route to
/// the appropriate section below and update `crate::openapi::ApiDoc` rather
/// than restructuring this function or `build_router`.
///
/// The whole sub-router is gated behind the *effective* orchestration
/// setting (`app_meta`-stored value, falling back to `TACK_ORCH_ENABLE`) via
/// [`orch::require_orch_enabled`] — with orchestration disabled, every route
/// here returns `409 Conflict` with `error.code: "orchestration_disabled"`,
/// naming where to enable it (`PUT /api/settings/orchestration`), rather
/// than a bare 404 (TODO.md §0 rule 8, rewritten 2026-08-05 — card E1). The
/// auth token gate (`require_token`) is layered on top of this in
/// `build_router`, so it still applies as usual. `/api/settings/orchestration`
/// itself lives outside this sub-router (registered directly in
/// `build_router`, beside `/settings/backup`) precisely so it stays
/// reachable while the feature is off — see that route's own comment.
fn orch_routes(state: AppState) -> Router<AppState> {
    Router::new()
        // ─── Control planes (Wave 1 / A4, 33.5) ───────────────────────────
        .route(
            "/control-planes",
            post(orch::create_control_plane).get(orch::list_control_planes),
        )
        .route(
            "/control-planes/{id}",
            get(orch::get_control_plane)
                .patch(orch::update_control_plane)
                .delete(orch::delete_control_plane),
        )
        // ─── Project ↔ control-plane link (Wave 1 / A4, 33.5) ─────────────
        .route(
            "/projects/{id}/orch-link",
            get(orch::get_orch_link).put(orch::put_orch_link),
        )
        // ─── Fleet view aggregate (Wave 1 / A4, 33.5) ──────────────────────
        .route("/fleet", get(orch::get_fleet))
        // ─── Wave 2 (Phase 34) — metrics ────────────────────────────────────
        .route("/metrics", get(orch::get_metrics)) // B3, 34.3/34.7
        // ─── Wave 2 (Phase 34) — item/project agent activity ───────────────
        .route(
            "/items/{id}/agent-activity",
            get(orch::get_item_agent_activity),
        ) // B6, 34.8/34.9
        .route(
            "/projects/{id}/agent-activity",
            get(orch::get_project_agent_activity),
        ) // B6, 34.8/34.9
        // ─── Wave 3 (Phase 35) — dispatch ───────────────────────────────────
        .route("/items/{id}/dispatch", post(orch::dispatch_item)) // C1, 35.2/35.3/35.6
        .route("/sprints/{id}/dispatch", post(orch::dispatch_sprint)) // C3, 35.4
        .route(
            "/sprints/{id}/dispatch/dry-run",
            get(orch::dry_run_sprint_dispatch),
        ) // C3, 35.4
        // ─── Wave 4 (Phases 36–38) — approvals + provisioning ──────────────
        .route("/approvals", get(orch::list_pending_approvals)) // D1, 36.1 — fleet-wide inbox, read-only
        .route("/approvals/{token}", post(orch::decide_approval)) // D1, 36.1 — also gated on TACK_ORCH_APPROVAL_TOKEN (checked inside the handler, not this layer)
        .route("/projects/{id}/orch-budget", get(orch::get_orch_budget)) // D2, 36.3 — budget cap vs. mirrored spend
        .route("/projects/{id}/orch-policy", get(orch::get_orch_policy)) // D2, 36.4 — guardrail/tool-call/approval metrics (control-plane-wide)
        .route(
            "/templates/{id}/provision",
            post(provisioning::create_project_with_pod),
        ) // D4, 37.2 — provision a pod + create/link a Tack project from a template, rollback-on-failure (see handlers/provisioning.rs's module doc for why this is a separate route rather than a `provision_pod:true` extension of the existing endpoint)
        .merge(crate::handlers::economics::economics_routes()) // D5, 38.1-38.4 — unit economics summary + per-item export; see handlers/economics.rs
        .layer(middleware::from_fn_with_state(
            state,
            orch::require_orch_enabled,
        ))
}

/// Build the full Axum router with all routes, middleware, and state.
pub fn build_router(state: AppState) -> Router {
    // ── CORS ─────────────────────────────────────────────────────────────────
    let allowed_origins: Vec<HeaderValue> = state
        .config
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            // `If-Match` — card G3's optimistic-concurrency precondition on
            // items/orch-links/control-planes PATCH/PUT. Without this, any
            // cross-origin browser client (anything through
            // `TACK_ALLOWED_ORIGINS` that isn't same-origin `embed-spa`)
            // fails preflight on every conditional write and silently falls
            // back to unconditional last-write-wins.
            header::IF_MATCH,
            // `X-Tack-Approval-Token` — pre-existing bug, not something this
            // cycle introduced: `frontend/src/features/approvals/api.ts`
            // sends this on every grant/deny and it has only ever worked
            // because production is same-origin via `embed-spa`. Reusing
            // the handler's own constant (rather than a hand-copied
            // literal) so this list can't drift from the header
            // `handlers::orch::decide_approval` actually reads.
            header::HeaderName::from_static(orch::APPROVAL_TOKEN_HEADER),
        ]))
        // No `expose_headers` call existed before this card — a browser
        // could read zero non-safelisted response headers from this API.
        // `ETag` is the first one anything needs: cards G1/G3 add it to
        // `GET` responses so a client can send it back as `If-Match`, and
        // an unexposed response header is invisible to `fetch()`/`XHR`
        // regardless of what the server sends on the wire.
        .expose_headers([header::ETAG])
        .max_age(Duration::from_secs(3600));

    // ── API routes ───────────────────────────────────────────────────────────
    let api = Router::new()
        // ─── OpenAPI contract (public — no auth to read the schema) ──────
        .route("/openapi.json", get(crate::openapi::openapi_json))
        // ─── Health & Debug (always public) ──────────────────────────────
        .route("/health", get(debug::health))
        .route("/debug/info", get(debug::debug_info))
        .route("/debug/db-stats", get(debug::db_stats))
        // ─── Backup / restore ────────────────────────────────────────────
        .route("/backup", get(backup::get_backup))
        .route(
            "/restore",
            post(backup::post_restore.layer(DefaultBodyLimit::max(ATTACH_LIMIT))),
        )
        // ─── Remote cloud backup ──────────────────────────────────────────
        .route("/backup/remote", post(backup::post_remote_backup))
        .route("/backup/remote", get(backup::get_remote_backups))
        .route("/backup/remote/restore", post(backup::post_remote_restore))
        .route("/backup/remote/verify", post(backup::post_remote_verify))
        // ─── Cloud backup settings (UI-editable) ──────────────────────────
        .route(
            "/settings/backup",
            get(settings::get_backup_settings).put(settings::put_backup_settings),
        )
        // ─── Orchestration settings (UI-editable; card E1) ─────────────────
        // Deliberately **outside** `orch_routes`'/`require_orch_enabled`'s
        // gate: this is the one orchestration-adjacent endpoint that must
        // stay reachable while orchestration is off — it's how an operator
        // discovers the feature exists and turns it on. See this file's
        // `orch_routes` doc comment and TODO.md §0 rule 8.
        .route(
            "/settings/orchestration",
            get(settings::get_orch_settings).put(settings::put_orch_settings),
        )
        // ─── Projects ────────────────────────────────────────────────────
        .route("/projects", post(projects::create_project))
        .route("/projects", get(projects::list_projects))
        .route("/projects/{id}", get(projects::get_project))
        .route("/projects/{id}", patch(projects::update_project))
        .route("/projects/{id}", delete(projects::delete_project))
        // ─── Export/Import ───────────────────────────────────────────────
        .route("/projects/{id}/export", get(export::export_project))
        .route("/projects/import", post(export::import_project))
        .route("/projects/{id}/import-csv", post(export::import_csv))
        .route(
            "/projects/{id}/import-github",
            post(import_github::import_github),
        )
        .route(
            "/projects/{id}/import-linear",
            post(import_linear::import_linear),
        )
        // ─── Items ───────────────────────────────────────────────────────
        .route("/projects/{project_id}/items", post(items::create_item))
        .route("/projects/{project_id}/items", get(items::list_items))
        .route(
            "/projects/{project_id}/items/tree",
            get(items::get_item_tree),
        )
        .route("/projects/{project_id}/search", get(items::search_items))
        .route("/search", get(items::search_items_global))
        .route("/items/{id}", get(items::get_item))
        .route("/items/{id}", patch(items::update_item))
        .route("/items/{id}", delete(items::delete_item))
        // ─── Sprints ─────────────────────────────────────────────────────
        .route(
            "/projects/{project_id}/sprints",
            post(sprints::create_sprint),
        )
        .route("/projects/{project_id}/sprints", get(sprints::list_sprints))
        .route("/sprints/{id}", get(sprints::get_sprint))
        .route("/sprints/{id}/status", patch(sprints::update_sprint_status))
        // ─── Roles ───────────────────────────────────────────────────────
        .route("/projects/{project_id}/roles", post(roles::create_role))
        .route("/projects/{project_id}/roles", get(roles::list_roles))
        .route("/roles/{id}", delete(roles::delete_role))
        .route("/items/{item_id}/roles/{role_id}", put(roles::assign_role))
        .route(
            "/items/{item_id}/roles/{role_id}",
            delete(roles::remove_role),
        )
        // ─── Comments ────────────────────────────────────────────────────
        .route("/items/{item_id}/comments", post(comments::create_comment))
        .route("/items/{item_id}/comments", get(comments::list_comments))
        // ─── Dependencies ────────────────────────────────────────────────
        .route(
            "/items/{item_id}/dependencies",
            post(dependencies::create_dependency),
        )
        .route(
            "/items/{item_id}/dependencies",
            get(dependencies::list_dependencies),
        )
        .route(
            "/items/{item_id}/dependencies/{dep_id}",
            delete(dependencies::delete_dependency),
        )
        // ─── Attachments (upload has its own higher body limit) ──────────
        .route(
            "/items/{item_id}/attachments",
            post(attachments::upload_attachment.layer(DefaultBodyLimit::max(ATTACH_LIMIT)))
                .get(attachments::list_attachments),
        )
        .route("/attachments/{id}", get(attachments::download_attachment))
        .route("/attachments/{id}", delete(attachments::delete_attachment))
        // ─── Project Templates ───────────────────────────────────────────
        .route("/templates", post(templates::create_template))
        .route("/templates", get(templates::list_templates))
        .route("/templates/{id}", get(templates::get_template))
        .route("/templates/{id}", delete(templates::delete_template))
        .route(
            "/projects/from-template/{id}",
            post(templates::create_project_from_template),
        )
        .route(
            "/projects/{id}/save-as-template",
            post(templates::save_project_as_template),
        )
        // ─── Custom Fields ───────────────────────────────────────────────
        .route(
            "/projects/{project_id}/custom-fields",
            post(custom_fields::create_field),
        )
        .route(
            "/projects/{project_id}/custom-fields",
            get(custom_fields::list_fields),
        )
        .route("/custom-fields/{id}", get(custom_fields::get_field))
        .route("/custom-fields/{id}", patch(custom_fields::update_field))
        .route("/custom-fields/{id}", delete(custom_fields::delete_field))
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            put(custom_fields::set_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            get(custom_fields::get_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields/{field_id}",
            delete(custom_fields::delete_field_value),
        )
        .route(
            "/items/{item_id}/custom-fields",
            get(custom_fields::get_all_field_values),
        )
        // ─── Multiple Boards ─────────────────────────────────────────────
        .route(
            "/projects/{project_id}/boards",
            post(boards_multi::create_board),
        )
        .route(
            "/projects/{project_id}/boards",
            get(boards_multi::list_boards),
        )
        .route("/projects/{id}/boards/live", get(websocket::board_live))
        .route("/boards/{id}", get(boards_multi::get_board))
        .route("/boards/{id}", patch(boards_multi::update_board))
        .route("/boards/{id}", delete(boards_multi::delete_board))
        .route("/boards/{id}/view", get(boards_multi::get_board_view))
        // ─── Alexa voice integration (skill-ID auth, exempt from token) ──
        .route("/alexa", post(alexa::handle_request))
        // ─── Agent-Factory Control Center (Phase 33+, gated) ──────────────
        // Every route this cycle needs is batched into this one sub-router so
        // router.rs is structurally touched once (TODO.md §2's chokepoint
        // note); later waves add their route to `orch_routes` below rather
        // than restructuring this file. `require_orch_enabled` returns a 409
        // with `error.code: "orchestration_disabled"` for every route here
        // while orchestration is off (TODO.md §0 rule 8, card E1).
        .merge(orch_routes(state.clone()))
        // ─── Auth token gate ──────────────────────────────────────
        .layer(middleware::from_fn_with_state(state.clone(), require_token));

    let outer = Router::new().nest("/api", api);

    #[cfg(feature = "embed-spa")]
    let outer = outer.fallback(spa::serve_spa);

    outer
        // ── Global body limit (attachments route overrides above) ────────
        .layer(DefaultBodyLimit::max(state.config.max_body_size_bytes))
        // ── Minimal security response headers ────────────────────────────
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        // ── CORS ─────────────────────────────────────────────────────────
        .layer(cors)
        // ── Request tracing ──────────────────────────────────────────────
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                )
            }),
        )
        .with_state(state)
}
