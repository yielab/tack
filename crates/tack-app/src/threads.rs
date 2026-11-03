//! Threads (ADR 0034, decision 7): opening one, writing in it, and the turn each message
//! starts. A turn is a run of the thread's profile, against the thread's hidden item, under the
//! principal `thread:<id>`; its input is the window of messages and the digest. When the run
//! ends, [`settle`] posts its final message as the reply, with the change sets the turn made.
//! The rules (the window, the caps) are `tack_core::thread`'s; this file asks the store and
//! starts the run.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tack_core::models::Project;
use tack_core::principal::{Capability, Principal, may};
use tack_core::thread::{
    self, Message, Ref, RefKind, Thread, ThreadScope, Turn, TurnState, body_problem, fit_reply,
    opens_turn, refs_problem, title_problem, turn_principal,
};
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tack_protocol::conversation::{REPLY_FILE, REPLY_MEDIA_TYPE, TurnNote};
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

use crate::AppError;
use crate::auto_start::{Permitted, permissions_for};
use crate::background::{Loops, Start};
use crate::ports::Artifacts;
use crate::runs::start_run::{RunRequest, resolve_profile, start_run};

/// How far back the digest a turn reads looks.
const DIGEST_WINDOW_HOURS: i64 = 24;

/// How often the turns whose runs ended are answered.
const SETTLE_EVERY: Duration = Duration::from_secs(5);

/// What opens a thread.
pub struct OpenThread {
    pub scope: ThreadScope,
    pub profile: String,
    pub title: String,
}

/// Opens a thread. The profile must exist and the project be there.
pub async fn open(
    repo: &Repository,
    clock: &dyn Clock,
    input: OpenThread,
) -> Result<Thread, AppError> {
    if let Some(problem) = title_problem(&input.title) {
        return Err(AppError::Invalid(problem));
    }
    let project = repo
        .get_project(input.scope.home())
        .await?
        .ok_or(AppError::NotFound)?;
    if !queries::agent_profiles::profile_exists(repo, &input.profile).await? {
        return Err(AppError::Invalid(format!(
            "the agent profile {} does not exist",
            input.profile
        )));
    }
    if let Some(credential) = input.scope.credential()
        && repo.get_agent_credential(credential).await?.is_none()
    {
        return Err(AppError::Invalid(format!(
            "the credential {credential} does not exist"
        )));
    }
    let anchor_status = project.workflow.initial_status()?;
    Ok(repo
        .create_thread(&tack_db::repo::threads::NewThread {
            scope: input.scope,
            profile: &input.profile,
            title: input.title.trim(),
            anchor_status: &anchor_status,
            now: clock.now(),
        })
        .await?)
}

/// What a message is posted with.
pub struct Post<'a> {
    pub thread: Uuid,
    pub principal: &'a Principal,
    pub body: String,
    pub refs: Vec<Ref>,
}

/// What posting a message did.
#[derive(Debug)]
pub struct Posted {
    pub message: Message,
    /// The turn the message started; `None` when it could not start one, or when its author
    /// does not start turns (an agent's message).
    pub turn: Option<Turn>,
    /// The note posted in the thread when the turn could not start, saying why.
    pub notice: Option<Message>,
}

/// Writes `post.body` in the thread and, when its author is one whose messages start a turn
/// (`thread::opens_turn`: the operator), starts that turn. A thread that reached a cap, or has
/// been idle for too long, closes and refuses; a thread whose last turn has not answered yet
/// refuses the person's next message. An agent's message starts no turn, so it is posted even
/// while a turn is pending or the turn cap is reached, and a profile that writes to its own
/// thread cannot start a loop.
pub async fn post(
    repo: &Repository,
    clock: &dyn Clock,
    post: Post<'_>,
) -> Result<Posted, AppError> {
    if let Some(problem) = body_problem(&post.body).or_else(|| refs_problem(&post.refs)) {
        return Err(AppError::Invalid(problem));
    }
    let now = clock.now();
    let thread = repo
        .get_thread(post.thread)
        .await?
        .ok_or(AppError::NotFound)?;
    let starts_turn = opens_turn(post.principal.kind);
    if let Err(stop) = thread::may_post(&thread, now, starts_turn) {
        if let Some(reason) = stop.closing_reason() {
            repo.close_thread(thread.id, reason).await?;
        }
        return Err(AppError::Conflict(stop.to_string()));
    }
    if starts_turn
        && repo
            .list_thread_turns(thread.id)
            .await?
            .iter()
            .any(|turn| turn.state == TurnState::Pending)
    {
        return Err(AppError::Conflict(
            "the previous turn has not answered yet; write again when it has".into(),
        ));
    }
    let message = repo
        .append_thread_message(
            thread.id,
            &post.principal.label(),
            post.body.trim(),
            &post.refs,
            now,
        )
        .await?;
    if !starts_turn {
        return Ok(Posted {
            message,
            turn: None,
            notice: None,
        });
    }
    match begin_turn(repo, clock, &thread, &message).await {
        Ok(turn) => Ok(Posted {
            message,
            turn: Some(turn),
            notice: None,
        }),
        Err(why) => {
            tracing::warn!(thread = %thread.id, %why, "thread: the turn could not start");
            let notice = repo
                .append_thread_message(
                    thread.id,
                    &turn_principal(thread.id),
                    &fit_reply(&format!("This turn could not start: {why}")),
                    &[],
                    clock.now(),
                )
                .await?;
            Ok(Posted {
                message,
                turn: None,
                notice: Some(notice),
            })
        }
    }
}

