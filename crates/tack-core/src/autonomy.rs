//! Merge policy (ADR 0079, decision 1) as a pure function: facts in, `Ok` or the
//! first failing condition out. No I/O, no clock, no GitHub.

use std::fmt;
use std::str::FromStr;

use globset::GlobBuilder;

use tack_protocol::mrp::MergeDecision;

/// Merge authority a project or a domain grants. Declaration order is the
/// order of authority, so `L3 < L4 < L5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    L3,
    L4,
    L5,
}

impl Level {
    /// One step down: L5 to L4, L4 to L3, L3 stays L3.
    #[must_use]
    pub fn demoted(self) -> Level {
        match self {
            Level::L5 => Level::L4,
            Level::L4 | Level::L3 => Level::L3,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::L3 => "L3",
            Level::L4 => "L4",
            Level::L5 => "L5",
        })
    }
}

impl FromStr for Level {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "L3" => Ok(Level::L3),
            "L4" => Ok(Level::L4),
            "L5" => Ok(Level::L5),
            other => Err(format!("unknown autonomy level \"{other}\"")),
        }
    }
}

/// A named area of a repository with its own level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub name: String,
    pub path_globs: Vec<String>,
    pub item_types: Vec<String>,
    pub level: Level,
}

/// The level that governs a change, and the name of the domain that set it
/// (`None` when no domain matched and the project level applies).
///
/// A domain matches when every path matches one of its globs (`**` crosses
/// directories, `*` does not) and `item_types` is empty or holds `item_type`.
/// With no path there is nothing to match, so no domain applies: an empty
/// change list must not inherit a domain's level by default. A glob that does
/// not compile matches nothing.
///
/// No match gives `(L3, None)`, or `(L5, None)` when the project is L5. When
/// several domains match, the lowest level and its name win.
#[must_use]
pub fn level_for(
    project: Level,
    domains: &[Domain],
    paths: &[String],
    item_type: &str,
) -> (Level, Option<String>) {
    let matched = domains
        .iter()
        .filter(|d| domain_matches(d, paths, item_type))
        .min_by_key(|d| d.level);
    match matched {
        Some(d) => (d.level, Some(d.name.clone())),
        None if project == Level::L5 => (Level::L5, None),
        None => (Level::L3, None),
    }
}

fn domain_matches(domain: &Domain, paths: &[String], item_type: &str) -> bool {
    if !domain.item_types.is_empty() && !domain.item_types.iter().any(|t| t == item_type) {
        return false;
    }
    if paths.is_empty() {
        return false;
    }
    let matchers: Vec<_> = domain
        .path_globs
        .iter()
        .filter_map(|g| GlobBuilder::new(g).literal_separator(true).build().ok())
        .map(|g| g.compile_matcher())
        .collect();
    paths.iter().all(|p| matchers.iter().any(|m| m.is_match(p)))
}

/// Outcome of one check that a level may require.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Pass,
    Fail(String),
}

/// State of CI on the change's head commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiState {
    Green,
    Pending,
    Red,
    Absent,
}

/// What asked for the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The Merge button on a finished task.
    Button,
    /// The project option: the task moved to Done.
    Done,
    /// The automatic merge at L4 or L5.
    Auto,
}

/// Everything the policy reads, gathered by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub trigger: Trigger,
    pub from_mcp: bool,
    pub decision: MergeDecision,
    pub level: Level,
    pub require: Check,
    pub attestation: Check,
    pub budget: Check,
    pub pr_open: Check,
    pub ci: CiState,
    /// The most autonomy the team that ran the change allows; `Ok(None)` when
    /// unknown, `Err` with the sentence when the run names a team its computer
    /// does not report.
    pub ceiling: Result<Option<Level>, String>,
}

/// The first condition that stops a merge. One variant per condition; the
/// carried text is the reason a failed check gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hold {
    FromMcp,
    NotMerge,
    LevelBelowL4,
    HarnessCeiling { ceiling: Level, wanted: Level },
    TeamNotReported(String),
    Require(String),
    Attestation(String),
    Budget(String),
    PrNotOpen(String),
    CiPending,
    CiRed,
    CiAbsent,
}

