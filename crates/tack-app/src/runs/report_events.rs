use chrono::{DateTime, Utc};
use tack_db::Repository;
use tack_db::repo::execution::{EventApplyResult, EventBatch, EventBatchResult, NewEvent};
use tack_protocol::execution::{AttemptId, Checkpoint, EventRecord, FencingToken, RunnerId};
use uuid::Uuid;

use crate::AppError;

/// A batch of events a runner reports for the attempt it holds.
pub struct EventReport<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    /// The checkpoint the runner believes the stream is at, when it sent one.
    pub previous_checkpoint: Option<&'a Checkpoint>,
    /// The checkpoint the stream reaches when this batch lands.
    pub checkpoint: &'a Checkpoint,
    /// The events as the protocol's message carries them.
    pub events: &'a [EventRecord],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventsReported {
    /// The batch landed, or it is an exact replay of one that did.
    Applied(EventBatchResult),
    /// This `(attempt, checkpoint)` key was already used with different events. Retrying the
    /// same key can never succeed.
    IdempotencyConflict,
    /// The stream is not where this batch expected it (out of order, or a concurrent writer won).
    /// The runner may resync and retry.
    Conflict,
    /// The fence is no longer the attempt's current one.
    Stale,
}

/// Appends a batch of events to an attempt's stream and moves its checkpoint, once per
/// checkpoint: the same batch again is a replay, the same checkpoint with other events is an
/// idempotency conflict, and a batch that does not continue from the stream's checkpoint is a
/// conflict. Nothing is written for a stale fence.
pub async fn report_events(
    repo: &Repository,
    now: DateTime<Utc>,
    report: EventReport<'_>,
) -> Result<EventsReported, AppError> {
    let rows: Vec<String> = report
        .events
        .iter()
        .map(|_| format!("evt_row_{}", Uuid::new_v4()))
        .collect();
    let events: Vec<NewEvent<'_>> = report
        .events
        .iter()
        .zip(&rows)
        .map(|(event, row_id)| NewEvent {
            id: row_id,
            event_id: &event.event_id,
            sequence: event.sequence,
            source: &event.source,
            kind: &event.kind,
            payload: &event.payload,
            occurred_at: event.occurred_at,
        })
        .collect();
    let result = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: report.runner_id,
                attempt_id: report.attempt_id,
                fencing_token: report.fencing_token,
                previous_checkpoint: report.previous_checkpoint,
                checkpoint: report.checkpoint,
            },
            &events,
            now,
        )
        .await?;
    Ok(match result {
        EventApplyResult::Applied(batch) => EventsReported::Applied(batch),
        EventApplyResult::IdempotencyConflict => EventsReported::IdempotencyConflict,
        EventApplyResult::Conflict => EventsReported::Conflict,
        EventApplyResult::Stale => EventsReported::Stale,
    })
}
