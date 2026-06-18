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
    import_github, import_linear, items, projects, roles, sprints, templates, websocket,
};
use crate::middleware::require_token;
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
}

impl AppState {
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.repo.pool()
    }
}

const ATTACH_LIMIT: usize = 50 * 1024 * 1024; // 50 MB for file uploads

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
        ]))
        .max_age(Duration::from_secs(3600));

    // ── API routes ───────────────────────────────────────────────────────────
    let api = Router::new()
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
        // ─── Auth token gate (T-104) ──────────────────────────────────────
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
