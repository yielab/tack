//! Factory metrics: what the agent fleet spent on a project in attempts, human
//! minutes and tokens, and what became of its pull requests. Every number is
//! computed from rows; every ratio carries its numerator and denominator, and a
//! figure with nothing to measure is `null` with its reason (`denominator_zero`
//! or `not_measured`), never 0. Quality figures are prill's, read from its
//! store and named as prill's. Nothing here is priced: spend is tokens.

use crate::extract::{Json, Path, Query};
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tack_protocol::mrp::MergeDecision;
use tokio::io::AsyncReadExt;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_db::queries::metrics as quality_rows;
use tack_db::repo::metrics::SpendRow;
use tack_db::repo::metrics::{self as rows, WaitRow};

use super::metrics_quality::{WorkQuality, work_quality};
use crate::error::{ApiError, ApiResult};
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
pub(super) const DENOMINATOR_ZERO: &str = "denominator_zero";
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
    /// Counted tokens (cache reads left out) of each request's first attempt (requests that are not rework).
    pub implementation_tokens: i64,
    pub implementation_attempts: i64,
    /// Counted tokens of later attempts plus every attempt of a request whose
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
    /// Merged and not reverted at the time of the query: no CI, defect or
    /// window behind it, so it is not a quality figure (prill's `quality` is).
    pub merged_not_reverted: i64,
}

/// One repository's figures from prill's store.
#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryQuality {
    /// `owner/repo`.
    pub repository: String,
    /// prill's `metrics/v1` document, as prill printed it.
    #[schema(value_type = Object)]
    pub metrics: serde_json::Value,
}

/// What prill measured. Present only when prill answered.
#[derive(Debug, Serialize, ToSchema)]
pub struct Quality {
    /// Always `prill`.
    pub source: String,
    pub prill_version: String,
    pub repositories: Vec<RepositoryQuality>,
}

/// Requests with an attempt in the window, by how they began. Rework wins over the other two.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunsByStart {
    pub person: i64,
    pub auto: i64,
    pub rework: i64,
}

