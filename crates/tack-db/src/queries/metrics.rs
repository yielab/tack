//! The rows behind the Quality figures of `GET /api/projects/{id}/metrics/factory`: one row per
//! attempt in the window with everything a figure or a cut needs, so the arithmetic lives in one
//! place above and none of it in SQL. `since` is an RFC 3339 instant compared to the attempt's
//! `created_at` with `julianday`; `None` is all time.

use sqlx::{AssertSqlSafe, Row};

use crate::repo::metrics::RUN_MODEL_SQL;
use crate::{Repository, StoreError};

/// One attempt of the project, with its request, its review and its pull request.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct QualityAttempt {
    pub attempt_id: String,
    pub request_id: String,
    pub item_id: String,
    pub state: String,
    /// The attempt's `terminal_reason`, as stored JSON.
    pub terminal_reason: Option<String>,
    /// The name of the profile the run resolved.
    pub profile: Option<String>,
    /// The agent that ran: the one the runner reported, else the one asked for.
    pub harness: Option<String>,
    /// The model the run is reported under (`RUN_MODEL_SQL`).
    pub model: Option<String>,
    pub team: Option<String>,
    /// The fingerprint of the prompt templates the request was composed with.
    pub template: Option<String>,
    /// No earlier attempt exists on the item.
    pub first_of_item: bool,
    /// The first task of the chain of recurring tasks the item belongs to; `None` for a task that does not repeat.
    pub recurring_root: Option<String>,
    /// A person's verdict on the attempt: `accepted` or `rejected`.
    pub verdict: Option<String>,
    /// The state of the pull request the attempt opened.
    pub pull_request: Option<String>,
    /// Where the bytes of the attempt's newest merge-readiness pack are kept.
    pub pack_reference: Option<String>,
}

/// Every attempt of the project created in the window, oldest first.
pub async fn quality_attempts(
    repo: &Repository,
    project_id: &str,
    media_type: &str,
    since: Option<&str>,
) -> Result<Vec<QualityAttempt>, StoreError> {
    // The fragments are constants of this crate; only values are bound.
    Ok(sqlx::query_as(AssertSqlSafe(format!(
        "WITH RECURSIVE chain(id, root) AS (
             SELECT id, id FROM items WHERE project_id = ?1 AND recurrence_of IS NULL
             UNION ALL
             SELECT i.id, chain.root FROM items i JOIN chain ON i.recurrence_of = chain.id)
         SELECT a.id AS attempt_id, a.request_id, r.item_id, a.state, a.terminal_reason,
                json_extract(r.request_snapshot, '$.resolved_agent_profile.name') AS profile,
                COALESCE(json_extract(a.actual_execution, '$.harness_kind'), r.requested_harness_kind) AS harness,
                {RUN_MODEL_SQL} AS model,
                json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team') AS team,
                r.prompt_template_sha AS template,
                NOT EXISTS (SELECT 1 FROM execution_attempts earlier
                              JOIN execution_requests er ON er.id = earlier.request_id
                             WHERE er.item_id = r.item_id
                               AND julianday(earlier.created_at) < julianday(a.created_at)) AS first_of_item,
                CASE WHEN i.recurrence IS NOT NULL OR i.recurrence_of IS NOT NULL
                     THEN chain.root END AS recurring_root,
                v.verdict AS verdict,
                p.state AS pull_request,
                (SELECT x.content_reference FROM execution_artifacts x
                  WHERE x.attempt_id = a.id AND x.media_type = ?2
                  ORDER BY x.created_at DESC, x.id DESC LIMIT 1) AS pack_reference
           FROM execution_attempts a
           JOIN execution_requests r ON r.id = a.request_id
           JOIN items i ON i.id = r.item_id
           LEFT JOIN chain ON chain.id = i.id
           LEFT JOIN attempt_reviews v ON v.attempt_id = a.id
           LEFT JOIN pull_requests p ON p.attempt_id = a.id
          WHERE i.project_id = ?1 AND (?3 IS NULL OR julianday(a.created_at) >= julianday(?3))
          ORDER BY a.created_at, a.id"
    )))
    .bind(project_id)
    .bind(media_type)
    .bind(since)
    .fetch_all(repo.pool())
    .await?)
}

/// What the agents of one kind (`harness`) asked a person in the window, and what they did: a
/// row per kind of question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionRow {
    pub harness: Option<String>,
    /// The decision's `kind`: what sort of thing was asked.
    pub class: String,
    pub asked: i64,
    /// Answered by a person.
    pub answered: i64,
    /// Answered with an option that lets the agent go on (`allow…`).
    pub granted: i64,
}

/// The decisions of the project's attempts in the window, by harness and kind.
pub async fn question_rows(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<QuestionRow>, StoreError> {
    let rows = sqlx::query(
        "SELECT COALESCE(json_extract(a.actual_execution, '$.harness_kind'), r.requested_harness_kind) AS harness,
                d.kind AS class, COUNT(*) AS asked,
                COALESCE(SUM(d.state = 'resolved'), 0) AS answered,
                COALESCE(SUM(d.state = 'resolved' AND json_valid(d.answer)
                             AND json_extract(d.answer, '$.option_id') LIKE 'allow%'), 0) AS granted
           FROM execution_decisions d
           JOIN execution_attempts a ON a.id = d.attempt_id
           JOIN execution_requests r ON r.id = a.request_id
           JOIN items i ON i.id = r.item_id
          WHERE i.project_id = ? AND (? IS NULL OR julianday(a.created_at) >= julianday(?))
          GROUP BY harness, class
          ORDER BY harness, class",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_all(repo.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| QuestionRow {
            harness: row.get("harness"),
            class: row.get("class"),
            asked: row.get("asked"),
            answered: row.get("answered"),
            granted: row.get("granted"),
        })
        .collect())
}

/// The steps the agents reported in the window, by harness: a local agent's model calls and
/// a docket team's hops. The count a question's rate is taken over.
pub async fn action_counts(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<(Option<String>, i64)>, StoreError> {
    let rows = sqlx::query(
        "SELECT COALESCE(json_extract(a.actual_execution, '$.harness_kind'), r.requested_harness_kind) AS harness,
                COUNT(*) AS actions
           FROM execution_events e
           JOIN execution_attempts a ON a.id = e.attempt_id
           JOIN execution_requests r ON r.id = a.request_id
           JOIN items i ON i.id = r.item_id
          WHERE i.project_id = ? AND e.kind IN (?, ?)
            AND (? IS NULL OR julianday(a.created_at) >= julianday(?))
          GROUP BY harness
          ORDER BY harness",
    )
    .bind(project_id)
    .bind(tack_protocol::event_kind::AGENT_MODEL_CALL)
    .bind(tack_protocol::event_kind::AGENT_HOP_STARTED)
    .bind(since)
    .bind(since)
    .fetch_all(repo.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("harness"), row.get("actions")))
        .collect())
}