impl fmt::Display for Hold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hold::FromMcp => {
                f.write_str("This run came through MCP; merge it with the Merge button")
            }
            Hold::NotMerge => f.write_str("prill did not say merge"),
            Hold::LevelBelowL4 => f.write_str(
                "This change's domain is not authorised for automatic merge (needs L4 or L5)",
            ),
            Hold::HarnessCeiling { ceiling, .. } => write!(
                f,
                "The team that ran this stays at {ceiling}: it needs a person during a run"
            ),
            Hold::TeamNotReported(reason)
            | Hold::Require(reason)
            | Hold::Attestation(reason)
            | Hold::Budget(reason)
            | Hold::PrNotOpen(reason) => f.write_str(reason),
            Hold::CiPending => f.write_str("CI is still running on the head commit"),
            Hold::CiRed => f.write_str("CI failed on the head commit"),
            Hold::CiAbsent => f.write_str("No CI result is reported for the head commit"),
        }
    }
}

fn check(c: &Check) -> Result<(), String> {
    match c {
        Check::Pass => Ok(()),
        Check::Fail(reason) => Err(reason.clone()),
    }
}

/// Decide whether a merge may happen. `Ok` means it may; `Err` names the first
/// failing condition in the order the trigger checks them.
pub fn evaluate(f: &Facts) -> Result<(), Hold> {
    match f.trigger {
        Trigger::Button => {}
        Trigger::Done => {
            if f.from_mcp {
                return Err(Hold::FromMcp);
            }
        }
        Trigger::Auto => {
            if f.from_mcp {
                return Err(Hold::FromMcp);
            }
            if f.decision != MergeDecision::Merge {
                return Err(Hold::NotMerge);
            }
            if f.level < Level::L4 {
                return Err(Hold::LevelBelowL4);
            }
            match &f.ceiling {
                Err(reason) => return Err(Hold::TeamNotReported(reason.clone())),
                Ok(Some(ceiling)) if *ceiling < f.level => {
                    return Err(Hold::HarnessCeiling {
                        ceiling: *ceiling,
                        wanted: f.level,
                    });
                }
                Ok(_) => {}
            }
            check(&f.require).map_err(Hold::Require)?;
            check(&f.attestation).map_err(Hold::Attestation)?;
            check(&f.budget).map_err(Hold::Budget)?;
        }
    }
    check(&f.pr_open).map_err(Hold::PrNotOpen)?;
    match f.ci {
        CiState::Green => Ok(()),
        CiState::Pending => Err(Hold::CiPending),
        CiState::Red => Err(Hold::CiRed),
        CiState::Absent => Err(Hold::CiAbsent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_table() {
        let base = |trigger| Facts {
            trigger,
            from_mcp: false,
            decision: MergeDecision::Merge,
            level: Level::L4,
            require: Check::Pass,
            attestation: Check::Pass,
            budget: Check::Pass,
            pr_open: Check::Pass,
            ci: CiState::Green,
            ceiling: Ok(None),
        };
        let fail = |s: &str| Check::Fail(s.to_string());
        let rows: Vec<(&str, Facts, Result<(), Hold>)> = vec![
            ("button all pass", base(Trigger::Button), Ok(())),
            (
                "button with an unsigned review pack",
                Facts {
                    decision: MergeDecision::Review,
                    attestation: fail("no signature"),
                    level: Level::L3,
                    ..base(Trigger::Button)
                },
                Ok(()),
            ),
            (
                "button from MCP is accepted",
                Facts {
                    from_mcp: true,
                    ..base(Trigger::Button)
                },
                Ok(()),
            ),
            (
                "button, PR not open",
                Facts {
                    pr_open: fail("no PR"),
                    ..base(Trigger::Button)
                },
                Err(Hold::PrNotOpen("no PR".into())),
            ),
            (
                "button, CI pending",
                Facts {
                    ci: CiState::Pending,
                    ..base(Trigger::Button)
                },
                Err(Hold::CiPending),
            ),
            (
                "button, CI red",
                Facts {
                    ci: CiState::Red,
                    ..base(Trigger::Button)
                },
                Err(Hold::CiRed),
            ),
            (
                "button, CI absent",
                Facts {
                    ci: CiState::Absent,
                    ..base(Trigger::Button)
                },
                Err(Hold::CiAbsent),
            ),
            ("done all pass", base(Trigger::Done), Ok(())),
            (
                "done from MCP",
                Facts {
                    from_mcp: true,
                    ..base(Trigger::Done)
                },
                Err(Hold::FromMcp),
            ),
            (
                "done, CI pending",
                Facts {
                    ci: CiState::Pending,
                    ..base(Trigger::Done)
                },
                Err(Hold::CiPending),
            ),
            ("auto all pass", base(Trigger::Auto), Ok(())),
            (
                "auto from MCP",
                Facts {
                    from_mcp: true,
                    ..base(Trigger::Auto)
                },
                Err(Hold::FromMcp),
            ),
            (
                "auto, prill said review",
                Facts {
                    decision: MergeDecision::Review,
                    ..base(Trigger::Auto)
                },
                Err(Hold::NotMerge),
            ),
            (
                "auto at L3",
                Facts {
                    level: Level::L3,
                    ..base(Trigger::Auto)
                },
                Err(Hold::LevelBelowL4),
            ),
            (
                "auto, require fails",
                Facts {
                    require: fail("stage tests not run"),
                    ..base(Trigger::Auto)
                },
                Err(Hold::Require("stage tests not run".into())),
            ),
            (
                "auto, attestation fails",
                Facts {
                    attestation: fail("subject_mismatch"),
                    ..base(Trigger::Auto)
                },
                Err(Hold::Attestation("subject_mismatch".into())),
            ),
            (
                "auto, budget exceeded",
                Facts {
                    budget: fail("3 of 3 auto merges today"),
                    ..base(Trigger::Auto)
                },
                Err(Hold::Budget("3 of 3 auto merges today".into())),
            ),
            (
                "auto, PR not open",
                Facts {
                    pr_open: fail("PR closed"),
                    ..base(Trigger::Auto)
                },
                Err(Hold::PrNotOpen("PR closed".into())),
            ),
            (
                "auto, CI pending",
                Facts {
                    ci: CiState::Pending,
                    ..base(Trigger::Auto)
                },
                Err(Hold::CiPending),
            ),
            (
                "auto, CI red",
                Facts {
                    ci: CiState::Red,
                    ..base(Trigger::Auto)
                },
                Err(Hold::CiRed),
            ),
            (
                "auto, CI absent",
                Facts {
                    ci: CiState::Absent,
                    ..base(Trigger::Auto)
                },
                Err(Hold::CiAbsent),
            ),
            (
                "auto, the team's ceiling is below the level",
                Facts {
                    ceiling: Ok(Some(Level::L3)),
                    ..base(Trigger::Auto)
                },
                Err(Hold::HarnessCeiling {
                    ceiling: Level::L3,
                    wanted: Level::L4,
                }),
            ),
            (
                "auto, the run's team is not reported by its computer",
                Facts {
                    ceiling: Err("Team fab-dev is not reported yet by this computer".into()),
                    ..base(Trigger::Auto)
                },
                Err(Hold::TeamNotReported(
                    "Team fab-dev is not reported yet by this computer".into(),
                )),
            ),
            (
                "auto, the team's ceiling allows the level",
                Facts {
                    ceiling: Ok(Some(Level::L5)),
                    ..base(Trigger::Auto)
                },
                Ok(()),
            ),
            (
                "button, the team's ceiling does not hold the Merge button",
                Facts {
                    ceiling: Ok(Some(Level::L3)),
                    ..base(Trigger::Button)
                },
                Ok(()),
            ),
            (
                "auto, MCP wins over every later failure",
                Facts {
                    from_mcp: true,
                    decision: MergeDecision::Review,
                    level: Level::L3,
                    ci: CiState::Red,
                    ..base(Trigger::Auto)
                },
                Err(Hold::FromMcp),
            ),
        ];
        for (name, facts, expected) in rows {
            assert_eq!(evaluate(&facts), expected, "row: {name}");
        }
    }

    #[test]
    fn level_for_table() {
        fn domain(name: &str, globs: &[&str], types: &[&str], level: Level) -> Domain {
            Domain {
                name: name.into(),
                path_globs: globs.iter().map(|g| (*g).into()).collect(),
                item_types: types.iter().map(|t| (*t).into()).collect(),
                level,
            }
        }
        let paths = |p: &[&str]| p.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let docs = domain("docs", &["docs/**"], &[], Level::L4);
        let migrations = domain("migrations", &["db/**/*.sql"], &["migration"], Level::L3);

        let rows = vec![
            (
                "one domain matches",
                Level::L3,
                vec![docs.clone(), migrations.clone()],
                paths(&["docs/guide/intro.md", "docs/a/b.md"]),
                "task",
                (Level::L4, Some("docs".into())),
            ),
            (
                "two domains match: the lowest wins",
                Level::L5,
                vec![
                    domain("wide", &["**"], &[], Level::L5),
                    domain("db", &["db/**"], &[], Level::L4),
                ],
                paths(&["db/x/0001.sql"]),
                "task",
                (Level::L4, Some("db".into())),
            ),
            (
                "no domain matches at L5",
                Level::L5,
                vec![docs.clone(), migrations.clone()],
                paths(&["src/main.rs"]),
                "task",
                (Level::L5, None),
            ),
        ];
        for (name, project, domains, changed, item_type, expected) in rows {
            assert_eq!(
                level_for(project, &domains, &changed, item_type),
                expected,
                "row: {name}"
            );
        }
    }
}
