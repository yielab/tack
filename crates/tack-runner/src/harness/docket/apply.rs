//! Applying a repository's team (`RunnerOperation::ApplyTeam`): make the pod from its shape, then
//! apply the team's folder to it. docket refuses a folder that would run commands on this machine
//! until the call carries the code it gave for them, so the first answer may be a request for a
//! person to read the commands; the call is made again when the operation comes back with the
//! person's code.

use std::path::Path;

use tack_protocol::execution::OperationOutcome;

use super::{client::Client, wire::ApplyAnswer};

/// The feature of `GET /health` under which `POST /pods` takes a `blueprint` and apply reaches
/// the pods of a folder.
pub(super) const POD_SHAPE: &str = "podShape";

/// The one shape whose team works in the repository; the others work in a folder of their own.
const SHAPE_IN_REPOSITORY: &str = "build";

/// Where, inside the project's checkout, a team's folder is kept.
const TEAMS_FOLDER: [&str; 2] = [".docket", "teams"];

pub(super) async fn apply_team(
    client: &Client,
    team: &str,
    shape: &str,
    directory: &str,
    accept: Option<&str>,
) -> OperationOutcome {
    let refused = |reason: String| OperationOutcome::Refused { reason };
    if !client.has_feature(POD_SHAPE).await {
        return refused(format!(
            "This docket does not list {POD_SHAPE}, so it cannot make a team from a shape. \
             Update docket, or make the team with `docket init {team} --blueprint {shape}`."
        ));
    }
    let folder = TEAMS_FOLDER
        .iter()
        .fold(Path::new(directory).to_path_buf(), |path, part| {
            path.join(part)
        })
        .join(team);
    if !folder.is_dir() {
        return refused(format!(
            "The project's folder on this computer has no .docket/teams/{team} folder ({}).",
            folder.display()
        ));
    }
    // A team in a folder must not be given the checkout as its working folder: it would write
    // inside the person's working tree, and read and change its own configuration of record.
    let location = (shape == SHAPE_IN_REPOSITORY).then_some(directory);
    match client.ensure_pod(team, location, Some(shape)).await {
        Ok(reply) if reply.status == 200 || reply.status == 201 => {}
        Ok(reply) => {
            return refused(format!(
                "docket did not make the team {team:?}: {}",
                reply.error()
            ));
        }
        Err(why) => return refused(why),
    }
    let reply = match client
        .apply_pod(team, &folder.display().to_string(), accept)
        .await
    {
        Ok(reply) => reply,
        Err(why) => return refused(why),
    };
    let answer = reply.read::<ApplyAnswer>().unwrap_or(ApplyAnswer {
        review_code: None,
        runs: Vec::new(),
    });
    match (reply.status, answer.review_code) {
        (200, _) => OperationOutcome::Done { team: None },
        (409, Some(code)) => OperationOutcome::NeedsReview {
            code,
            commands: answer
                .runs
                .iter()
                .map(|run| format!("{} {}: {}", run.kind, run.name, run.runs))
                .collect(),
        },
        _ => refused(format!(
            "docket did not apply the team {team:?}: {}",
            reply.error()
        )),
    }
}