/// Merges in the window, by what triggered them.
#[derive(Debug, Serialize, ToSchema)]
pub struct MergesByTrigger {
    pub button: i64,
    pub done: i64,
    pub auto: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FactoryMetrics {
    pub since: Option<String>,
    pub attempts: i64,
    pub decisions: i64,
    /// decisions ÷ attempts.
    pub escalation_rate: Ratio,
    pub human_minutes_per_decision: HumanMinutes,
    /// Attempts with at least one escalation the agent's policy settled itself, and no
    /// person was asked: a count apart from `decisions`, which only a person's answer makes.
    pub decided_alone: i64,
    /// decided_alone ÷ attempts.
    pub decided_alone_rate: Ratio,
    pub human_minutes_per_mrp: HumanMinutes,
    pub verification_tax: VerificationTax,
    /// Packs produced in the window and not yet reviewed.
    pub mrp_produced: i64,
    pub mrp_unreviewed: i64,
    /// accepted ÷ reviewed.
    pub mrp_acceptance_rate: Ratio,
    pub outcomes: Outcomes,
    pub runs_by_start: RunsByStart,
    pub merges_by_trigger: MergesByTrigger,
    /// Decisions created in the window that a person has answered.
    pub decisions_answered: i64,
    /// Tasks merged in the window whose newest merged run began by itself and whose
    /// requests never had a resolved decision, over the tasks merged in the window.
    pub lights_off_rate: Ratio,
    /// prill's figures; absent when `quality_unavailable` says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_unavailable: Option<String>,
    /// Tokens and attempts per model.
    pub cost_by_model: Vec<CostByModel>,
    /// Tokens and attempts per docket team; `No team` for runs that name none.
    pub cost_by_team: Vec<CostByTeam>,
    /// How well the work came out: rates over the window's attempts, and the same cut by who did it.
    pub work_quality: WorkQuality,
}

/// One model's tokens in the window. `tokens_in` and `tokens_out` are counted
/// tokens: the input the cache served is in `cache_read`, shown and not counted.
#[derive(Debug, Serialize, ToSchema)]
pub struct CostByModel {
    /// Absent for attempts that reported no model and asked for none.
    pub model: Option<String>,
    pub tokens_in: i64,
    pub cache_read: i64,
    pub tokens_out: i64,
    pub attempts: i64,
}

/// One docket team's tokens in the window, counted as in [`CostByModel`].
#[derive(Debug, Serialize, ToSchema)]
pub struct CostByTeam {
    pub team: String,
    pub tokens_in: i64,
    pub cache_read: i64,
    pub tokens_out: i64,
    pub attempts: i64,
}

const NO_TEAM: &str = "No team";

struct CostLine {
    key: Option<String>,
    /// Every input token, cache reads included.
    tokens_in: i64,
    cache_read: i64,
    tokens_out: i64,
    attempts: i64,
}

/// The spend rows folded by `key`.
fn cost_lines(spend: &[SpendRow], key: impl Fn(&SpendRow) -> Option<String>) -> Vec<CostLine> {
    let mut lines: std::collections::BTreeMap<Option<String>, CostLine> = Default::default();
    for row in spend {
        let k = key(row);
        let line = lines.entry(k.clone()).or_insert(CostLine {
            key: k,
            tokens_in: 0,
            cache_read: 0,
            tokens_out: 0,
            attempts: 0,
        });
        line.tokens_in += row.tokens_in;
        line.cache_read += row.cache_read;
        line.tokens_out += row.tokens_out;
        line.attempts += row.attempts;
    }
    lines.into_values().collect()
}

pub(super) fn ratio(numerator: i64, denominator: i64, null_reason: &str) -> Ratio {
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
/// The pack contract carries no cache figure, so these are its counted tokens.
async fn pack_tokens(storage: &ArtifactStorage, reference: Option<&str>) -> Option<i64> {
    let mut file = storage.open_for_read(reference?).await.ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    let pack: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let usage = pack.get("usage")?;
    Some(usage.get("tokens_in")?.as_i64()? + usage.get("tokens_out")?.as_i64()?)
}

/// prill's figures for each repository the project opened pull requests in
/// that has a store, or why there are none.
async fn prill_quality(
    state: &AppState,
    project: &str,
) -> Result<(String, Vec<(String, serde_json::Value)>), String> {
    let prill = &state.prill().await;
    let version = crate::prill_store::version(prill).await?;
    let repos = tack_db::queries::pull_requests::repos_of_project(&state.repo, project)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not list the project's repositories");
            "the board could not list the project's repositories".to_owned()
        })?;
    let mut found = Vec::new();
    let mut failure = None;
    for repo in repos {
        let Some(db) = crate::prill_store::store_path(&state.config.storage_dir, &repo) else {
            continue;
        };
        if !db.is_file() {
            continue;
        }
        match crate::prill_store::metrics(prill, &db).await {
            Ok(metrics) => found.push((repo, metrics)),
            Err(reason) => failure = Some(reason),
        }
    }
    if found.is_empty() {
        return Err(failure.unwrap_or_else(|| "no repository has a prill store yet".to_owned()));
    }
    Ok((version, found))
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
    let repo = &state.repo;

    let totals = rows::attempt_totals(repo, &project, since).await?;
    let decision_waits = rows::decision_waits(repo, &project, since).await?;
    let mrp_waits = rows::mrp_waits(repo, &project, since).await?;
    let mrp = rows::mrp_totals(repo, &project, MRP_MEDIA_TYPE, since).await?;
    let prs = rows::pull_request_totals(repo, &project, since).await?;
    let starts = rows::runs_by_start(repo, &project, since).await?;
    let triggers = rows::merges_by_trigger(repo, &project, since).await?;
    let (lights_off, merged_items) = rows::lights_off(repo, &project, since).await?;

