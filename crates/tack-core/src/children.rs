//! A parent item follows its children and moves nothing (ADR 0036, decision 7): how far they are,
//! and whether the last of their runs has ended. Pure; `tack_db::queries::children` reads the facts.

use crate::models::ChildrenProgress;
use crate::workflow::WorkflowConfig;

/// What a child item says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub status: String,
    /// The team its latest run went to, or the single agent when that run had no team.
    pub team: Option<String>,
    /// A run of it is queued, running or waiting for a person.
    pub open_run: bool,
}

/// The progress of `children`, or `None` for an item without any. Done is a status in the
/// workflow's done category; the teams are those of the children's latest runs, each once, sorted.
pub fn progress(children: &[Child], workflow: &WorkflowConfig) -> Option<ChildrenProgress> {
    if children.is_empty() {
        return None;
    }
    let mut teams: Vec<String> = children.iter().filter_map(|c| c.team.clone()).collect();
    teams.sort();
    teams.dedup();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Some(ChildrenProgress {
        total: count(children.len()),
        done: count(
            children
                .iter()
                .filter(|c| workflow.is_done_status(&c.status))
                .count(),
        ),
        teams,
    })
}

/// Whether a run that just ended was the last one of its parent's children: there are children and
/// none of them has a run open. The parent's status is not asked: the person moves it.
pub fn runs_settled(children: &[Child]) -> bool {
    !children.is_empty() && children.iter().all(|c| !c.open_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::preset_workflow;

    fn child(status: &str, team: Option<&str>, open_run: bool) -> Child {
        Child {
            status: status.to_owned(),
            team: team.map(str::to_owned),
            open_run,
        }
    }

    fn workflow() -> WorkflowConfig {
        preset_workflow("simple").expect("simple")
    }

    #[test]
    fn no_children_is_no_progress() {
        assert_eq!(progress(&[], &workflow()), None);
        assert!(!runs_settled(&[]));
    }

    #[test]
    fn progress_counts_done_and_names_each_team_once() {
        let children = [
            child("Done", Some("saas-build"), false),
            child("To Do", Some("saas-build"), false),
            child("To Do", Some("claude-code"), true),
            child("To Do", None, false),
        ];
        let p = progress(&children, &workflow()).expect("progress");
        assert_eq!((p.total, p.done), (4, 1));
        assert_eq!(p.teams, ["claude-code", "saas-build"]);
    }

    #[test]
    fn the_runs_are_settled_when_no_child_has_one_open() {
        let mut children = [child("Done", None, false), child("To Do", None, true)];
        assert!(!runs_settled(&children));
        children[1].open_run = false;
        assert!(runs_settled(&children));
    }
}
