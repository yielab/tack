use crate::extract::{Path, Upgrade};
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures::{
    SinkExt,
    stream::{SplitSink, StreamExt},
};
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

/// The one subprotocol every live socket's handshake selects — it names the message
/// format the sockets actually send (tagged JSON), nothing more. The frontend
/// (`shared/realtime/boardSocket.ts`) always offers it. A client that offers a
/// subprotocol and gets a handshake response with no `Sec-WebSocket-Protocol`
/// header at all must fail the connection per the WebSocket spec (RFC 6455 §4.1),
/// so a server that does not select it here is silently unreachable from every
/// browser, even though the handshake itself still completes with a 101.
const LIVE_SUBPROTOCOL: &str = "tack.v1";

/// The sending half of a live socket.
pub struct Outbox(SplitSink<WebSocket, Message>);

impl Outbox {
    /// Sends `value` as one JSON text frame. `false` when the client is gone, which ends the feed.
    pub async fn send<T: Serialize>(&mut self, value: &T) -> bool {
        match serde_json::to_string(value) {
            Ok(json) => self.0.send(Message::Text(json.into())).await.is_ok(),
            Err(e) => {
                tracing::error!("Failed to serialize event: {}", e);
                true
            }
        }
    }
}

/// Accepts the upgrade, selecting [`LIVE_SUBPROTOCOL`], and runs `feed` against the client until
/// either side hangs up. The token travels in a second subprotocol the auth middleware reads
/// before this runs, so the handlers behind it never see it.
pub fn serve_live<F, Fut>(ws: WebSocketUpgrade, feed: F) -> Response
where
    F: FnOnce(Outbox) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    ws.protocols([LIVE_SUBPROTOCOL])
        .on_upgrade(move |socket| async move {
            let (sender, mut receiver) = socket.split();

            // Everything the client sends is ignored but a close; axum answers pings itself.
            let mut send_task = tokio::spawn(feed(Outbox(sender)));
            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = receiver.next().await {
                    if matches!(msg, Message::Close(_)) {
                        break;
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
        })
}

/// WebSocket handler for live board updates
#[instrument(skip(ws, state))]
pub async fn board_live(
    Upgrade(ws): Upgrade,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    // Verify project exists
    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    Ok(serve_live(ws, move |mut out| async move {
        let mut rx = state.broadcast_tx.subscribe();
        while let Ok(event) = rx.recv().await {
            // Only send events for this project
            if event_matches_project(&event, project_id) && !out.send(&event).await {
                break;
            }
        }
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The client-side requirement: an unknown event
    /// variant on the wire must be ignored, not thrown. On the Rust side the
    /// analogous guarantee is that decoding never panics on a *future* tag —
    /// `#[serde(tag = "type", rename_all = "snake_case")]` on a closed enum
    /// means an unrecognised `type` fails deserialization with an `Err`
    /// rather than a panic, so callers (like the frontend, and any future
    /// Rust WebSocket client) always get a recoverable `Result`.
    #[test]
    fn unrecognised_event_type_fails_to_deserialize_not_panic() {
        let raw = serde_json::json!({"type": "some_future_event", "project_id": Uuid::nil()});
        let result: Result<BoardEvent, _> = serde_json::from_value(raw);
        assert!(result.is_err());
    }
}
