//! Which tasks a project that starts its own work starts now.

use uuid::Uuid;

use crate::models::Priority;
use crate::run_defaults::Resolved;

/// The system comment a demotion to L3 leaves on the item whose merge caused it.
pub const DROPPED_TO_L3: &str = "Auto-start is off: the project dropped to L3";

#[derive(Debug, Clone)]
pub struct Candidate {
    pub item_id: Uuid,
    pub priority: Priority,
    pub sort_order: i32,
    pub version: i64,
    pub has_open_request: bool,
    pub unfinished_blockers: usize,
    pub profile: Option<Resolved>,
    /// A fleet, or exactly one active runner, to run on.
    pub has_machine: bool,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_open: usize,
    pub open_now: usize,
    pub budget_reached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    OpenRequest,
    Blocked(usize),
    NoProfile,
    NoMachine,
    Budget,
    MaxOpen,
}

/// The items to start, most urgent first, and why each of the others is not started.
pub fn pick(candidates: &[Candidate], limits: &Limits) -> (Vec<Uuid>, Vec<(Uuid, Skip)>) {
    let rank = |priority: &Priority| match priority {
        Priority::Critical => 0,
        Priority::High => 1,
        Priority::Medium => 2,
        Priority::Low => 3,
        Priority::None => 4,
    };
    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by_key(|c| (rank(&c.priority), c.sort_order));
    let room = limits.max_open.saturating_sub(limits.open_now);
    let (mut picked, mut skipped) = (Vec::new(), Vec::new());
    for c in ordered {
        let skip = if c.has_open_request {
            Some(Skip::OpenRequest)
        } else if c.unfinished_blockers > 0 {
            Some(Skip::Blocked(c.unfinished_blockers))
        } else if c.profile.is_none() {
            Some(Skip::NoProfile)
        } else if !c.has_machine {
            Some(Skip::NoMachine)
        } else if limits.budget_reached {
            Some(Skip::Budget)
        } else if picked.len() >= room {
            Some(Skip::MaxOpen)
        } else {
            None
        };
        match skip {
            Some(skip) => skipped.push((c.item_id, skip)),
            None => picked.push(c.item_id),
        }
    }
    (picked, skipped)
}

/// The numbers the sentence of a [`Skip`] quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub max_open: usize,
    pub open_now: usize,
}

/// Why a task is not started, as a code a screen switches on and a sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Why {
    pub code: &'static str,
    pub text: String,
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The reason a task waits, in plain words that carry the way out where there is one.
pub fn why(skip: &Skip, facts: &Facts) -> Why {
    let (code, text) = match skip {
        Skip::OpenRequest => ("open_request", "It already has a run open".to_owned()),
        Skip::Blocked(n) => (
            "blocked",
            format!(
                "Blocked by {}",
                count(*n, "unfinished task", "unfinished tasks")
            ),
        ),
        Skip::NoProfile => ("no_profile", "No one is chosen to do it".to_owned()),
        Skip::NoMachine => ("no_machine", "No computer can run it".to_owned()),
        Skip::Budget => ("budget", "This project's token limit is reached".to_owned()),
        Skip::MaxOpen => ("max_open", max_open_text(facts)),
    };
    Why { code, text }
}

