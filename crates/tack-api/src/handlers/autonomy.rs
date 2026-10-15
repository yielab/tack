//! A project's autonomy settings: how far Tack may merge on its own, where, and
//! under which checks (ADR 0079). Promotion is the PUT; demotion is the tick's.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_core::autonomy::{Domain, Level};
use tack_db::repo::autonomy::{self as store, Settings};
use tack_db::repo::budgets::ProjectBudgets;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

/// One named area of the repository with its own level.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DomainSettings {
    pub name: String,
    /// Globs a change's every path must match (`**` crosses directories).
    pub path_globs: Vec<String>,
    /// Item types the domain covers; empty means any.
    #[serde(default)]
    pub item_types: Vec<String>,
    /// `L3`, `L4` or `L5`.
    pub level: String,
    /// Read only: changes Tack merged by itself in this domain.
    #[serde(default)]
    #[schema(read_only)]
    pub merged: i64,
    /// Read only: of those, how many were reverted or turned CI red (each one
    /// took the domain down a level).
    #[serde(default)]
    #[schema(read_only)]
    pub undone: i64,
}

/// The project's daily limits; `null` is no limit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BudgetSettings {
    pub tokens_per_day: Option<i64>,
    pub usd_per_day: Option<f64>,
    pub auto_merges_per_day: Option<i64>,
    pub queue_items_max: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AutonomySettings {
    /// `L3` (a person merges), `L4` (Tack merges inside the domains) or `L5` (everywhere).
    pub level: String,
    /// Replaces the project's domains.
    pub domains: Vec<DomainSettings>,
    /// The verification stages prill must require before Tack merges alone.
    pub verify_require: Vec<String>,
    /// PEM public key the signed pack is checked against.
    pub attestation_public_key: Option<String>,
    /// How many times a rejected change goes back to the agent; 0 is never.
    pub verify_loop_max_attempts: i64,
    pub budgets: BudgetSettings,
    /// Merge when a person moves the task to Done.
    pub merge_on_done: bool,
    /// Read only: why Tack could not merge by itself here, whatever the level
    /// (GitHub polling off, no prill on the board, no public key); empty when
    /// nothing on this board stops it.
    #[serde(default)]
    #[schema(read_only)]
    pub automatic_merge_blocked_by: Vec<String>,
    /// Read only: the most autonomy the teams this project's roles use allow,
    /// `L5`, `L4` or `L3`.
    #[serde(default = "full_ceiling")]
    #[schema(read_only)]
    pub ceiling: String,
    /// Read only: the teams that cap it, one line each.
    #[serde(default)]
    #[schema(read_only)]
    pub teams: Vec<TeamLine>,
}

fn full_ceiling() -> String {
    Level::L5.to_string()
}

/// A team a role's profile runs that needs a person during a run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamLine {
    pub name: String,
    /// The Tack agent profile that names the team.
    pub profile: String,
    /// The level the team's tasks stay at.
    pub caps_at: String,
    /// What to change so the team runs lights-out.
    pub fix: String,
}

impl From<Settings> for AutonomySettings {
    fn from(s: Settings) -> Self {
        AutonomySettings {
            level: s.level.to_string(),
            domains: s
                .domains
                .into_iter()
                .map(|d| DomainSettings {
                    name: d.name,
                    path_globs: d.path_globs,
                    item_types: d.item_types,
                    level: d.level.to_string(),
                    merged: 0,
                    undone: 0,
                })
                .collect(),
            verify_require: s.verify_require,
            attestation_public_key: s.attestation_public_key,
            verify_loop_max_attempts: s.verify_loop_max_attempts,
            budgets: BudgetSettings {
                tokens_per_day: s.budgets.tokens_per_day,
                usd_per_day: s.budgets.usd_per_day,
                auto_merges_per_day: s.budgets.auto_merges_per_day,
                queue_items_max: s.budgets.queue_items_max,
            },
            merge_on_done: s.merge_on_done,
            automatic_merge_blocked_by: Vec::new(),
            ceiling: full_ceiling(),
            teams: Vec::new(),
        }
    }
}

fn level(text: &str) -> ApiResult<Level> {
    text.parse().map_err(ApiError::BadRequest)
}

