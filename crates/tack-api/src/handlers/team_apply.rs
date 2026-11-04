//! Applying the repository's teams from the board (ADR 0036, decision 8). The board has no
//! checkout and no line to docket: it asks a runner of the agent that has them
//! (`TeamOperation::ApplyTeam`, queued like resuming a team), and the runner makes the team and
//! applies the project's `.docket/teams/<team>` folder. When docket wants a person to read the
//! commands that folder would run, the answer carries them with docket's code; the person sends
//! the same request again with the code as `accept`. Nothing is kept between the two calls.
//! Tack writes no recipe, service, secret or quota (ADR 0030, decision 5).

use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_core::models::CodeOrigin;
use tack_core::outline::TEAM_SHAPES;
use tack_core::outline::bundle::is_team_name;
use tack_protocol::execution::{HarnessKind, OperationOutcome, TeamOperation};

use super::teams::reported_harnesses;
use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path};
use crate::router::AppState;

/// The agent that has teams of a repository's own.
const DOCKET: &str = "docket";

/// The feature of docket's API that makes a pod from a named shape and applies a folder to it.
pub const POD_SHAPE: &str = "podShape";

/// How long one team waits for the runner, which asks for what waits for it every few seconds.
const RUNNER_ANSWER_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);

/// One team to apply: a row of the import preview's `missing` list, or one of the file's `teams`.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct TeamToApply {
    /// The team's name, which is also the name of its folder under `.docket/teams/`.
    pub name: String,
    /// The shape the team is made from: `build`, `respond`, `operate` or `routine`.
    pub shape: String,
    /// The code docket gave for the commands the folder would run, once a person has read them:
    /// the answer to an earlier `needs_review` for this team.
    #[serde(default)]
    pub accept: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ApplyTeamsRequest {
    pub teams: Vec<TeamToApply>,
}

/// How applying one team went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamApplyState {
    /// The team exists and the folder is applied.
    Applied,
    /// docket would run commands on the computer and waits for a person to read them: send the
    /// team again with `review.code` as `accept`.
    NeedsReview,
    /// It was not done; `message` says why.
    Refused,
}

/// What docket asks a person to read before it applies the folder.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamApplyReview {
    /// The code to send back as `accept`; it changes when the folder does.
    pub code: String,
    /// What the folder would run on the computer, one line each, as docket listed it.
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamApplyResult {
    pub team: String,
    pub state: TeamApplyState,
    /// Present with `needs_review`.
    pub review: Option<TeamApplyReview>,
    /// Why it was refused.
    pub message: Option<String>,
}

/// POST /api/projects/{id}/teams/apply
///
/// Making teams on a person's computer is a person's call, so the route is the operator's alone
/// (`agent_access`).
#[instrument(skip(state, request))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/teams/apply",
    tag = "teams",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ApplyTeamsRequest,
    responses(
        (status = 200, description = "How each team went, in the order asked: applied, waiting for a person to read the commands docket listed, or refused with the reason", body = Vec<TeamApplyResult>),
        (status = 400, description = "No team named, a name or shape docket does not know, or the project's code is not a folder on a computer", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "No such project", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "No active runner reports a docket that lists `podShape`; the sentence says what to do", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn apply_teams(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApplyTeamsRequest>,
) -> ApiResult<Json<Vec<TeamApplyResult>>> {
    let project = state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    if request.teams.is_empty() {
        return Err(ApiError::BadRequest(
            "Name at least one team to apply".into(),
        ));
    }
    for team in &request.teams {
        if !is_team_name(&team.name) {
            return Err(ApiError::BadRequest(format!(
                "{:?} is not a team name docket accepts: lowercase letters, digits and hyphens, up to 64",
                team.name
            )));
        }
        if !TEAM_SHAPES.contains(&team.shape.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "{:?} is not a shape of team; choose one of {}",
                team.shape,
                TEAM_SHAPES.join(", ")
            )));
        }
    }
    let directory = match (project.code_origin, project.repository.as_deref()) {
        (CodeOrigin::Folder | CodeOrigin::NewFolder, Some(path)) if !path.is_empty() => {
            path.to_owned()
        }
        _ => {
            return Err(ApiError::BadRequest(
                "A team is applied from the project's folder on a computer; this project has no \
                 folder. Set the project's folder first."
                    .into(),
            ));
        }
    };
    // The newest report among the runners whose docket can make a team from a shape.
    let asked = reported_harnesses(&state)
        .await?
        .into_iter()
        .filter(|reported| {
            reported.kind == DOCKET && reported.capability.lists_api_feature(POD_SHAPE)
        })
        .max_by_key(|reported| reported.capability.probed_at)
        .ok_or_else(|| {
            ApiError::Conflict(format!(
                "No computer reports a docket that can make a team from the board: it must list \
                 {POD_SHAPE}. Update docket on the computer that holds the project's folder, or \
                 make the team there with the command shown beside it."
            ))
        })?;

    let mut results = Vec::with_capacity(request.teams.len());
    // A runner that did not answer one team will not answer the next within a request's patience.
    let mut silent = None;
    for team in request.teams {
        let result = |state, review, message| TeamApplyResult {
            team: team.name.clone(),
            state,
            review,
            message,
        };
        if let Some(sentence) = &silent {
            results.push(result(
                TeamApplyState::Refused,
                None,
                Some(String::clone(sentence)),
            ));
            continue;
        }
        let outcome = tack_app::runner_operations::operations()
            .ask(
                &asked.runner_id,
                TeamOperation::ApplyTeam {
                    harness: HarnessKind::new(DOCKET),
                    team: team.name.clone(),
                    shape: team.shape.clone(),
                    directory: directory.clone(),
                    accept: team.accept.clone().filter(|code| !code.trim().is_empty()),
                },
                RUNNER_ANSWER_WITHIN,
            )
            .await;
        results.push(match outcome {
            Ok(OperationOutcome::Done { .. }) => result(TeamApplyState::Applied, None, None),
            Ok(OperationOutcome::NeedsReview { code, commands }) => result(
                TeamApplyState::NeedsReview,
                Some(TeamApplyReview { code, commands }),
                None,
            ),
            Ok(OperationOutcome::Refused { reason }) => {
                result(TeamApplyState::Refused, None, Some(reason))
            }
            Err(_) => {
                let sentence = format!(
                    "The runner {} did not answer in time. It may be busy or offline; try again in a moment.",
                    asked.runner
                );
                silent = Some(sentence.clone());
                result(TeamApplyState::Refused, None, Some(sentence))
            }
        });
    }
    Ok(Json(results))
}
