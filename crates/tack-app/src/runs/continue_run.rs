//! Continuing a stopped step: a run whose task ran out of its own
//! budget — not a cliff, docket's own ADR 0038 — kept its work and may be told to go on. Continuing
//! is a new execution request, like a rework (`runs/complete/rework.rs`), but it never creates a
//! new docket task: the harness follows the same one (`harness/docket.rs`'s `continue_task`
//! metadata), with what was left of the first run's own token limit.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tack_db::repo::execution::{EnqueueResult, NewExecutionRequest};
use tack_db::{Repository, canonical_json, queries};
use tack_protocol::execution::StableErrorCode;

use super::start_run::{Refusal, Started};

fn error(code: StableErrorCode, message: &str, details: Value) -> Refusal {
    Refusal {
        code,
        message: message.to_owned(),
        details,
    }
}

fn not_found(reason: &str) -> Refusal {
    error(
        StableErrorCode::NotFound,
        "continues does not name an execution request",
        json!({"field": "continues", "reason": reason}),
    )
}

/// What the request named is not a run whose kept step may be continued: decision 5's table — no
/// such request, or one whose own ending never offered Continue (any failure that is not a step
/// budget stop with its work kept, a token-bound stop, a stop that changed nothing, or a docket
/// too old for `retryContinues`/`budgetOutcome`).
async fn continue_source(
    repo: &Repository,
    request_id: &str,
) -> Result<queries::execution_requests::ContinueSourceRow, Refusal> {
    queries::execution_requests::continue_source_of_request(repo, request_id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read the run's own ending");
            error(
                StableErrorCode::InternalError,
                "Could not read the run's own ending",
                json!({}),
            )
        })?
        .ok_or_else(|| not_found("continues_not_found"))
}

