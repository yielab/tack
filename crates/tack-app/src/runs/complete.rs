//! Completing an attempt: the runner reports how it ended, the store settles the attempt, and
//! what a success sets going follows: the item moves to the status the project maps a finished
//! run to, and a change the verifier rejected goes back to the agent that made it.

mod rework;
mod status_map;

use std::future::Future;

use tack_db::Repository;
use tack_db::repo::execution::{Completion, CompletionResponse, CompletionResult, state};
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::MergeReadinessPack;

use crate::AppError;

pub use status_map::ItemMoved;

/// How a runner says an attempt ended.
pub struct CompletionReport<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    /// The runner's own id for this report; the same id again is a replay.
    pub completion_id: &'a str,
    pub final_event_checkpoint: Option<&'a str>,
    /// `Succeeded`, `Failed` or `Cancelled`.
    pub terminal_state: ExecutionState,
    /// The reason, the execution and the usage, already encoded as JSON.
    pub terminal_reason_json: &'a str,
    pub actual_execution_json: &'a str,
    pub usage_json: &'a str,
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
/// that names something to fix enqueues one follow-up run.
///
/// `read_pack` reads the Merge-Readiness Pack stored at a content reference, or `None` when it
/// cannot be read or parsed; the bytes live outside the store, so the caller supplies it.
pub async fn complete<R, Fut>(
    repo: &Repository,
    clock: &dyn Clock,
    report: CompletionReport<'_>,
    read_pack: R,
) -> Result<Completed, AppError>
where
    R: Fn(String) -> Fut,
    Fut: Future<Output = Option<MergeReadinessPack>>,
{
    let result = repo
        .complete_execution_result(
            Completion {
                runner_id: report.runner_id,
                attempt_id: report.attempt_id,
                fencing_token: report.fencing_token,
                completion_id: report.completion_id,
                final_event_checkpoint: report.final_event_checkpoint,
                terminal_state: state::text(report.terminal_state),
                terminal_reason: report.terminal_reason_json,
                actual_execution: report.actual_execution_json,
                usage: report.usage_json,
            },
            clock,
        )
        .await?;
    Ok(match result {
        CompletionResult::Committed(response) => {
            let mut item_moved = None;
            if report.terminal_state == ExecutionState::Succeeded {
                item_moved = status_map::apply(repo, report.attempt_id).await;
                if let Err(err) =
                    rework::enqueue_rework(repo, clock, report.attempt_id, &read_pack).await
                {
                    tracing::warn!(
                        attempt_id = report.attempt_id,
                        error = %err,
                        "rework: could not enqueue the follow-up"
                    );
                }
            }
            Completed::Committed {
                response,
                item_moved,
            }
        }
        CompletionResult::Replayed(response) => Completed::Replayed(response),
        CompletionResult::IdempotencyConflict => Completed::IdempotencyConflict,
        CompletionResult::Conflict => Completed::Conflict,
        CompletionResult::Stale => Completed::Stale,
    })
}
