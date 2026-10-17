use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tracing::{info, instrument};
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{
    CreateDependency, CreateItem, CreateSprint, Dependency, Item, ItemBrief, ItemSource, Project,
    Sprint, UpdateProject, UpsertItemBrief,
};
use tack_core::outline::bundle::{Parts, to_bundle};
use tack_core::outline::{BundleAutoStart, BundleAutonomy, BundleBudgets, BundleDomain};
use tack_db::repo::autonomy as store;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    #[serde(default = "default_format")]
    format: String,
    /// `snapshot` (the default) is the database dump `POST /api/projects/import`
    /// reads; `bundle` is the project file of ADR 0081 (JSON or YAML).
    scope: Option<String>,
}

fn default_format() -> String {
    "json".to_string()
}

/// GET /api/projects/:id/export
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/export",
    tag = "export",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ExportQuery,
    ),
    responses(
        (status = 200, description = "Export file (JSON, YAML, or CSV per the `format` query)"),
        (status = 400, description = "Unsupported format", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn export_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    info!(project_id = %project_id, format = %query.format, "Exporting project");
    let bundle = match query.scope.as_deref() {
        None | Some("snapshot") => false,
        Some("bundle") => true,
        Some(other) => {
            return Err(ApiError::BadRequest(format!(
                "Unsupported scope '{other}'. Use 'snapshot' or 'bundle'"
            )));
        }
    };
    if bundle && !matches!(query.format.as_str(), "json" | "yaml") {
        return Err(ApiError::BadRequest(
            "A project file is JSON or YAML; use format 'json' or 'yaml' with scope 'bundle'"
                .into(),
        ));
    }

    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let items = state
        .repo
        .list_items(project_id, &Default::default())
        .await?;
    if bundle {
        return bundle_response(&state, &project, &items, &query.format).await;
    }
    let sprints = state.repo.list_sprints(project_id).await?;
    let dependencies = state.repo.list_dependencies_for_project(project_id).await?;
    let briefs = state.repo.list_item_briefs_for_project(project_id).await?;
    let brief_versions = state
        .repo
        .list_item_brief_versions_for_project(project_id)
        .await?;

    // Shared snapshot used by the structured (JSON / YAML) formats. Keys serialize
    // in a stable order, so YAML/JSON exports produce clean, git-diffable text.
    let export_data = serde_json::json!({
        "project": project,
        "items": items,
        "sprints": sprints,
        "dependencies": dependencies,
        "briefs": briefs,
        "brief_versions": brief_versions,
        "metadata": {
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION"),
            "total_items": items.len(),
            "total_sprints": sprints.len(),
            "total_dependencies": dependencies.len(),
        }
    });

    match query.format.as_str() {
        "json" => {
            let json_data = serde_json::to_string_pretty(&export_data)
                .map_err(|e| anyhow::anyhow!("Failed to serialize: {}", e))?;

            let filename = format!("{}-export.json", project.name.replace(' ', "-"));

            Ok((
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                json_data,
            )
                .into_response())
        }
        "yaml" => {
            let yaml_data = serde_saphyr::to_string(&export_data)
                .map_err(|e| anyhow::anyhow!("Failed to serialize YAML: {}", e))?;

            let filename = format!("{}-export.yaml", project.name.replace(' ', "-"));

            Ok((
                [
                    (header::CONTENT_TYPE, "application/x-yaml"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                yaml_data,
            )
                .into_response())
        }
        "csv" => {
            let csv_output = tack_core::outline::items_to_csv(&items);

            let filename = format!("{}-export.csv", project.name.replace(' ', "-"));

            Ok((
                [
                    (header::CONTENT_TYPE, "text/csv"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                csv_output,
            )
                .into_response())
        }
        _ => Err(ApiError::BadRequest(format!(
            "Unsupported format '{}'. Use 'json', 'yaml', or 'csv'",
            query.format
        ))),
    }
}

/// The project as one project file (ADR 0081), written the way `import` reads it.
async fn bundle_response(
    state: &AppState,
    project: &Project,
    items: &[Item],
    format: &str,
) -> Result<Response, ApiError> {
    let project_id = project.id;
    let fields = state.repo.list_fields_for_project(project_id).await?;
    let profiles = state.repo.list_agent_profile_rows().await?;
    let roles = state.repo.list_roles(project_id).await?;
    let mut sprints = state.repo.list_sprints(project_id).await?;
    // The list is newest first; a file reads in the order the sprints run.
    sprints.sort_by(|a, b| (a.start_date, &a.name).cmp(&(b.start_date, &b.name)));
    let item_roles = state.repo.item_roles_for_project(project_id).await?;
    let item_fields = state.repo.field_values_for_project(project_id).await?;
    let dependencies = state.repo.list_dependencies_for_project(project_id).await?;
    let fleets = state.repo.fleet_names().await?;
    let settings = store::get_settings(state.repo.pool(), &project_id.to_string()).await?;
    let autonomy = settings.map(|s| BundleAutonomy {
        level: Some(s.level.to_string()),
        domains: s
            .domains
            .into_iter()
            .map(|d| BundleDomain {
                name: d.name,
                paths: d.path_globs,
                item_types: d.item_types,
                level: d.level.to_string(),
            })
            .collect(),
        verify_require: Some(s.verify_require),
        verify_loop_max_attempts: Some(s.verify_loop_max_attempts),
        budgets: Some(BundleBudgets {
            tokens_per_day: s.budgets.tokens_per_day,
            usd_per_day: s.budgets.usd_per_day,
            auto_merges_per_day: s.budgets.auto_merges_per_day,
            queue_items_max: s.budgets.queue_items_max,
        }),
        merge_on_done: Some(s.merge_on_done),
    });
    let outline = to_bundle(&Parts {
        project,
        fields: &fields,
        profiles: &profiles,
        roles: &roles,
        sprints: &sprints,
        autonomy,
        auto_start: BundleAutoStart {
            enabled: project.auto_start_enabled,
            status: project.auto_start_status.clone(),
            max_open: Some(project.auto_start_max_open),
        },
        items,
        item_roles: &item_roles,
        item_fields: &item_fields,
        dependencies: &dependencies,
        fleets: &fleets,
    });
    let name = project.name.replace(' ', "-");
    let (content_type, extension, text) = if format == "yaml" {
        (
            "application/x-yaml",
            "yaml",
            serde_saphyr::to_string(&outline)
                .map_err(|e| anyhow::anyhow!("Failed to serialize YAML: {}", e))?,
        )
    } else {
        (
            "application/json",
            "json",
            serde_json::to_string_pretty(&outline)
                .map_err(|e| anyhow::anyhow!("Failed to serialize: {}", e))?,
        )
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{name}-bundle.{extension}\""),
            ),
        ],
        text,
    )
        .into_response())
}

// ─── Import ──────────────────────────────────────────────────────────────────

/// A JSON body, or YAML when the `Content-Type` mentions yaml, as one
/// `serde_json::Value` (YAML is a JSON superset, so both decode alike).
/// `what` names the body in the error, e.g. "outline".
pub(crate) fn structured_body(
    headers: &HeaderMap,
    body: &[u8],
    what: &str,
) -> Result<serde_json::Value, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if !matches!(
        media_type,
        "application/json" | "application/yaml" | "application/x-yaml" | "text/yaml"
    ) {
        return Err(ApiError::UnsupportedMediaType(format!(
            "{what} must be sent as application/json, application/yaml, application/x-yaml or text/yaml"
        )));
    }
    if media_type != "application/json" {
        serde_saphyr::from_slice(body)
            .map_err(|e| ApiError::BadRequest(format!("Invalid YAML {what}: {e}")))
    } else {
        serde_json::from_slice(body)
            .map_err(|e| ApiError::BadRequest(format!("Invalid JSON {what}: {e}")))
    }
}

