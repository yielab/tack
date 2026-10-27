//! The `kind` of the timeline events a runner reports for an attempt, which the board stores as
//! they come and its queries count by name.

/// A docket hop began.
pub const AGENT_HOP_STARTED: &str = "agent.hop_started";
/// A docket hop ended; its text carries the verdict and the tokens.
pub const AGENT_HOP_FINISHED: &str = "agent.hop_finished";
/// Files docket did not commit are left out of the evidence.
pub const AGENT_UNTRACKED: &str = "agent.untracked";
/// docket refused the task, with its own words.
pub const AGENT_REFUSED: &str = "agent.refused";
/// docket did not take a person's answer; the question is asked again.
pub const AGENT_ANSWER_FAILED: &str = "agent.answer_failed";

/// docket asked a person.
pub const ESCALATION_ASKED: &str = "escalation.asked";
/// A person answered what docket asked.
pub const ESCALATION_RESOLVED: &str = "escalation.resolved";
/// Nobody answered before the question expired.
pub const ESCALATION_EXPIRED: &str = "escalation.expired";
/// docket's policy answered by itself, without asking anyone.
pub const ESCALATION_DECIDED: &str = "escalation.decided";
/// docket asked for something Tack cannot put to a person.
pub const ESCALATION_UNREADABLE: &str = "escalation.unreadable";