/// A new execution request that follows the same docket task `source_request_id`'s run left
/// stopped at its own step budget, with what was left of its token limit (decision 6). Refused
/// when the named request does not exist, or its own ending never offered Continue (decision 5) —
/// never offered at all, offered `false`, or left no team and task to follow.
pub async fn continue_run(
    repo: &Repository,
    now: DateTime<Utc>,
    source_request_id: &str,
) -> Result<Started, Refusal> {
    let source = continue_source(repo, source_request_id).await?;
    if source.possible != Some(1) {
        return Err(error(
            StableErrorCode::Unprocessable,
            source
                .why
                .as_deref()
                .unwrap_or("This run cannot be continued."),
            json!({"field": "continues", "reason": "continues_not_offered"}),
        ));
    }
    let (Some(pod), Some(task)) = (source.pod.clone(), source.task.clone()) else {
        return Err(error(
            StableErrorCode::Unprocessable,
            "The stopped run named no docket team and task to follow",
            json!({"field": "continues", "reason": "continues_not_offered"}),
        ));
    };

    // Decision 6: what the first run's limit has left, from what it counted the same way every
    // other budget reads a request's spend (`tack_db::queries::budgets::counted_by_request`),
    // never a figure worked out again here.
    let counted = queries::budgets::counted_by_request(repo, &[source_request_id.to_owned()])
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read what the run counted");
            error(
                StableErrorCode::InternalError,
                "Could not read what the run counted",
                json!({}),
            )
        })?
        .get(source_request_id)
        .copied()
        .unwrap_or(0);
    let remaining = source
        .limit_tokens
        .map(|limit| (limit - counted).max(0) as u64);
    if remaining == Some(0) {
        return Err(error(
            StableErrorCode::Unprocessable,
            "The task's token limit is spent, and continuing cannot raise it.",
            json!({"field": "continues", "reason": "continues_no_tokens_left"}),
        ));
    }

    let raw = queries::execution_requests::request_snapshot(repo, source_request_id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read the run's snapshot");
            error(
                StableErrorCode::InternalError,
                "Could not read the run's snapshot",
                json!({}),
            )
        })?
        .ok_or_else(|| not_found("continues_not_found"))?;
    let mut snapshot: Value = serde_json::from_str(&raw).map_err(|failure| {
        tracing::error!(error = %failure, "The run's snapshot is not valid JSON");
        error(
            StableErrorCode::InternalError,
            "Could not read the run's snapshot",
            json!({}),
        )
    })?;

    let key = format!("continue-{source_request_id}");
    let scope = &source.idempotency_scope;
    // A second ask under the same `continues` is a replay, not a new request: its `request_id`
    // and `created_at` must be the ones it was first stored with (`enqueue_execution` checks
    // `created_at` against `now` only for a request it has not seen before), the same way
    // `start_run`'s own retry does.
    let existing = queries::execution_requests::snapshot_of_idempotency_key(repo, scope, &key)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read a previous continuation");
            error(
                StableErrorCode::InternalError,
                "Could not read a previous continuation",
                json!({}),
            )
        })?;
    let stored_invalid = || {
        error(
            StableErrorCode::InternalError,
            "A previously stored continuation could not be read back",
            json!({}),
        )
    };
    let (new_id, created_at) = match existing {
        Some(stored) => {
            let stored: Value = serde_json::from_str(&stored).map_err(|_| stored_invalid())?;
            let id = stored["request_id"]
                .as_str()
                .ok_or_else(stored_invalid)?
                .to_owned();
            let at = stored["created_at"]
                .as_str()
                .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                .ok_or_else(stored_invalid)?
                .with_timezone(&Utc);
            (id, at)
        }
        None => (
            format!(
                "exec_{}",
                hex::encode(Sha256::digest(format!("{scope}\0{key}").as_bytes()))
            ),
            now,
        ),
    };
    let object = snapshot.as_object_mut().ok_or_else(|| {
        error(
            StableErrorCode::InternalError,
            "The run's snapshot is not an object",
            json!({}),
        )
    })?;
    object.insert("request_id".into(), json!(new_id));
    object.insert("idempotency_key".into(), json!(key));
    object.insert("created_at".into(), json!(created_at.to_rfc3339()));
    let mut metadata = object.get("metadata").cloned().unwrap_or(json!({}));
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("continues".into(), json!(source_request_id));
        metadata.insert("continue_task".into(), json!(format!("{pod}/{task}")));
        metadata.insert(
            "continue_baseline_usage".into(),
            json!({
                "input": source.tokens_in.unwrap_or(0).max(0),
                "output": source.tokens_out.unwrap_or(0).max(0),
            }),
        );
        // The new request's own limit is set below, not inherited from the run it continues.
        metadata.remove("limit_source");
        metadata.remove("rework_base");
        metadata.remove("rework_of");
    }
    object.insert("metadata".into(), metadata);
    if let Some(budgets) = object.get_mut("budgets").and_then(Value::as_object_mut) {
        match remaining {
            Some(tokens) => {
                budgets.insert("tokens".into(), json!(tokens));
            }
            None => {
                budgets.remove("tokens");
            }
        }
    }

    let snapshot = canonical_json(snapshot);
    let text = |name: &str| snapshot[name].to_string();
    let request_snapshot = snapshot.to_string();
    let fingerprint = hex::encode(Sha256::digest(request_snapshot.as_bytes()));
    let selector_id = snapshot["selector"]["runner_id"]
        .as_str()
        .or(snapshot["selector"]["fleet_id"].as_str())
        .unwrap_or("");
    let owned = |name: &str| snapshot[name].as_str().map(str::to_owned);

    let result = repo
        .enqueue_execution(
            NewExecutionRequest {
                id: &new_id,
                item_id: &source.item_id,
                idempotency_scope: &source.idempotency_scope,
                idempotency_key: &key,
                request_fingerprint: &fingerprint,
                selector_kind: snapshot["selector"]["kind"].as_str().unwrap_or(""),
                selector_id,
                agent_profile_id: owned("agent_profile_id").as_deref(),
                agent_profile_snapshot: &text("resolved_agent_profile"),
                requested_harness_kind: owned("requested_harness_kind").as_deref(),
                requested_model: owned("requested_model").as_deref(),
                repository_snapshot: &text("repository"),
                permission_policy: &text("permission_policy"),
                timeout_seconds: snapshot["timeout_seconds"].as_i64(),
                budgets: &text("budgets"),
                environment: &text("environment"),
                metadata: &text("metadata"),
                request_snapshot: &request_snapshot,
            },
            now,
        )
        .await
        .map_err(|failure| {
            tracing::warn!(error = %failure, "enqueue_execution failed for a continuation");
            error(
                StableErrorCode::InternalError,
                "Could not enqueue the continuation",
                json!({}),
            )
        })?;
    match result {
        EnqueueResult::Created(request_id) => Ok(Started {
            request_id,
            replayed: false,
        }),
        EnqueueResult::Replayed(request_id) => Ok(Started {
            request_id,
            replayed: true,
        }),
        EnqueueResult::Conflict => Err(error(
            StableErrorCode::IdempotencyConflict,
            "The idempotency key was used with a different request",
            json!({"idempotency_key": key}),
        )),
    }
}
