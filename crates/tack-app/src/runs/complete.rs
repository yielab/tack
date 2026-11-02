//! Completing an attempt: the runner reports how it ended, the store settles the attempt, and
//! what a success sets going follows: a change the verifier rejected goes back to the agent that
//! made it. The task's status is never written here; it is a person's.

mod rework;

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
    /// The attempt ended with this report.
    Committed { response: CompletionResponse },
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
/// committed, a rejection by the verifier that names something to fix enqueues one follow-up
/// run. It is decided from what is stored before the report and written in the transaction that
/// ends the attempt, so a crash leaves the attempt open with its report to be sent again, never
/// ended with its follow-up lost.
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
    let rework = if report.terminal_state == ExecutionState::Succeeded {
        rework::plan(
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
        })
    } else {
        None
    };
    let (result, _) = repo
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
                rework: rework.as_ref().map(rework::Rework::follow_up),
            },
            now,
        )
        .await?;
    Ok(match result {
        CompletionResult::Committed(response) => Completed::Committed { response },
        CompletionResult::Replayed(response) => Completed::Replayed(response),
        CompletionResult::IdempotencyConflict => Completed::IdempotencyConflict,
        CompletionResult::Conflict => Completed::Conflict,
        CompletionResult::Stale => Completed::Stale,
    })
}
