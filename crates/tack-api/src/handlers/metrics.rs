//! Factory metrics: what the agent fleet cost a project in attempts, human
//! minutes and tokens, and what became of its pull requests. Every number is
//! computed from rows; every ratio carries its numerator and denominator, and a
//! figure with nothing to measure is `null` with its reason (`denominator_zero`
//! or `not_measured`), never 0. No dollar figure is read or summed here.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_db::repo::metrics::{self as rows, WaitRow};

use crate::error::{ApiError, ApiResult};
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
const DENOMINATOR_ZERO: &str = "denominator_zero";
const NOT_MEASURED: &str = "not_measured";

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FactoryMetricsQuery {
    /// Start of the window (RFC 3339); absent means all time.
    since: Option<String>,
}

/// A ratio with the counts it came from.
#[derive(Debug, Serialize, ToSchema)]
pub struct Ratio {
    pub numerator: i64,
    pub denominator: i64,
    /// `null` when `null_reason` is set.
    pub value: Option<f64>,
    /// `denominator_zero` or `not_measured`; absent when `value` is present.
    pub null_reason: Option<String>,
}

/// Median and p90 of a human wait, in minutes. The start of each wait is
/// `viewed_at` when a human opened it, else `created_at`; the counts say which.
#[derive(Debug, Serialize, ToSchema)]
pub struct HumanMinutes {
    /// Resolved rows measured.
    pub measured: i64,
    /// Of those, waits that start at `viewed_at`.
    pub start_viewed_at: i64,
    /// Of those, waits that start at `created_at` (never viewed).
    pub start_created_at: i64,
    pub median_minutes: Option<f64>,
    /// Nearest-rank 90th percentile.
    pub p90_minutes: Option<f64>,
    /// `not_measured` when no row was resolved in the window.
    pub null_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerificationTax {
    /// (verification + rework) ÷ implementation.
    pub ratio: Ratio,
    /// Tokens in + out of each request's first attempt (requests that are not rework).
    pub implementation_tokens: i64,
    pub implementation_attempts: i64,
    /// Tokens of later attempts plus every attempt of a request whose
    /// `metadata.rework_of` names another request.
    pub rework_tokens: i64,
    pub rework_attempts: i64,
    /// Tokens in + out of every readable MRP's `usage`.
    pub verification_tokens: i64,
    pub verification_packs: i64,
    /// Packs whose content is missing or does not carry token usage.
    pub verification_packs_unmeasured: i64,
    /// Attempts counted above whose usage carries no token value.
    pub attempts_unmeasured: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Outcomes {
    /// Pull requests opened in the window, and what each is now.
    pub opened: i64,
    pub merged: i64,
    pub closed: i64,
    pub reverted: i64,
    /// Merged and not reverted.
    pub pqc: i64,
    /// `pqc` ÷ `opened`.
    pub pqc_rate: Ratio,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FactoryMetrics {
    pub since: Option<String>,
    pub attempts: i64,
    pub decisions: i64,
    /// decisions ÷ attempts.
    pub escalation_rate: Ratio,
    pub human_minutes_per_decision: HumanMinutes,
    pub human_minutes_per_mrp: HumanMinutes,
    pub verification_tax: VerificationTax,
    /// Packs produced in the window and not yet reviewed.
    pub mrp_produced: i64,
    pub mrp_unreviewed: i64,
    /// accepted ÷ reviewed.
    pub mrp_acceptance_rate: Ratio,
    pub outcomes: Outcomes,
}

fn ratio(numerator: i64, denominator: i64, null_reason: &str) -> Ratio {
    if denominator == 0 {
        return Ratio {
            numerator,
            denominator,
            value: None,
            null_reason: Some(null_reason.into()),
        };
    }
    Ratio {
        numerator,
        denominator,
        value: Some(numerator as f64 / denominator as f64),
        null_reason: None,
    }
}

fn minutes_between(start: &str, end: &str) -> Option<f64> {
    let start = DateTime::parse_from_rfc3339(start).ok()?;
    let end = DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_milliseconds() as f64 / 60_000.0)
}

fn human_minutes(waits: &[WaitRow]) -> HumanMinutes {
    let mut minutes = Vec::new();
    let (mut viewed, mut created) = (0, 0);
    for wait in waits {
        let (start, from_viewed) = match &wait.viewed_at {
            Some(viewed_at) => (viewed_at, true),
            None => (&wait.created_at, false),
        };
        if let Some(m) = minutes_between(start, &wait.resolved_at) {
            minutes.push(m);
            if from_viewed {
                viewed += 1;
            } else {
                created += 1;
            }
        }
    }
    minutes.sort_by(f64::total_cmp);
    let n = minutes.len();
    let (median, p90) = if n == 0 {
        (None, None)
    } else {
        let median = if n % 2 == 1 {
            minutes[n / 2]
        } else {
            (minutes[n / 2 - 1] + minutes[n / 2]) / 2.0
        };
        let rank = (n as f64 * 0.9).ceil() as usize;
        (Some(median), Some(minutes[rank - 1]))
    };
    HumanMinutes {
        measured: n as i64,
        start_viewed_at: viewed,
        start_created_at: created,
        median_minutes: median,
        p90_minutes: p90,
        null_reason: (n == 0).then(|| NOT_MEASURED.into()),
    }
}

/// Tokens in + out of a pack's `usage`, or `None` when the pack cannot be read.
async fn pack_tokens(storage: &ArtifactStorage, reference: Option<&str>) -> Option<i64> {
    let mut file = storage.open_for_read(reference?).await.ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    let pack: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let usage = pack.get("usage")?;
    Some(usage.get("tokens_in")?.as_i64()? + usage.get("tokens_out")?.as_i64()?)
}

/// GET /api/projects/:id/metrics/factory
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/metrics/factory",
    tag = "metrics",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        FactoryMetricsQuery,
    ),
    responses(
        (status = 200, description = "Factory metrics measured from the project's rows", body = FactoryMetrics),
        (status = 400, description = "`since` is not RFC 3339", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn factory_metrics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<FactoryMetricsQuery>,
) -> ApiResult<Json<FactoryMetrics>> {
    state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    let since = match query.since {
        Some(raw) => Some(
            DateTime::parse_from_rfc3339(&raw)
                .map_err(|e| ApiError::BadRequest(format!("`since` must be RFC 3339: {e}")))?
                .with_timezone(&Utc)
                .to_rfc3339(),
        ),
        None => None,
    };
    let since = since.as_deref();
    let project = id.to_string();
    let pool = state.repo.pool();

    let totals = rows::attempt_totals(pool, &project, since).await?;
    let decision_waits = rows::decision_waits(pool, &project, since).await?;
    let mrp_waits = rows::mrp_waits(pool, &project, since).await?;
    let mrp = rows::mrp_totals(pool, &project, MRP_MEDIA_TYPE, since).await?;
    let prs = rows::pull_request_totals(pool, &project, since).await?;

    let storage = ArtifactStorage::new(format!("{}/execution-artifacts", state.config.storage_dir));
    let (mut verification_tokens, mut verification_packs, mut unmeasured_packs) = (0, 0, 0);
    for reference in rows::mrp_pack_references(pool, &project, MRP_MEDIA_TYPE, since).await? {
        match pack_tokens(&storage, reference.as_deref()).await {
            Some(tokens) => {
                verification_tokens += tokens;
                verification_packs += 1;
            }
            None => unmeasured_packs += 1,
        }
    }

    let implementation = totals.implementation;
    let rework = totals.rework;
    // An implementation side that has attempts but no token value is
    // unmeasured, not a zero denominator.
    let tax_reason =
        if implementation.attempts > 0 && implementation.unmeasured == implementation.attempts {
            NOT_MEASURED
        } else {
            DENOMINATOR_ZERO
        };

    Ok(Json(FactoryMetrics {
        since: since.map(str::to_owned),
        attempts: totals.attempts,
        decisions: totals.decisions,
        escalation_rate: ratio(totals.decisions, totals.attempts, DENOMINATOR_ZERO),
        human_minutes_per_decision: human_minutes(&decision_waits),
        human_minutes_per_mrp: human_minutes(&mrp_waits),
        verification_tax: VerificationTax {
            ratio: ratio(
                verification_tokens + rework.tokens,
                implementation.tokens,
                tax_reason,
            ),
            implementation_tokens: implementation.tokens,
            implementation_attempts: implementation.attempts,
            rework_tokens: rework.tokens,
            rework_attempts: rework.attempts,
            verification_tokens,
            verification_packs,
            verification_packs_unmeasured: unmeasured_packs,
            attempts_unmeasured: implementation.unmeasured + rework.unmeasured,
        },
        mrp_produced: mrp.produced,
        mrp_unreviewed: mrp.produced - mrp.reviewed,
        mrp_acceptance_rate: ratio(mrp.accepted, mrp.reviewed, DENOMINATOR_ZERO),
        outcomes: Outcomes {
            opened: prs.opened,
            merged: prs.merged,
            closed: prs.closed,
            reverted: prs.reverted,
            pqc: prs.merged,
            pqc_rate: ratio(prs.merged, prs.opened, DENOMINATOR_ZERO),
        },
    }))
}
