use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures::{SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::{error::ApiError, router::AppState};

/// Messages sent over WebSocket for board updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardEvent {
    /// Item was created
    ItemCreated {
        project_id: Uuid,
        item_id: Uuid,
        status: String,
    },
    /// Item was updated (moved, edited, etc)
    ItemUpdated {
        project_id: Uuid,
        item_id: Uuid,
        old_status: Option<String>,
        new_status: String,
    },
    /// Item was deleted
    ItemDeleted { project_id: Uuid, item_id: Uuid },
    /// Board configuration changed (columns, WIP limits)
    BoardConfigUpdated { project_id: Uuid },
    /// Sprint changed
    SprintUpdated { project_id: Uuid, sprint_id: Uuid },
    /// Keepalive ping
    Ping,
}

/// WebSocket handler for live board updates
#[instrument(skip(ws, state))]
pub async fn board_live(
    ws: WebSocketUpgrade,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    // Verify project exists
    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, project_id, state)))
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, project_id: Uuid, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.broadcast_tx.subscribe();

    // Spawn task to receive broadcast messages and send to client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Only send events for this project
            if event_matches_project(&event, project_id) {
                let msg = match serde_json::to_string(&event) {
                    Ok(json) => Message::Text(json.into()),
                    Err(e) => {
                        tracing::error!("Failed to serialize event: {}", e);
                        continue;
                    }
                };

                if sender.send(msg).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn task to receive messages from client (mostly pings)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Received WebSocket message: {}", text);
                    // Could handle client commands here if needed
                }
                Message::Close(_) => {
                    break;
                }
                Message::Ping(data) => {
                    // Axum handles ping/pong automatically, but we log it
                    tracing::trace!("Received ping: {:?}", data);
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }
}

/// Check if event is relevant for this project
fn event_matches_project(event: &BoardEvent, project_id: Uuid) -> bool {
    match event {
        BoardEvent::ItemCreated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::ItemUpdated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::ItemDeleted {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::BoardConfigUpdated { project_id: pid } => *pid == project_id,
        BoardEvent::SprintUpdated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::Ping => true,
    }
}

/// Helper function to broadcast events (called from other handlers)
pub fn broadcast_event(state: &AppState, event: BoardEvent) {
    // Ignore errors - no subscribers is fine
    let _ = state.broadcast_tx.send(event);
}
