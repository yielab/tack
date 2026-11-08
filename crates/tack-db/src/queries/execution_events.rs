use tack_protocol::event_kind::{AGENT_HOP_FINISHED, AGENT_HOP_STARTED};

use crate::{Repository, StoreError};

/// The latest hop event of a request's newest attempt: its kind and the line it reported.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LatestHopRow {
    pub request_id: String,
    pub kind: String,
    /// The event's `text`; `None` when its payload carries none.
    pub text: Option<String>,
}

/// The latest hop event (`agent.hop_started` or `agent.hop_finished`) of each listed request's
/// newest attempt, in one query for the whole batch. A request whose newest attempt reported
/// none (a single agent's run, or an attempt not yet at its first hop) is left out.
pub async fn latest_hops_of_requests(
    repo: &Repository,
    request_ids: &[String],
) -> Result<Vec<LatestHopRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT a.request_id, e.kind, json_extract(e.payload, '$.text') AS text
           FROM execution_attempts a
           JOIN execution_events e ON e.attempt_id = a.id
          WHERE a.request_id IN (SELECT value FROM json_each(?))
            AND a.attempt_number = (SELECT MAX(attempt_number) FROM execution_attempts WHERE request_id = a.request_id)
            AND e.kind IN (?, ?)
            AND e.sequence = (SELECT MAX(sequence) FROM execution_events WHERE attempt_id = a.id AND kind IN (?, ?))",
    )
    .bind(serde_json::json!(request_ids).to_string())
    .bind(AGENT_HOP_STARTED)
    .bind(AGENT_HOP_FINISHED)
    .bind(AGENT_HOP_STARTED)
    .bind(AGENT_HOP_FINISHED)
    .fetch_all(repo.pool())
    .await?)
}
