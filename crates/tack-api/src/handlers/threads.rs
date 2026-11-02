//! Threads (ADR 0034, decision 7): a conversation between a person and a profile. The operator
//! opens, lists and closes them; the operator, or an agent credential holding `converse` on the
//! thread's project, reads one and writes in it, and each message starts a turn. The rules live in
//! `tack_core::thread` and `tack_app::threads`; this file extracts, calls and answers.

use axum::Extension;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use tack_app::threads::{self, OpenThread, Post};
use tack_core::principal::Principal;
use tack_core::thread::{Message, Ref, Thread, ThreadScope, Turn};
use tack_protocol::SystemClock;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::router::AppState;

/// What opens a thread.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThread {
    /// The project the thread's turns run in; with no credential, the only project it reaches.
    pub project_id: Uuid,
    /// When given, the thread reaches the projects this credential may read, and only that
    /// credential (and the operator) may write in it.
    #[serde(default)]
    pub credential_id: Option<Uuid>,
    /// The agent profile whose runs are the thread's turns.
    pub profile: String,
    pub title: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ThreadFilter {
    /// Only the threads whose home is this project.
    project_id: Option<Uuid>,
    /// Only the open threads.
    #[serde(default)]
    open: bool,
}

/// A thread with everything in it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadDetail {
    pub thread: Thread,
    /// Oldest first.
    pub messages: Vec<Message>,
    /// Oldest first.
    pub turns: Vec<Turn>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PostMessage {
    pub body: String,
    /// Items, change sets and decisions the message is about.
    #[serde(default)]
    pub refs: Vec<Ref>,
}

/// What posting a message did.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostedMessage {
    pub message: Message,
    /// The turn the message started; absent when it could not start one.
    pub turn: Option<Turn>,
    /// What the thread says about a turn that could not start.
    pub notice: Option<Message>,
}

/// POST /api/threads
#[utoipa::path(
    post,
    path = "/api/threads",
    tag = "threads",
    request_body = CreateThread,
    responses(
        (status = 200, description = "The thread, open, with no message", body = Thread),
        (status = 400, description = "The title, the profile or the credential is not acceptable", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn create_thread(
    State(state): State<AppState>,
    Json(input): Json<CreateThread>,
) -> ApiResult<Json<Thread>> {
    let scope = match input.credential_id {
        Some(credential) => ThreadScope::Credential {
            credential,
            home: input.project_id,
        },
        None => ThreadScope::Project {
            project: input.project_id,
        },
    };
    Ok(Json(
        threads::open(
            &state.repo,
            &SystemClock,
            OpenThread {
                scope,
                profile: input.profile,
                title: input.title,
            },
        )
        .await?,
    ))
}

/// GET /api/threads
#[utoipa::path(
    get,
    path = "/api/threads",
    tag = "threads",
    params(ThreadFilter),
    responses((status = 200, description = "Threads, the most recently written first", body = Vec<Thread>)),
)]
#[instrument(skip(state))]
pub async fn list_threads(
    State(state): State<AppState>,
    Query(filter): Query<ThreadFilter>,
) -> ApiResult<Json<Vec<Thread>>> {
    let projects = filter.project_id.map(|project| vec![project]);
    Ok(Json(
        state
            .repo
            .list_threads(projects.as_deref(), filter.open)
            .await?,
    ))
}

/// GET /api/threads/{id}
#[utoipa::path(
    get,
    path = "/api/threads/{id}",
    tag = "threads",
    params(("id" = Uuid, Path, description = "Thread ID")),
    responses(
        (status = 200, description = "The thread, its messages and its turns", body = ThreadDetail),
        (status = 403, description = "The credential lacks the scope `converse` on the thread's project", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_thread(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ThreadDetail>> {
    let thread = state
        .repo
        .get_thread(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Thread {id} not found")))?;
    Ok(Json(ThreadDetail {
        messages: state.repo.list_thread_messages(id).await?,
        turns: state.repo.list_thread_turns(id).await?,
        thread,
    }))
}

/// POST /api/threads/{id}/messages
#[utoipa::path(
    post,
    path = "/api/threads/{id}/messages",
    tag = "threads",
    params(("id" = Uuid, Path, description = "Thread ID")),
    request_body = PostMessage,
    responses(
        (status = 200, description = "The message is in the thread; when a person wrote it, its turn has started (an agent's message starts none)", body = PostedMessage),
        (status = 400, description = "The message or its references are not acceptable", body = crate::openapi::ErrorEnvelope),
        (status = 403, description = "The credential lacks the scope `converse` on the thread's project", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "The thread is closed or idle, or a person's message is past the turn cap or comes before the last turn has answered", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, principal, input))]
pub async fn post_message(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<PostMessage>,
) -> ApiResult<Json<PostedMessage>> {
    let posted = threads::post(
        &state.repo,
        &SystemClock,
        Post {
            thread: id,
            principal: &principal,
            body: input.body,
            refs: input.refs,
        },
    )
    .await?;
    Ok(Json(PostedMessage {
        message: posted.message,
        turn: posted.turn,
        notice: posted.notice,
    }))
}

/// POST /api/threads/{id}/close
#[utoipa::path(
    post,
    path = "/api/threads/{id}/close",
    tag = "threads",
    params(("id" = Uuid, Path, description = "Thread ID")),
    responses(
        (status = 200, description = "The thread, closed", body = Thread),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn close_thread(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Thread>> {
    state.repo.close_thread(id, "closed").await?;
    state
        .repo
        .get_thread(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("Thread {id} not found")))
}
