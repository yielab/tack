//! A group follows the work under it and moves nothing (ADR 0036, decisions 7 and 11): how far the
//! work is, and whether the last of its runs has ended. Pure; `tack_db::queries::children` reads
//! the facts.

use chrono::{DateTime, Utc};

use crate::models::Progress;
use crate::workflow::WorkflowConfig;

/// What the work under a group that is in one status says, added up. The store folds work by
/// status because the facts of a group's whole subtree are read at once and a tree's top group
/// can hold tens of thousands of items; the workflow, which says what done is, stays here.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    pub status: String,
    pub count: u32,
    /// The estimates of this work added up; `None` when none of it has an estimate.
    pub estimate: Option<f64>,
    /// The earliest start: when the work started, else when it was created.
    pub starts: Option<DateTime<Utc>>,
    /// The latest due date.
    pub due: Option<DateTime<Utc>>,
    /// The teams its latest runs went to (or the single agent when a run had no team), each once.
    pub teams: Vec<String>,
    /// How much of it has a run queued, running or waiting for a person.
    pub open_runs: u32,
    /// How much of it asks a person for an answer or a decision.
    pub needs_you: u32,
}

/// The progress of the work under a group, or `None` when there is none. Done is a status in the
/// workflow's done category; the teams are those of the latest runs, each once, sorted.
pub fn progress(tallies: &[Tally], workflow: &WorkflowConfig) -> Option<Progress> {
    let total: u32 = tallies.iter().map(|t| t.count).sum();
    if total == 0 {
        return None;
    }
    let done = |t: &&Tally| workflow.is_done_status(&t.status);
    let estimated = tallies.iter().any(|t| t.estimate.is_some());
    let mut teams: Vec<String> = tallies.iter().flat_map(|t| t.teams.clone()).collect();
    teams.sort();
    teams.dedup();
    Some(Progress {
        total,
        done: tallies.iter().filter(done).map(|t| t.count).sum(),
        estimate_total: estimated.then(|| tallies.iter().filter_map(|t| t.estimate).sum()),
        estimate_done: estimated.then(|| {
            tallies
                .iter()
                .filter(done)
                .filter_map(|t| t.estimate)
                .sum()
        }),
        starts: tallies.iter().filter_map(|t| t.starts).min(),
        ends: tallies.iter().filter_map(|t| t.due).max(),
        teams,
        open_runs: tallies.iter().map(|t| t.open_runs).sum(),
        needs_you: tallies.iter().map(|t| t.needs_you).sum(),
    })
}

/// Whether a run that just ended was the last one under its group: there is work and none of it
/// has a run open. The group's status is not asked: the person moves it.
pub fn runs_settled(tallies: &[Tally]) -> bool {
    tallies.iter().any(|t| t.count > 0) && tallies.iter().all(|t| t.open_runs == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::preset_workflow;
    use chrono::TimeZone;

    fn day(d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, d, 9, 0, 0).unwrap()
    }

    fn tally(status: &str, count: u32) -> Tally {
        Tally {
            status: status.to_owned(),
            count,
            estimate: None,
            starts: Some(day(1)),
            due: None,
            teams: Vec::new(),
            open_runs: 0,
            needs_you: 0,
        }
    }

    fn workflow() -> WorkflowConfig {
        preset_workflow("simple").expect("simple")
    }

    #[test]
    fn no_work_is_no_progress() {
        assert_eq!(progress(&[], &workflow()), None);
        assert_eq!(progress(&[tally("To Do", 0)], &workflow()), None);
        assert!(!runs_settled(&[]));
    }

    #[test]
    fn progress_counts_done_and_names_each_team_once() {
        let mut done = tally("Done", 1);
        done.teams = vec!["saas-build".into()];
        let mut todo = tally("To Do", 3);
        todo.teams = vec!["claude-code".into(), "saas-build".into()];
        todo.open_runs = 1;
        let p = progress(&[done, todo], &workflow()).expect("progress");
        assert_eq!((p.total, p.done, p.open_runs), (4, 1, 1));
        assert_eq!(p.teams, ["claude-code", "saas-build"]);
    }

    #[test]
    fn points_count_when_any_work_has_an_estimate() {
        let mut done = tally("Done", 2);
        done.estimate = Some(3.0);
        let mut todo = tally("To Do", 1);
        todo.estimate = Some(5.0);
        let p = progress(&[done, todo], &workflow()).expect("progress");
        assert_eq!((p.estimate_total, p.estimate_done), (Some(8.0), Some(3.0)));
    }

    #[test]
    fn no_estimate_anywhere_is_no_points_and_not_zero() {
        let p = progress(&[tally("Done", 1)], &workflow()).expect("progress");
        assert_eq!((p.estimate_total, p.estimate_done), (None, None));
    }

    #[test]
    fn nothing_done_with_estimates_is_zero_points_done() {
        let mut todo = tally("To Do", 1);
        todo.estimate = Some(2.0);
        let p = progress(&[todo], &workflow()).expect("progress");
        assert_eq!((p.estimate_total, p.estimate_done), (Some(2.0), Some(0.0)));
    }

    #[test]
    fn the_span_runs_from_the_earliest_start_to_the_latest_due() {
        let mut first = tally("To Do", 1);
        first.starts = Some(day(3));
        first.due = Some(day(20));
        let mut second = tally("Done", 1);
        second.starts = Some(day(2));
        second.due = Some(day(9));
        let p = progress(&[first, second], &workflow()).expect("progress");
        assert_eq!((p.starts, p.ends), (Some(day(2)), Some(day(20))));
        let p = progress(&[tally("To Do", 1)], &workflow()).expect("progress");
        assert_eq!((p.starts, p.ends), (Some(day(1)), None));
    }

    #[test]
    fn work_that_asks_a_person_is_counted() {
        let mut todo = tally("To Do", 2);
        todo.needs_you = 1;
        assert_eq!(progress(&[todo], &workflow()).unwrap().needs_you, 1);
    }

    #[test]
    fn the_runs_are_settled_when_no_work_has_one_open() {
        let mut items = [tally("Done", 1), tally("To Do", 1)];
        items[1].open_runs = 1;
        assert!(!runs_settled(&items));
        items[1].open_runs = 0;
        assert!(runs_settled(&items));
    }
}