/// The projects a thread's (or a trigger's) digest covers: its project, or the ones its
/// credential may read.
pub(crate) async fn digest_projects(
    repo: &Repository,
    scope: &ThreadScope,
    home: &Project,
) -> Result<(Vec<Project>, bool), AppError> {
    let Some(credential) = scope.credential() else {
        return Ok((vec![home.clone()], false));
    };
    let principal = repo
        .get_agent_credential(credential)
        .await?
        .ok_or(AppError::NotFound)?
        .principal();
    let projects = repo
        .list_projects(home.workspace_id)
        .await?
        .into_iter()
        .filter(|project| !project.archived && may(&principal, project.id, Capability::Read))
        .collect();
    Ok((projects, principal.may_everywhere(Capability::Read)))
}

/// Starts the run that answers `message`, and records it as the thread's next turn.
async fn begin_turn(
    repo: &Repository,
    clock: &dyn Clock,
    thread: &Thread,
    message: &Message,
) -> Result<Turn, String> {
    let now = clock.now();
    let internal = |what: &str| {
        let what = what.to_owned();
        move |failure: &dyn std::fmt::Display| {
            tracing::error!(error = %failure, "thread: {what}");
            format!("{what}; the board's log says why")
        }
    };
    let anchor = repo
        .thread_anchor(thread.id)
        .await
        .map_err(|e| internal("could not read the thread")(&e))?
        .ok_or("the thread has no item to run against")?;
    let item = repo
        .get_item(anchor)
        .await
        .map_err(|e| internal("could not read the thread's item")(&e))?
        .ok_or("the thread has no item to run against")?;
    let home = repo
        .get_project(thread.scope.home())
        .await
        .map_err(|e| internal("could not read the project")(&e))?
        .ok_or("the thread's project is gone")?;
    let messages = repo
        .list_thread_messages(thread.id)
        .await
        .map_err(|e| internal("could not read the messages")(&e))?;
    let (projects, outside_board) = digest_projects(repo, &thread.scope, &home)
        .await
        .map_err(|e| internal("could not tell which projects the thread reaches")(&e))?;
    let digest = crate::digest::read(
        repo,
        clock,
        &projects,
        now - chrono::Duration::hours(DIGEST_WINDOW_HOURS),
        outside_board,
    )
    .await
    .map_err(|e| internal("could not compute the digest")(&e))?;
    let digest = serde_json::to_string(&digest).unwrap_or_else(|_| "{}".into());
    let input = thread::turn_input(&thread::window(&messages), &digest, REPLY_FILE);
    let input = format!(
        "{input}\nThis conversation is thread {id}. When you propose a change set, give it the \
         origin `thread` and the origin_ref `{id}`, so the reply can show it.\n",
        id = thread.id
    );
    repo.set_anchor_text(anchor, &input, now)
        .await
        .map_err(|e| internal("could not write the turn's input")(&e))?;

    let position = thread.turns + 1;
    let resolved = resolve_profile(repo, &item, &home, Some(&thread.profile))
        .await
        .map_err(|refusal| refusal.message)?
        .ok_or("the thread's agent profile cannot be resolved")?;
    let Permitted { permissions, .. } = permissions_for(repo, &home, &resolved).await;
    let note = TurnNote {
        id: thread.id.to_string(),
        turn: position,
        max_turns: thread::MAX_TURNS,
        message: message.body.clone(),
    };
    let request = RunRequest {
        item_id: anchor,
        idempotency_key: format!("thread:{}:{position}", thread.id),
        selector: None,
        agent_profile_id: Some(thread.profile.clone()),
        requested_harness_kind: None,
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: None,
        permission_policy: permissions,
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({
            "started_by": "thread",
            tack_protocol::conversation::TURN_KEY: note.into_metadata(),
        }),
        timeout_seconds: None,
        verify: None,
        push_branch: None,
        autonomy: None,
        origin: None,
        from_run: None,
    };
    let started = start_run(repo, now, &turn_principal(thread.id), request)
        .await
        .map_err(|refusal| refusal.message)?;
    repo.start_thread_turn(thread.id, message.id, &started.request_id, now)
        .await
        .map_err(|e| internal("could not record the turn")(&e))
}

