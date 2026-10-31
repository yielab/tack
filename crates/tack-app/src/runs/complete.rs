//! Completing an attempt: the runner reports how it ended, the store settles the attempt, and
//! what a success sets going follows: the item moves to the status the project maps a finished
//! run to, and a change the verifier rejected goes back to the agent that made it.

mod rework;
mod status_map;

use chrono::{DateTime, Utc};
use std::future::Future;

use serde_json::{Map, Value};
use tack_db::Repository;
use tack_db::repo::execution::{Completion, CompletionResponse, CompletionResult, FollowUps};
use tack_protocol::execution::{
    ActualExecution, AttemptId, Checkpoint, CompletionId, ExecutionState, FencingToken, RunnerId,
    Usage,
};
use tack_protocol::mrp::MergeReadinessPack;

use crate::AppError;

pub use status_map::ItemMoved;

/// How a runner says an attempt ended.
pub struct CompletionReport<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    /// The runner's own id for this report; the same id again is a replay.
    pub completion_id: &'a CompletionId,
    pub final_event_checkpoint: Option<&'a Checkpoint>,
    /// `Succeeded`, `Failed` or `Cancelled`.
    pub terminal_state: ExecutionState,
    /// The adapter's reason: an object with open keys (see `Completion::terminal_reason`).
    pub terminal_reason: &'a Map<String, Value>,
    pub actual_execution: &'a ActualExecution,
    pub usage: &'a Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completed {
    /// The attempt ended with this report. `item_moved` is the board move a success caused.
    Committed {
        response: CompletionResponse,
        item_moved: Option<ItemMoved>,
    },
    /// This `completion_id` had already ended the attempt; the first answer again.
    Replayed(CompletionResponse),
    /// `completion_id` was already used with different content. Retrying can never succeed.
    IdempotencyConflict,
    /// Another report already ended the attempt, or a concurrent writer won. The runner may
    /// retry.
    Conflict,
    /// The fence is no longer the attempt's current one.
    Stale,
}

/// Ends an attempt with the runner's report. Idempotent on `completion_id`. When a success is
/// committed, the item is moved per the request's status map (best effort: a refused move or a
/// failed lookup is logged and never fails the completion), and a rejection by the verifier
/// that names something to fix enqueues one follow-up run. Both are decided from what is stored
/// before the report and written in the transaction that ends the attempt, so a crash leaves
/// the attempt open with its report to be sent again, never ended with its board move lost.
///
/// `read_pack` reads the Merge-Readiness Pack stored at a content reference, or `None` when it
/// cannot be read or parsed; the bytes live outside the store, so the caller supplies it.
pub async fn complete<R, Fut>(
    repo: &Repository,
    now: DateTime<Utc>,
    report: CompletionReport<'_>,
    read_pack: R,
) -> Result<Completed, AppError>
where
    R: Fn(String) -> Fut,
    Fut: Future<Output = Option<MergeReadinessPack>>,
{
    let (item_move, rework) = if report.terminal_state == ExecutionState::Succeeded {
        let item_move =
            status_map::plan(repo, report.attempt_id.as_str(), report.terminal_reason).await;
        let rework = rework::plan(
            repo,
            now,
            report.attempt_id.as_str(),
            report.actual_execution,
            &read_pack,
        )
        .await
        .unwrap_or_else(|err| {
            tracing::warn!(
                attempt_id = %report.attempt_id,
                error = %err,
                "rework: could not plan the follow-up"
            );
            None
        });
        (item_move, rework)
    } else {
        (None, None)
    };
    let (result, followed) = repo
        .complete_execution_with_follow_ups(
            Completion {
                runner_id: report.runner_id,
                attempt_id: report.attempt_id,
                fencing_token: report.fencing_token,
                completion_id: report.completion_id,
                final_event_checkpoint: report.final_event_checkpoint,
                terminal_state: report.terminal_state,
                terminal_reason: report.terminal_reason,
                actual_execution: report.actual_execution,
                usage: report.usage,
            },
            FollowUps {
                item_update: item_move.as_ref().map(status_map::Move::item_update),
                rework: rework.as_ref().map(rework::Rework::follow_up),
            },
            now,
        )
        .await?;
    Ok(match result {
        CompletionResult::Committed(response) => Completed::Committed {
            response,
            item_moved: item_move.and_then(|planned| {
                planned.moved(report.attempt_id.as_str(), followed.item_update)
            }),
        },
        CompletionResult::Replayed(response) => Completed::Replayed(response),
        CompletionResult::IdempotencyConflict => Completed::IdempotencyConflict,
        CompletionResult::Conflict => Completed::Conflict,
        CompletionResult::Stale => Completed::Stale,
    })
}