    let storage = ArtifactStorage::new(format!("{}/execution-artifacts", state.config.storage_dir));
    let (mut verification_tokens, mut verification_packs, mut unmeasured_packs) = (0, 0, 0);
    for reference in rows::mrp_pack_references(repo, &project, MRP_MEDIA_TYPE, since).await? {
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

    let spend = tack_db::repo::metrics::spend_by_model(repo, &project, since).await?;
    let cost_by_team = cost_lines(&spend, |row| {
        Some(row.team.clone().unwrap_or_else(|| NO_TEAM.into()))
    })
    .into_iter()
    .map(|line| CostByTeam {
        team: line.key.unwrap_or_default(),
        tokens_in: line.tokens_in - line.cache_read,
        cache_read: line.cache_read,
        tokens_out: line.tokens_out,
        attempts: line.attempts,
    })
    .collect();
    let cost_by_model = cost_lines(&spend, |row| row.model.clone())
        .into_iter()
        .map(|line| CostByModel {
            model: line.key,
            tokens_in: line.tokens_in - line.cache_read,
            cache_read: line.cache_read,
            tokens_out: line.tokens_out,
            attempts: line.attempts,
        })
        .collect();
    let work_quality = quality_figures(&state, &storage, &project, since).await?;
    let (quality, quality_unavailable) = match prill_quality(&state, &project).await {
        Ok((version, found)) => {
            let repositories = found
                .into_iter()
                .map(|(repository, metrics)| RepositoryQuality {
                    repository,
                    metrics,
                })
                .collect();
            (
                Some(Quality {
                    source: "prill".into(),
                    prill_version: version,
                    repositories,
                }),
                None,
            )
        }
        Err(reason) => (None, Some(reason)),
    };

    Ok(Json(FactoryMetrics {
        since: since.map(str::to_owned),
        attempts: totals.attempts,
        decisions: totals.decisions,
        escalation_rate: ratio(totals.decisions, totals.attempts, DENOMINATOR_ZERO),
        human_minutes_per_decision: human_minutes(&decision_waits),
        decided_alone: totals.decided_alone,
        decided_alone_rate: ratio(totals.decided_alone, totals.attempts, DENOMINATOR_ZERO),
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
            merged_not_reverted: prs.merged,
        },
        runs_by_start: RunsByStart {
            person: starts.person,
            auto: starts.auto,
            rework: starts.rework,
        },
        merges_by_trigger: MergesByTrigger {
            button: triggers.button,
            done: triggers.done,
            auto: triggers.auto,
        },
        decisions_answered: i64::try_from(decision_waits.len()).unwrap_or(i64::MAX),
        lights_off_rate: ratio(lights_off, merged_items, DENOMINATOR_ZERO),
        quality,
        quality_unavailable,
        cost_by_model,
        cost_by_team,
        work_quality,
    }))
}

/// The pack's word on each labelled attempt in `attempts` that left a readable one.
async fn pack_decisions(
    storage: &ArtifactStorage,
    attempts: &[quality_rows::QualityAttempt],
) -> HashMap<String, MergeDecision> {
    let mut decisions = HashMap::new();
    for attempt in attempts {
        let labelled =
            attempt.verdict.is_some() || attempt.pull_request.as_deref() == Some("reverted");
        let Some(reference) = attempt.pack_reference.as_deref().filter(|_| labelled) else {
            continue;
        };
        let Ok(mut file) = storage.open_for_read(reference).await else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).await.is_err() {
            continue;
        }
        let word = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|pack| pack.pointer("/recommendation/decision").cloned())
            .and_then(|word| serde_json::from_value::<MergeDecision>(word).ok());
        if let Some(word) = word {
            decisions.insert(attempt.attempt_id.clone(), word);
        }
    }
    decisions
}

async fn quality_figures(
    state: &AppState,
    storage: &ArtifactStorage,
    project: &str,
    since: Option<&str>,
) -> ApiResult<WorkQuality> {
    let repo = &state.repo;
    let attempts = quality_rows::quality_attempts(repo, project, MRP_MEDIA_TYPE, since).await?;
    let questions = quality_rows::question_rows(repo, project, since).await?;
    let actions = quality_rows::action_counts(repo, project, since).await?;
    let packs = pack_decisions(storage, &attempts).await;
    Ok(work_quality(&attempts, &packs, &questions, &actions))
}