fn settings_of(input: AutonomySettings) -> ApiResult<Settings> {
    let mut domains = Vec::new();
    for d in input.domains {
        if d.name.trim().is_empty() {
            return Err(ApiError::BadRequest("a domain needs a name".into()));
        }
        if domains.iter().any(|seen: &Domain| seen.name == d.name) {
            return Err(ApiError::BadRequest(format!(
                "domain \"{}\" appears twice",
                d.name
            )));
        }
        domains.push(Domain {
            level: level(&d.level)?,
            name: d.name,
            path_globs: d.path_globs,
            item_types: d.item_types,
        });
    }
    let b = input.budgets;
    let negative = [b.tokens_per_day, b.auto_merges_per_day, b.queue_items_max]
        .into_iter()
        .flatten()
        .any(|n| n < 0)
        || b.usd_per_day.is_some_and(|n| n < 0.0)
        || input.verify_loop_max_attempts < 0;
    if negative {
        return Err(ApiError::BadRequest(
            "budgets and the loop cap cannot be negative".into(),
        ));
    }
    Ok(Settings {
        level: level(&input.level)?,
        domains,
        verify_require: input.verify_require,
        attestation_public_key: input
            .attestation_public_key
            .filter(|key| !key.trim().is_empty()),
        verify_loop_max_attempts: input.verify_loop_max_attempts,
        budgets: ProjectBudgets {
            tokens_per_day: b.tokens_per_day,
            usd_per_day: b.usd_per_day,
            auto_merges_per_day: b.auto_merges_per_day,
            queue_items_max: b.queue_items_max,
        },
        merge_on_done: input.merge_on_done,
    })
}

/// GET /api/projects/:id/autonomy
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/autonomy",
    tag = "autonomy",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project's autonomy settings", body = AutonomySettings),
        (status = 404, description = "No such project", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_autonomy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AutonomySettings>> {
    Ok(Json(current(&state, &id.to_string()).await?))
}

/// The teams the project's roles run that an active runner reports attended.
async fn attended_teams(state: &AppState, project_id: &str) -> ApiResult<Vec<TeamLine>> {
    let pool = state.repo.pool();
    let profiles = store::role_profile_teams(pool, project_id).await?;
    if profiles.iter().all(|(_, team)| team.is_none()) {
        return Ok(Vec::new());
    }
    let reported: Vec<_> = store::active_runner_snapshots(pool)
        .await?
        .iter()
        .flat_map(|snapshot| crate::autonomy::harnesses_of(snapshot))
        .flat_map(|harness| harness.teams.unwrap_or_default())
        .collect();
    let mut lines = Vec::new();
    for (profile, team) in profiles {
        let Some(team) = team else { continue };
        let Some(report) = reported
            .iter()
            .find(|t| t.name == team && crate::autonomy::is_attended(t))
        else {
            continue;
        };
        if lines.iter().any(|line: &TeamLine| line.name == team) {
            continue;
        }
        lines.push(TeamLine {
            name: team,
            profile,
            caps_at: Level::L3.to_string(),
            fix: report.violations.first().map_or_else(
                || "run it lights-out".to_owned(),
                |violation| violation.fix.clone(),
            ),
        });
    }
    Ok(lines)
}

/// The settings as they stand, with everything only the board can read.
async fn current(state: &AppState, project_id: &str) -> ApiResult<AutonomySettings> {
    let mut settings: AutonomySettings = store::get_settings(state.repo.pool(), project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".into()))?
        .into();
    let counts = store::domain_counts(state.repo.pool(), project_id).await?;
    for domain in &mut settings.domains {
        if let Some((merged, undone)) = counts.get(&domain.name) {
            (domain.merged, domain.undone) = (*merged, *undone);
        }
    }
    let mut blocked = Vec::new();
    if state.config.github_poll_seconds == 0 || state.config.github_token.is_none() {
        blocked.push(
            "GitHub polling is off on this board, so Tack cannot read CI or merge".to_owned(),
        );
    }
    if let Err(reason) = crate::prill_store::version(&state.config.prill_program).await {
        blocked.push(format!("{reason}, so a signed pack cannot be checked"));
    }
    if settings.attestation_public_key.is_none() {
        blocked.push("The project has no public key to check prill's signature against".to_owned());
    }
    let teams = attended_teams(state, project_id).await?;
    for team in &teams {
        blocked.push(format!(
            "The team {} runs attended: its tasks stay at {}. Fix: {}",
            team.name, team.caps_at, team.fix
        ));
    }
    if !teams.is_empty() {
        settings.ceiling = Level::L3.to_string();
    }
    settings.teams = teams;
    settings.automatic_merge_blocked_by = blocked;
    Ok(settings)
}

/// PUT /api/projects/:id/autonomy
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/projects/{id}/autonomy",
    tag = "autonomy",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = AutonomySettings,
    responses(
        (status = 200, description = "The settings now in force", body = AutonomySettings),
        (status = 400, description = "An unknown level, a nameless or repeated domain, a negative limit", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "No such project", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_autonomy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<AutonomySettings>,
) -> ApiResult<Json<AutonomySettings>> {
    let settings = settings_of(input)?;
    if !store::put_settings(state.repo.pool(), &id.to_string(), &settings).await? {
        return Err(ApiError::NotFound("Project not found".into()));
    }
    Ok(Json(current(&state, &id.to_string()).await?))
}
