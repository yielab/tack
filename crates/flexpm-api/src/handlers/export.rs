use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tracing::{info, instrument};
use uuid::Uuid;

use flexpm_core::models::{
    CreateDependency, CreateItem, CreateSprint, Dependency, Item, ItemType, Priority, Project,
    Sprint, UpdateProject,
};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "json".to_string()
}

/// GET /api/projects/:id/export
#[instrument(skip(state))]
pub async fn export_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    info!(project_id = %project_id, format = %query.format, "Exporting project");

    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let items = state
        .repo
        .list_items(project_id, &Default::default())
        .await?;
    let sprints = state.repo.list_sprints(project_id).await?;
    let dependencies = state.repo.list_dependencies_for_project(project_id).await?;

    match query.format.as_str() {
        "json" => {
            let export_data = serde_json::json!({
                "project": project,
                "items": items,
                "sprints": sprints,
                "dependencies": dependencies,
                "metadata": {
                    "exported_at": chrono::Utc::now().to_rfc3339(),
                    "version": env!("CARGO_PKG_VERSION"),
                    "total_items": items.len(),
                    "total_sprints": sprints.len(),
                    "total_dependencies": dependencies.len(),
                }
            });

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
        "csv" => {
            let mut csv_output =
                String::from("id,title,type,status,priority,assignee,parent_id,created_at\n");

            for item in &items {
                csv_output.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    item.id,
                    item.title.replace(',', " "),
                    item.item_type,
                    item.status,
                    item.priority,
                    item.assignee.as_deref().unwrap_or(""),
                    item.parent_id.map(|id| id.to_string()).unwrap_or_default(),
                    item.created_at.to_rfc3339(),
                ));
            }

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
            "Unsupported format '{}'. Use 'json' or 'csv'",
            query.format
        ))),
    }
}

// ─── Import ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ImportPayload {
    project: Project,
    items: Vec<Item>,
    sprints: Vec<Sprint>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

/// POST /api/projects/import
#[instrument(skip(state))]
pub async fn import_project(
    State(state): State<AppState>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Import endpoint called");

    let payload: ImportPayload = serde_json::from_value(raw)
        .map_err(|e| ApiError::BadRequest(format!("Invalid import payload: {e}")))?;

    let src = &payload.project;

    let new_project = state
        .repo
        .create_project(
            state.workspace_id,
            flexpm_core::models::CreateProject {
                name: src.name.clone(),
                description: src.description.clone(),
                project_type: src.project_type.clone(),
                template: None,
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
                        archived: None,
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
        let new = state
            .repo
            .create_item(
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
                },
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

    Ok(serde_json::json!({
        "sprints_imported": data.sprints.len(),
        "items_imported": data.items.len(),
        "dependencies_imported": deps_imported,
    }))
}

// ─── CSV import ───────────────────────────────────────────────────────────────

/// POST /api/projects/:id/import-csv
///
/// Accepts a UTF-8 CSV body (Content-Type: text/csv or text/plain).
/// First row must be column headers.  Only `title` is required; all other
/// columns are optional.  Recognised headers (case-insensitive):
///   title, description, type, item_type, status, priority, assignee, estimate
///
/// Returns `{ "created": N, "skipped": N }`.
#[instrument(skip(state, body))]
pub async fn import_csv(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let text = std::str::from_utf8(&body)
        .map_err(|_| ApiError::BadRequest("CSV body is not valid UTF-8".into()))?;

    // Resolve project to get its default (first) workflow status.
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let default_status = project
        .workflow
        .statuses
        .iter()
        .min_by_key(|s| s.order)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "To Do".to_string());

    let mut lines = text.lines();

    let header_line = lines
        .next()
        .ok_or_else(|| ApiError::BadRequest("CSV has no header row".into()))?;

    let headers: Vec<String> = split_csv_row(header_line)
        .into_iter()
        .map(|h| h.trim().to_lowercase())
        .collect();

    let col = |name: &str| headers.iter().position(|h| h == name);

    let title_col = col("title")
        .ok_or_else(|| ApiError::BadRequest("CSV must have a 'title' column".into()))?;

    let desc_col    = col("description");
    let type_col    = col("type").or_else(|| col("item_type"));
    let status_col  = col("status");
    let prio_col    = col("priority");
    let assign_col  = col("assignee");
    let est_col     = col("estimate");

    let mut created = 0usize;
    let mut skipped = 0usize;

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_csv_row(line);
        let get = |idx: Option<usize>| -> Option<&str> {
            idx.and_then(|i| fields.get(i)).map(|s| s.trim()).filter(|s| !s.is_empty())
        };

        let title = match get(Some(title_col)) {
            Some(t) => t.to_string(),
            None => { skipped += 1; continue; }
        };

        let status = status_col
            .and_then(|i| get(Some(i)))
            .map(|s| {
                // Accept any status string; validate against workflow, fall back to default.
                if project.workflow.statuses.iter().any(|ws| ws.name.eq_ignore_ascii_case(s)) {
                    // Find the correctly-cased name.
                    project.workflow.statuses.iter()
                        .find(|ws| ws.name.eq_ignore_ascii_case(s))
                        .map(|ws| ws.name.clone())
                        .unwrap_or_else(|| default_status.clone())
                } else {
                    default_status.clone()
                }
            })
            .unwrap_or_else(|| default_status.clone());

        let item_type: Option<ItemType> = get(type_col).and_then(|s| parse_item_type(s));

        let priority: Option<Priority> = get(prio_col).and_then(|s| match s.to_lowercase().as_str() {
            "critical" => Some(Priority::Critical),
            "high"     => Some(Priority::High),
            "medium"   => Some(Priority::Medium),
            "low"      => Some(Priority::Low),
            "none"     => Some(Priority::None),
            _          => None,
        });

        let estimate: Option<f64> = get(est_col).and_then(|s| s.parse().ok());

        let data = CreateItem {
            title,
            description: get(desc_col).map(|s| s.to_string()),
            item_type,
            parent_id: None,
            priority,
            estimate,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
            assignee: get(assign_col).map(|s| s.to_string()),
        };

        match state.repo.create_item(project_id, &status, data).await {
            Ok(_) => created += 1,
            Err(e) => {
                tracing::warn!(error = %e, "Skipped CSV row due to create_item error");
                skipped += 1;
            }
        }
    }

    info!(project_id = %project_id, created, skipped, "CSV import complete");
    Ok(Json(serde_json::json!({ "created": created, "skipped": skipped })))
}

/// Split a single CSV row respecting double-quoted fields (RFC 4180 subset).
fn split_csv_row(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    // Escaped quote inside a quoted field.
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    fields.push(current);
    fields
}

fn parse_item_type(s: &str) -> Option<ItemType> {
    match s.to_lowercase().as_str() {
        "epic"        => Some(ItemType::Epic),
        "feature"     => Some(ItemType::Feature),
        "task"        => Some(ItemType::Task),
        "subtask"     => Some(ItemType::Subtask),
        "bug"         => Some(ItemType::Bug),
        "requirement" => Some(ItemType::Requirement),
        other if !other.is_empty() => Some(ItemType::Custom(other.to_string())),
        _ => None,
    }
}
