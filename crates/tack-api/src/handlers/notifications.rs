//! The inbox over HTTP and over the live socket (ADR 0035, decision 4). What notifies is
//! `tack_core::notification`'s and who writes the rows is `tack_app::notifications::Inbox`; this
//! file lists them, marks them read, and tells a screen what changed so it need not poll.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tracing::instrument;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use tack_app::notifications::Change;
use tack_core::notification::Notification;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query, Upgrade};
use crate::handlers::websocket::serve_live;
use crate::router::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

#[derive(Debug, Default, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationState {
    /// What still asks something of the person.
    #[default]
    Open,
    /// Everything kept, newest first.
    All,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// `open` (the default): what still asks something of the person. `all`: everything kept,
    /// newest first.
    #[serde(default)]
    state: NotificationState,
    /// How many to return, 50 by default and 500 at most. `0` returns none, for a caller that
    /// wants only `open`.
    limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationList {
    /// How many notifications are open, whatever `limit` cut.
    pub open: u64,
    pub notifications: Vec<Notification>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadAll {
    /// How many were unread.
    pub read: usize,
}

/// GET /api/notifications
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/notifications",
    tag = "notifications",
    params(ListParams),
    responses(
        (status = 200, description = "The notifications, newest first, and how many are open", body = NotificationList),
        (status = 400, description = "`state` or `limit` is not what it should be", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<NotificationList>> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let notifications = match params.state {
        NotificationState::Open => state.notifications.open_notifications(limit).await?,
        NotificationState::All => state.notifications.recent(limit).await?,
    };
    let open = state.notifications.open_count().await?;
    Ok(Json(NotificationList {
        open,
        notifications,
    }))
}

/// POST /api/notifications/{id}/read
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/notifications/{id}/read",
    tag = "notifications",
    params(("id" = Uuid, Path, description = "The notification")),
    responses(
        (status = 204, description = "Marked read; one that is read already, or not there, changes nothing"),
        (status = 400, description = "`id` is not an id", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn read_notification(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state.notifications.read(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/notifications/read-all
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/notifications/read-all",
    tag = "notifications",
    responses(
        (status = 200, description = "Every unread notification is read now", body = ReadAll),
    ),
)]
pub async fn read_all_notifications(State(state): State<AppState>) -> ApiResult<Json<ReadAll>> {
    let read = state.notifications.read_all().await?;
    Ok(Json(ReadAll { read }))
}

/// One message of the live socket: a [`Change`] and how many notifications are open after it.
#[derive(Serialize)]
struct Told<'a> {
    #[serde(flatten)]
    change: &'a Change,
    open: u64,
}

/// Just the count: the first message, and the one after a listener fell behind.
#[derive(Serialize)]
struct Counted {
    open: u64,
}

/// GET /api/notifications/live — upgrades to a WebSocket (subprotocol `tack.v1`, the token in a
/// second subprotocol `tack.auth.<base64url>`). It sends `{"open": n}` on connect, then each
/// change as `{"type": "opened" | "closed" | "read", …, "open": n}`; after a stretch of changes it
/// could not keep up with, `{"open": n}` again, which tells the client to read the list afresh.
#[instrument(skip(ws, state))]
pub async fn notifications_live(
    Upgrade(ws): Upgrade,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    Ok(serve_live(ws, move |mut out| async move {
        let inbox = state.notifications;
        // Listening starts before the count is read, so a change in between is told, not lost.
        let mut changes = inbox.subscribe();
        let Ok(open) = inbox.open_count().await else {
            return;
        };
        if !out.send(&Counted { open }).await {
            return;
        }
        loop {
            let sent = match changes.recv().await {
                Ok(change) => match inbox.open_count().await {
                    Ok(open) => {
                        out.send(&Told {
                            change: &change,
                            open,
                        })
                        .await
                    }
                    Err(_) => return,
                },
                Err(RecvError::Lagged(_)) => match inbox.open_count().await {
                    Ok(open) => out.send(&Counted { open }).await,
                    Err(_) => return,
                },
                Err(RecvError::Closed) => return,
            };
            if !sent {
                return;
            }
        }
    }))
}
