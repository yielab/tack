use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// POST /api/items/:id/attachments
///
/// Upload a file attachment to an item.
/// Accepts multipart/form-data with a "file" field.
#[instrument(skip(state, multipart))]
pub async fn upload_attachment(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!(item_id = %item_id, "Uploading attachment");

    // Verify item exists
    let item = state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {} not found", item_id)))?;

    debug!(item_id = %item_id, item_title = %item.title, "Item found");

    // Process multipart form data
    let mut filename = None;
    let mut content_type = None;
    let mut file_data = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());

            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("Failed to read file data: {}", e)))?;

            file_data = Some(data);
        }
    }

    let filename = filename.ok_or_else(|| ApiError::BadRequest("No file provided".to_string()))?;
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let file_data =
        file_data.ok_or_else(|| ApiError::BadRequest("No file data provided".to_string()))?;

    let size_bytes = file_data.len() as u64;

    debug!(
        filename = %filename,
        content_type = %content_type,
        size_bytes = size_bytes,
        "File details parsed"
    );

    // Validate file size (max 50MB)
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
    if size_bytes > MAX_FILE_SIZE {
        return Err(ApiError::BadRequest(format!(
            "File too large: {} bytes (max: {} bytes)",
            size_bytes, MAX_FILE_SIZE
        )));
    }

    // Create storage directory if it doesn't exist
    let storage_base = PathBuf::from(&state.config.storage_dir);
    let item_storage = storage_base.join(item_id.to_string());

    std::fs::create_dir_all(&item_storage).map_err(|e| {
        warn!(error = %e, "Failed to create storage directory");
        anyhow::anyhow!("Failed to create storage directory: {}", e)
    })?;

    // Generate unique filename to avoid collisions
    let file_id = Uuid::new_v4();
    let file_ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let storage_filename = if file_ext.is_empty() {
        format!("{}", file_id)
    } else {
        format!("{}.{}", file_id, file_ext)
    };

    let storage_path = item_storage.join(&storage_filename);

    // Write file to disk
    let mut file = std::fs::File::create(&storage_path).map_err(|e| {
        warn!(error = %e, path = ?storage_path, "Failed to create file");
        anyhow::anyhow!("Failed to create file: {}", e)
    })?;

    file.write_all(&file_data).map_err(|e| {
        warn!(error = %e, "Failed to write file data");
        anyhow::anyhow!("Failed to write file: {}", e)
    })?;

    debug!(path = ?storage_path, "File written to disk");

    // Store relative path for portability
    let relative_path = format!("{}/{}", item_id, storage_filename);

    // Create attachment record in database
    use tack_db::repo::attachments::AttachmentRepo;
    let attachment_repo = AttachmentRepo::new(state.repo.pool());

    let attachment = attachment_repo
        .create_attachment(item_id, filename, content_type, relative_path, size_bytes)
        .await?;

    info!(
        attachment_id = %attachment.id,
        filename = %attachment.filename,
        size_bytes = attachment.size_bytes,
        "Attachment uploaded successfully"
    );

    Ok(Json(serde_json::to_value(attachment).unwrap()))
}

/// GET /api/attachments/:id
///
/// Download an attachment file.
#[instrument(skip(state))]
pub async fn download_attachment(
    State(state): State<AppState>,
    Path(attachment_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    info!(attachment_id = %attachment_id, "Downloading attachment");

    // Get attachment record
    use tack_db::repo::attachments::AttachmentRepo;
    let attachment_repo = AttachmentRepo::new(state.repo.pool());

    let attachment = attachment_repo
        .get_attachment(attachment_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Attachment {} not found", attachment_id)))?;

    debug!(
        filename = %attachment.filename,
        storage_path = %attachment.storage_path,
        "Attachment record found"
    );

    // Read file from disk
    let storage_base = PathBuf::from(&state.config.storage_dir);
    let file_path = storage_base.join(&attachment.storage_path);

    let file_data = std::fs::read(&file_path).map_err(|e| {
        warn!(error = %e, path = ?file_path, "Failed to read file");
        anyhow::anyhow!("Failed to read file: {}", e)
    })?;

    info!(
        attachment_id = %attachment_id,
        filename = %attachment.filename,
        bytes = file_data.len(),
        "File read successfully"
    );

    // Return file with appropriate headers
    let response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, attachment.mime_type.as_str()),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{}\"", attachment.filename),
            ),
            (header::CONTENT_LENGTH, &file_data.len().to_string()),
        ],
        file_data,
    )
        .into_response();

    Ok(response)
}

/// GET /api/items/:id/attachments
///
/// List all attachments for an item.
#[instrument(skip(state))]
pub async fn list_attachments(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    info!(item_id = %item_id, "Listing attachments");

    // Verify item exists
    state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {} not found", item_id)))?;

    use tack_db::repo::attachments::AttachmentRepo;
    let attachment_repo = AttachmentRepo::new(state.repo.pool());

    let attachments = attachment_repo.list_attachments(item_id).await?;

    info!(item_id = %item_id, count = attachments.len(), "Attachments listed");

    Ok(Json(
        attachments
            .into_iter()
            .map(|a| serde_json::to_value(a).unwrap())
            .collect(),
    ))
}

/// DELETE /api/attachments/:id
///
/// Delete an attachment file.
#[instrument(skip(state))]
pub async fn delete_attachment(
    State(state): State<AppState>,
    Path(attachment_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    info!(attachment_id = %attachment_id, "Deleting attachment");

    // Get attachment record
    use tack_db::repo::attachments::AttachmentRepo;
    let attachment_repo = AttachmentRepo::new(state.repo.pool());

    let attachment = attachment_repo
        .get_attachment(attachment_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Attachment {} not found", attachment_id)))?;

    // Delete file from disk
    let storage_base = PathBuf::from(&state.config.storage_dir);
    let file_path = storage_base.join(&attachment.storage_path);

    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|e| {
            warn!(error = %e, path = ?file_path, "Failed to delete file");
            anyhow::anyhow!("Failed to delete file: {}", e)
        })?;

        debug!(path = ?file_path, "File deleted from disk");
    }

    // Delete attachment record
    attachment_repo.delete_attachment(attachment_id).await?;

    info!(attachment_id = %attachment_id, "Attachment deleted successfully");

    Ok(StatusCode::NO_CONTENT)
}
