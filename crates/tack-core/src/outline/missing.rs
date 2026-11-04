//! What a project file expects that no computer reports: a team nobody runs, an agent nobody has.
//! Pure: [`missing`] reads the file and the names the computers report, and answers with each
//! gap and the command or sentence that closes it. An import creates none of it; the list is for
//! the person who reads the preview.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::Outline;

/// docket's shipped recipes: a method of this name is made by `docket init --recipe`, and any
/// other is a pipeline the team's owner writes.
const SHIPPED_RECIPES: [&str; 12] = [
    "cross-family-review",
    "explore-then-edit",
    "frugal",
    "mutation",
    "parallel-explore",
    "reflexion",
    "research-review",
    "secure-build",
    "spec-first",
    "strong-plan-cheap-edit",
    "tdd",
    "web-research",
];

/// What kind of thing is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MissingKind {
    /// A team the file expects and no computer reports.
    Team,
    /// An agent a profile of the file runs and no computer reports.
    Harness,
}

/// One gap between a project file and what the computers report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Missing {
    pub kind: MissingKind,
    /// The team's name, or the agent's.
    pub name: String,
    /// A team's shape, when the file says it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// What the operator does to close the gap: docket's command for a team, the sentence that
    /// installs an agent for a harness.
    pub fix: String,
}

/// docket's command that makes a team of this shape with these methods.
fn init_command(name: &str, shape: Option<&str>, methods: &[String]) -> String {
    let mut command = format!("docket init {name}");
    if let Some(shape) = shape {
        command.push_str(&format!(" --blueprint {shape}"));
    }
    for method in methods
        .iter()
        .filter(|method| SHIPPED_RECIPES.contains(&method.as_str()))
    {
        command.push_str(&format!(" --recipe {method}"));
    }
    command
}

/// The teams the file expects (its `teams`, and the ones its profiles name) that no computer
/// reports, then the agents its profiles and automation run that no computer reports. A team is
/// reported under any agent that has it.
pub fn missing(
    outline: &Outline,
    reported_teams: &BTreeSet<String>,
    reported_harnesses: &BTreeSet<String>,
) -> Vec<Missing> {
    let mut teams: BTreeMap<&str, (Option<&str>, &[String])> = BTreeMap::new();
    for team in &outline.teams {
        teams.insert(
            team.name.trim(),
            (team.shape.as_deref().map(str::trim), &team.methods),
        );
    }
    for profile in &outline.profiles {
        if let Some(name) = profile
            .tool_policy
            .get("docket")
            .and_then(|docket| docket.get("team"))
            .and_then(|team| team.as_str())
        {
            teams.entry(name).or_insert((None, &[]));
        }
    }
    let mut gaps: Vec<Missing> = teams
        .into_iter()
        .filter(|(name, _)| !reported_teams.contains(*name))
        .map(|(name, (shape, methods))| Missing {
            kind: MissingKind::Team,
            name: name.to_owned(),
            shape: shape.map(str::to_owned),
            fix: init_command(name, shape, methods),
        })
        .collect();

    let mut harnesses: BTreeSet<&str> = outline
        .profiles
        .iter()
        .filter_map(|profile| profile.tool_policy.get("agent")?.as_str())
        .collect();
    harnesses.extend(
        outline
            .automation
            .as_ref()
            .and_then(|automation| automation.default_harness.as_deref())
            .map(str::trim),
    );
    gaps.extend(
        harnesses
            .into_iter()
            .filter(|harness| !reported_harnesses.contains(*harness))
            .map(|harness| Missing {
                kind: MissingKind::Harness,
                name: harness.to_owned(),
                shape: None,
                fix: format!(
                    "No computer reports {harness}. Install it on a computer that runs a runner, \
                     then run `tack runner doctor` there to check it."
                ),
            }),
    );
    gaps
}

#[cfg(test)]
#[path = "missing/tests.rs"]
mod tests;