fn max_open_text(facts: &Facts) -> String {
    if facts.open_now >= facts.max_open {
        format!(
            "{} already open, the most this project allows",
            count(facts.open_now, "run is", "runs are")
        )
    } else {
        format!(
            "This project allows {} open at once, and the tasks ahead of this one take the places left",
            count(facts.max_open, "run", "runs")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_defaults::Source;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn cand(n: u128, priority: Priority, sort_order: i32) -> Candidate {
        Candidate {
            item_id: id(n),
            priority,
            sort_order,
            version: 1,
            has_open_request: false,
            unfinished_blockers: 0,
            profile: Some(Resolved {
                profile_id: "p".into(),
                fleet_id: None,
                harness: None,
                source: Source::Implementer,
                from: None,
            }),
            has_machine: true,
        }
    }

    fn limits() -> Limits {
        Limits {
            max_open: 10,
            open_now: 0,
            budget_reached: false,
        }
    }

    #[test]
    fn picks_by_priority_then_order_and_names_why_each_other_waits() {
        let mut open = cand(1, Priority::Critical, 0);
        open.has_open_request = true;
        let mut blocked = cand(2, Priority::Critical, 0);
        blocked.unfinished_blockers = 2;
        let mut no_profile = cand(3, Priority::Critical, 0);
        no_profile.profile = None;
        let mut no_machine = cand(4, Priority::Critical, 0);
        no_machine.has_machine = false;
        let ready = [
            cand(5, Priority::Low, 0),
            cand(6, Priority::High, 9),
            cand(7, Priority::High, 2),
            cand(8, Priority::Medium, 0),
        ];
        let mut all = vec![open, blocked, no_profile, no_machine];
        all.extend(ready.iter().cloned());

        // (limits, picked, skipped)
        let rows = [
            (
                limits(),
                vec![id(7), id(6), id(8), id(5)],
                vec![
                    (id(1), Skip::OpenRequest),
                    (id(2), Skip::Blocked(2)),
                    (id(3), Skip::NoProfile),
                    (id(4), Skip::NoMachine),
                ],
            ),
            (
                Limits {
                    max_open: 3,
                    open_now: 1,
                    ..limits()
                },
                vec![id(7), id(6)],
                vec![
                    (id(1), Skip::OpenRequest),
                    (id(2), Skip::Blocked(2)),
                    (id(3), Skip::NoProfile),
                    (id(4), Skip::NoMachine),
                    (id(8), Skip::MaxOpen),
                    (id(5), Skip::MaxOpen),
                ],
            ),
            (
                Limits {
                    budget_reached: true,
                    ..limits()
                },
                vec![],
                vec![
                    (id(1), Skip::OpenRequest),
                    (id(2), Skip::Blocked(2)),
                    (id(3), Skip::NoProfile),
                    (id(4), Skip::NoMachine),
                    (id(7), Skip::Budget),
                    (id(6), Skip::Budget),
                    (id(8), Skip::Budget),
                    (id(5), Skip::Budget),
                ],
            ),
        ];
        for (limits, picked, skipped) in rows {
            assert_eq!(pick(&all, &limits), (picked, skipped), "{limits:?}");
        }
    }

    fn facts() -> Facts {
        Facts {
            max_open: 3,
            open_now: 3,
        }
    }

    #[test]
    fn each_skip_has_a_code_and_a_plain_sentence() {
        let facts = facts();
        // (skip, code, text)
        let rows = [
            (
                Skip::OpenRequest,
                "open_request",
                "It already has a run open",
            ),
            (Skip::Blocked(2), "blocked", "Blocked by 2 unfinished tasks"),
            (Skip::Blocked(1), "blocked", "Blocked by 1 unfinished task"),
            (Skip::NoProfile, "no_profile", "No one is chosen to do it"),
            (Skip::NoMachine, "no_machine", "No computer can run it"),
            (
                Skip::Budget,
                "budget",
                "This project's token limit is reached",
            ),
            (
                Skip::MaxOpen,
                "max_open",
                "3 runs are already open, the most this project allows",
            ),
        ];
        for (skip, code, text) in rows {
            assert_eq!(
                why(&skip, &facts),
                Why {
                    code,
                    text: text.into()
                }
            );
        }
    }

    #[test]
    fn a_cap_with_room_says_the_tasks_ahead_take_the_places_left() {
        let open = Facts {
            max_open: 1,
            open_now: 0,
        };
        assert_eq!(
            why(&Skip::MaxOpen, &open).text,
            "This project allows 1 run open at once, and the tasks ahead of this one take the places left"
        );
        let one = Facts {
            max_open: 1,
            open_now: 1,
        };
        assert_eq!(
            why(&Skip::MaxOpen, &one).text,
            "1 run is already open, the most this project allows"
        );
    }
}