/// What a turn that ended says.
enum Ended {
    Reply(String),
    Failed(String),
}

/// How the run of a turn ended, or `None` while it goes on.
async fn ended(
    repo: &Repository,
    artifacts: &dyn Artifacts,
    turn: &Turn,
) -> Result<Option<Ended>, AppError> {
    let Some(state) = queries::execution_requests::request_state(repo, &turn.request_id).await?
    else {
        return Ok(Some(Ended::Failed("the run is gone".into())));
    };
    match state {
        ExecutionState::Succeeded => {
            let attempt =
                queries::execution_attempts::latest_attempt_of_request(repo, &turn.request_id)
                    .await?;
            let reference = match attempt {
                Some(attempt) => {
                    queries::execution_artifacts::latest_content_reference(
                        repo,
                        &attempt,
                        REPLY_MEDIA_TYPE,
                    )
                    .await?
                }
                None => None,
            };
            let reply = match reference {
                Some(reference) => artifacts
                    .read(&reference)
                    .await
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .filter(|text| !text.trim().is_empty()),
                None => None,
            };
            Ok(Some(match reply {
                Some(reply) => Ended::Reply(reply),
                None => Ended::Failed("the agent finished without leaving a reply".into()),
            }))
        }
        ExecutionState::Failed | ExecutionState::Cancelled => Ok(Some(Ended::Failed(format!(
            "the run ended {}",
            state.as_str()
        )))),
        _ => Ok(None),
    }
}

/// Posts the reply of every turn whose run has ended, once each, and returns how many it
/// answered. The reply refers to the change sets the turn made.
pub async fn settle(
    repo: &Repository,
    clock: &dyn Clock,
    artifacts: &dyn Artifacts,
) -> Result<usize, AppError> {
    let mut answered = 0;
    for turn in repo.running_thread_turns().await? {
        let Some(outcome) = ended(repo, artifacts, &turn).await? else {
            continue;
        };
        let now = clock.now();
        let (state, body, refs) = match outcome {
            Ended::Reply(reply) => {
                let sets = repo
                    .change_sets_of_thread(turn.thread, turn.started_at)
                    .await?;
                let refs = sets
                    .iter()
                    .map(|set| Ref {
                        kind: RefKind::ChangeSet,
                        id: set.to_string(),
                    })
                    .collect();
                (TurnState::Replied, fit_reply(&reply), refs)
            }
            Ended::Failed(why) => (
                TurnState::Failed,
                format!("This turn failed: {why}."),
                Vec::new(),
            ),
        };
        // Whoever settles the turn first posts; a second pass finds it settled.
        if repo
            .settle_thread_turn(turn.thread, turn.position, state, now)
            .await?
        {
            repo.append_thread_message(
                turn.thread,
                &turn_principal(turn.thread),
                &body,
                &refs,
                now,
            )
            .await?;
            answered += 1;
        }
    }
    Ok(answered)
}

/// One pass of the loop: answers the turns that ended and closes the threads left idle.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    artifacts: &dyn Artifacts,
) -> Result<usize, AppError> {
    let answered = settle(repo, clock, artifacts).await?;
    let cutoff = clock.now() - chrono::Duration::days(thread::IDLE_DAYS);
    repo.close_idle_threads(cutoff).await?;
    Ok(answered)
}

/// Adds the loop to the server's loops.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    artifacts: Arc<dyn Artifacts>,
) {
    loops.spawn_tick("threads", Start::AfterInterval, SETTLE_EVERY, move || {
        let (repo, clock, artifacts) = (repo.clone(), clock.clone(), artifacts.clone());
        async move {
            if let Err(error) = tick(&repo, clock.as_ref(), artifacts.as_ref()).await {
                tracing::warn!(%error, "threads: a pass failed");
            }
        }
    });
}