#[derive(Debug, Deserialize)]
struct ImportPayload {
    project: Project,
    items: Vec<Item>,
    sprints: Vec<Sprint>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
    #[serde(default)]
    briefs: Vec<ItemBrief>,
    #[serde(default)]
    brief_versions: Vec<tack_core::brief::ItemBriefVersion>,
}

/// POST /api/projects/import
///
/// Accepts the same snapshot shape produced by `export?format=json|yaml`. The
/// body is parsed as YAML when the `Content-Type` mentions YAML, otherwise JSON —
/// so a YAML export round-trips back in unchanged. (YAML is a JSON superset, so
/// both decode into the same intermediate `serde_json::Value`.)
#[instrument(skip(state, body))]
#[utoipa::path(
    post,
    path = "/api/projects/import",
    tag = "import",
    request_body(content = serde_json::Value, description = "Project snapshot (JSON, or YAML when Content-Type mentions yaml) as produced by export"),
    responses(
        (status = 200, description = "Import result with the new project and stats", body = serde_json::Value),
        (status = 400, description = "Invalid import payload", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn import_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Import endpoint called");

    let raw = structured_body(&headers, &body, "import payload")?;

    let payload: ImportPayload = serde_json::from_value(raw)
        .map_err(|e| ApiError::BadRequest(format!("Invalid import payload: {e}")))?;

    let src = &payload.project;

    let new_project = state
        .repo
        .create_project(
            state.workspace_id,
            tack_core::models::CreateProject {
                name: src.name.clone(),
                description: src.description.clone(),
                project_type: src.project_type.clone(),
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;

    match run_import(&state, new_project.id, &payload).await {
        Err(e) => {
            let _ = state.repo.delete_project(new_project.id).await;
            Err(ApiError::Internal(e))
        }
        Ok(stats) => {
            // Restore original workflow + vocabulary
            let _ = state
                .repo
                .update_project(
                    new_project.id,
                    UpdateProject {
                        name: None,
                        description: None,
                        workflow: Some(src.workflow.clone()),
                        vocabulary: Some(src.vocabulary.clone()),
                        default_model: None,
                        github_token_ref: None,
                        archived: None,
                        ..Default::default()
                    },
                )
                .await;

            let final_project = state
                .repo
                .get_project(new_project.id)
                .await?
                .ok_or_else(|| {
                    ApiError::Internal(anyhow::anyhow!("project missing after import"))
                })?;

            Ok(Json(serde_json::json!({
                "success": true,
                "project": final_project,
                "stats": stats,
            })))
        }
    }
}

async fn run_import(
    state: &AppState,
    new_project_id: Uuid,
    data: &ImportPayload,
) -> anyhow::Result<serde_json::Value> {
    // ── Sprints ──────────────────────────────────────────────────────────────
    let mut sprint_id_map: HashMap<Uuid, Uuid> = HashMap::new();

    for sprint in &data.sprints {
        let new = state
            .repo
            .create_sprint(
                new_project_id,
                CreateSprint {
                    name: sprint.name.clone(),
                    goal: sprint.goal.clone(),
                    start_date: sprint.start_date,
                    end_date: sprint.end_date,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        sprint_id_map.insert(sprint.id, new.id);
    }

    // ── Items pass 1: create without parent_id ────────────────────────────
    let mut item_id_map: HashMap<Uuid, Uuid> = HashMap::new();

    for item in &data.items {
        // Preserve the imported item's own `source` rather than defaulting to
        // `Manual` — this is what makes the trust marker survive an
        // export → import round-trip. An
        // export produced before this field existed, or any hand-built
        // payload that omits it, deserializes `item.source` to
        // `ItemSource::Unknown` (untrusted) via `Item`'s `#[serde(default)]`,
        // never to `Manual` — so a payload can't claim trust it never had
        // just by leaving the field out. A payload that claims `manual` is
        // an import like any other and lands as `json_import`.
        let source = if item.source == ItemSource::Manual {
            ItemSource::JsonImport
        } else {
            item.source.clone()
        };
        let new = state
            .repo
            .create_item_with_source(
                new_project_id,
                &item.status,
                CreateItem {
                    title: item.title.clone(),
                    description: item.description.clone(),
                    item_type: Some(item.item_type.clone()),
                    parent_id: None,
                    priority: Some(item.priority.clone()),
                    estimate: item.estimate,
                    estimate_unit: Some(item.estimate_unit.clone()),
                    tags: Some(item.tags.clone()),
                    due_date: item.due_date,
                    sprint_id: item.sprint_id.and_then(|s| sprint_id_map.get(&s).copied()),
                    assignee: item.assignee.clone(),
                    recurrence: item.recurrence,
                    recurrence_of: None,
                    source_artifact_id: item.source_artifact_id.clone(),
                },
                source,
            )
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        item_id_map.insert(item.id, new.id);
    }

    // ── Items pass 2: wire up parent_ids ────────────────────────────────────
    for item in &data.items {
        if let Some(old_parent) = item.parent_id
            && let (Some(&new_id), Some(&new_parent)) =
                (item_id_map.get(&item.id), item_id_map.get(&old_parent))
        {
            sqlx::query("UPDATE items SET parent_id = ? WHERE id = ?")
                .bind(new_parent.to_string())
                .bind(new_id.to_string())
                .execute(state.repo.pool())
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
        }
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    let mut deps_imported = 0usize;
    for dep in &data.dependencies {
        if let (Some(&new_src), Some(&new_tgt)) = (
            item_id_map.get(&dep.source_item_id),
            item_id_map.get(&dep.target_item_id),
        ) && state
            .repo
            .create_dependency(
                new_src,
                CreateDependency {
                    target_item_id: new_tgt,
                    dependency_type: dep.dependency_type.clone(),
                },
            )
            .await
            .is_ok()
        {
            deps_imported += 1;
        }
    }

    // ── Briefs ───────────────────────────────────────────────────────────────
    let mut briefs_imported = 0usize;
    for brief in &data.briefs {
        if let Some(&new_item) = item_id_map.get(&brief.item_id) {
            let upsert = UpsertItemBrief {
                acceptance: brief.acceptance.clone(),
                constraints: brief.constraints.clone(),
                definition_of_done: brief.definition_of_done.clone(),
                risk: brief.risk.clone(),
            };
            // An export is a file anyone can edit; the same rules as PUT apply.
            upsert
                .validate()
                .map_err(|e| anyhow::anyhow!("brief of item {}: {e}", brief.item_id))?;
            state
                .repo
                .upsert_item_brief(new_item, upsert)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            // An export from before versions were kept has none: the upsert's own stands.
            let mut versions: Vec<_> = data
                .brief_versions
                .iter()
                .filter(|v| v.item_id == brief.item_id)
                .cloned()
                .collect();
            if !versions.is_empty() {
                for version in &mut versions {
                    version.item_id = new_item;
                }
                state
                    .repo
                    .restore_item_brief_versions(new_item, &versions)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
            briefs_imported += 1;
        }
    }

    Ok(serde_json::json!({
        "sprints_imported": data.sprints.len(),
        "items_imported": data.items.len(),
        "dependencies_imported": deps_imported,
        "briefs_imported": briefs_imported,
    }))
}
