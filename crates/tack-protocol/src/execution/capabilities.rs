//! Runner capability snapshots and support declarations.

use std::{borrow::Cow, collections::BTreeMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{HarnessKind, ProtocolVersion};

/// The three support levels fixed by runner protocol v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Advisory,
}

/// A capability value coupled to the reason supplied by the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityValue {
    pub support: CapabilitySupport,
    /// `null` is meaningful fixture data: preserve it instead of silently
    /// omitting the key during a round trip.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Per-feature support statements reported by a runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCapabilities {
    pub cancel: CapabilityValue,
    pub resume: CapabilityValue,
    pub decisions: CapabilityValue,
    pub artifacts: CapabilityValue,
    pub usage: CapabilityValue,
    /// Whether this harness's runs can propose items (a `plan` artifact in plan-v1), and why not.
    /// Absent means the harness did not say: a reader shows it as unsupported with the reason
    /// "this agent does not say".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposals: Option<CapabilityValue>,
    /// Whether this harness can say what its teams are doing, the work it did not start included
    /// (a `TeamActivity`), and why not. Absent means the harness did not say: a reader shows it
    /// as unsupported with the reason "this agent does not say".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<CapabilityValue>,
    /// Whether this harness can hold a conversation across turns: `continues` (the agent keeps
    /// the context of the earlier turns) or `replays` (each turn is a new run given the
    /// transcript), as [`ConverseMode`](crate::conversation::ConverseMode) reads it, and why not.
    /// Absent means the harness did not say: a reader shows it as unsupported with the reason
    /// "this agent does not say".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converse: Option<CapabilityValue>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Current and total execution capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concurrency {
    pub total: u32,
    pub available: u32,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// The key under which a harness that speaks a versioned API (docket) reports it, beside
/// `installed_version`, which holds only the program's own version. It rides in
/// `additional`, so an older runner or reader that does not know it loses nothing.
const API_VERSION_KEY: &str = "api_version";

/// The key under which a harness lists the features of its API that the board uses and that it
/// answers to (docket's `features`), by name. Rides in `additional` like [`API_VERSION_KEY`]; absent
/// means none is listed, or the runner is older than the key.
const API_FEATURES_KEY: &str = "api_features";

/// The key under which an agent reports what it itself can do, per feature, in the shape of the
/// runner's own `features`. It rides in `additional` beside `decisions`, so a runner or reader that
/// does not know it loses nothing; absent means the agent did not say.
const FEATURES_KEY: &str = "features";

/// The key under which a harness reports who meets each [`ParityRule`], as a list of
/// [`ParityLine`]. It rides in `additional` like [`FEATURES_KEY`]; absent means the runner is older
/// than the key.
const PARITY_KEY: &str = "parity";

/// One installed harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapability {
    pub harness_kind: HarnessKind,
    pub installed_version: String,
    pub probe_error: Option<String>,
    pub probed_at: DateTime<Utc>,
    /// Whether **this harness** can pause a run and ask the operator before
    /// it acts, for a request whose `permission_policy.approvals` is `ask`.
    /// A runner's top-level `features.decisions` only says whether the
    /// protocol path exists on this runner at all; which harness can
    /// actually use it differs per adapter (claude-code's grammar keeps
    /// stdin open and answers a `can_use_tool` question, codex/docket/
    /// opencode never open one), so the scheduler reads this field, not
    /// `features.decisions`, to admit or refuse an `ask` request. Only
    /// `Supported` counts, `Advisory` and an absent attestation both mean
    /// "not attested" and are treated like `Unsupported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<CapabilityValue>,
    /// Whether a request must name a model for this harness, or may leave
    /// the choice to it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requires_model: bool,
    /// Why a request must name a model; present only when it must.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_model_reason: Option<String>,
    /// The models the agent offers. `None` means the runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelsReport>,
    /// The model the agent itself would use, only when the agent names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Whether the agent is signed in. `None` means the runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<SignInReport>,
    /// Where the agent's own documentation covers its models and its login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocsLinks>,
    /// The most autonomy the harness's teams allow, `"L3"`; absent when the
    /// runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy_ceiling: Option<String>,
    /// Each team the harness runs, with what keeps it from running lights-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<Vec<TeamReport>>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl HarnessCapability {
    /// The API version the harness answers, when it has one: `"5"`, not `"API 5"`.
    pub fn api_version(&self) -> Option<&str> {
        self.additional.get(API_VERSION_KEY)?.as_str()
    }

    /// What this agent itself reports it can do, when it said: stop, resume, answer questions,
    /// propose items, report usage. `None` when the report carries none, or one that cannot be read.
    pub fn features(&self) -> Option<FeatureCapabilities> {
        serde_json::from_value(self.additional.get(FEATURES_KEY)?.clone()).ok()
    }

    /// Records what this agent can do; see [`Self::features`].
    pub fn set_features(&mut self, features: &FeatureCapabilities) {
        if let Ok(value) = serde_json::to_value(features) {
            self.additional.insert(FEATURES_KEY.to_owned(), value);
        }
    }

    /// Who meets each parity rule for this agent, when the runner said. `None` when the report
    /// carries none, or one that cannot be read.
    pub fn parity(&self) -> Option<Vec<ParityLine>> {
        serde_json::from_value(self.additional.get(PARITY_KEY)?.clone()).ok()
    }

    /// Records who meets each parity rule; see [`Self::parity`].
    pub fn set_parity(&mut self, column: &[(ParityRule, ParityCell)]) {
        let lines: Vec<ParityLine> = column
            .iter()
            .map(|(rule, cell)| ParityLine::new(*rule, cell.clone()))
            .collect();
        if let Ok(value) = serde_json::to_value(lines) {
            self.additional.insert(PARITY_KEY.to_owned(), value);
        }
    }

    /// Whether the harness lists `feature` among those of its API the runner checked.
    pub fn lists_api_feature(&self, feature: &str) -> bool {
        self.additional
            .get(API_FEATURES_KEY)
            .and_then(|listed| listed.as_array())
            .is_some_and(|listed| listed.iter().any(|name| name.as_str() == Some(feature)))
    }

    /// Records which of the API's features the harness lists; see [`Self::lists_api_feature`].
    pub fn set_api_features(&mut self, features: &[&str]) {
        self.additional
            .insert(API_FEATURES_KEY.to_owned(), serde_json::json!(features));
    }

    /// Records the API version the harness answers; see [`Self::api_version`].
    pub fn set_api_version(&mut self, version: &str) {
        self.additional
            .insert(API_VERSION_KEY.to_owned(), serde_json::json!(version));
    }
}

/// One team a harness runs.
///
/// Every optional field below is in Tack's own words, filled by the harness's adapter from what
/// its agent reports. Absent means the harness did not say, which is never the same as "none":
/// a screen shows what was reported and nothing it had to guess. No field may carry a harness's
/// own key name; a harness's own labels arrive only as data (`shape`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamReport {
    pub name: String,
    /// How closely a person supervises the team, as the harness names it (`attended`,
    /// `lights-out`). Absent for a team of one, which has no such setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub violations: Vec<TeamViolation>,
    /// The folders the team's sandbox may read besides its own work, as docket resolved them
    /// (its machine-wide grants narrowed by the team's own setting); empty when there are none.
    /// Absent when the harness did not say, which is not the same as none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
    /// Whether the team works in a repository or in a plain folder. Absent when the harness did
    /// not say, which is not the same as a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub works_in: Option<WorksIn>,
    /// The record sets the team keeps, with how many rows each holds. Absent when the harness
    /// has no such thing or did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<TeamRecordSet>>,
    /// Who does the work: one entry per member, with the model it runs on. Absent means the
    /// harness did not say; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<TeamMember>>,
    /// Whether the team runs to the end alone or stops for a person. Absent means the harness did
    /// not say; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decides: Option<TeamDecides>,
    /// How far the team's agents can reach over the network. Absent means the harness did not
    /// say; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<TeamNetwork>,
    /// How much work the team holds now. Absent means the harness did not say; no field of this
    /// type may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<TeamQueue>,
    /// A label the harness gives the team's shape, shown as it came and never sent anywhere.
    /// Absent means the harness did not say; no field may carry a harness's own key
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// The steps a task goes through, in order. Absent means the harness did not say (an empty
    /// list is a team that reported no steps); no field may carry a harness's own
    /// key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<TeamStep>>,
    /// Whether what the members propose is kept for every task of the team together (`true`: a
    /// name used in one task cannot be used again in another until the team's list of proposals
    /// is recreated) or for each task apart (`false`). Absent means the team keeps no list of
    /// proposals or the harness did not say; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposals_shared_across_tasks: Option<bool>,
    /// Why the team claims no task now, when it does not: tasks sent to it wait. Absent means the
    /// team is not paused, or the harness did not say; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<TeamPause>,
    /// The folder a team that works in one writes in, as a path on the computer that runs it.
    /// Absent for a team in a repository, or when the harness did not say; no field may carry a
    /// harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// What the team hands back when a task ends, apart from the rows in its record sets, which
    /// `records` says. Absent means the harness did not say, which is not the same as a branch; no
    /// field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivers: Option<TeamDelivers>,
    /// Every method a task may name, each with the roles the team lacks. Absent means the harness
    /// did not say (an empty list is not a team without methods); no field may carry a harness's
    /// own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<TeamMethod>>,
    /// What the team's work still needs, each satisfied or not. Absent means the harness did not
    /// say, which is not the same as needing nothing; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs: Option<Vec<TeamNeed>>,
    /// How many tokens the team may spend, how many it has, and over what span. Absent means the
    /// harness did not say, which is not the same as no limit; no field may carry a harness's own
    /// key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<TeamBudget>,
    /// What the team has learnt about its work and what it waits to have approved. Absent means
    /// the harness keeps no such memory or did not say, which is not the same as nothing learnt;
    /// no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<TeamMemory>,
    /// How the team reads the web, in the harness's own sentence: whether it does, why, and what
    /// to change when it does not. Absent means the harness did not say, which is not the same as
    /// no web; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_read: Option<TeamWebRead>,
    /// The web search the team has: which provider, whether it is ready and the one command that
    /// makes it so. Absent means the harness did not say, which is not the same as no search; no
    /// field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<TeamSearch>,
}

/// How a team reads the web and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamWebRead {
    pub mode: TeamWebMode,
    /// Why the team reads as it does, a sentence the harness wrote.
    pub reason: String,
    /// What the operator changes to give the team the web; empty unless the team reads none.
    #[serde(default)]
    pub fix: String,
}

/// How far a team's `fetch` reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamWebMode {
    /// Any public host, with no list to keep.
    Open,
    /// The hosts the team names, or the hosts the operator granted.
    Listed,
    /// No web: no member reads it.
    Closed,
}

/// The web search a team has, in the harness's own sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamSearch {
    pub provider: TeamSearchProvider,
    /// Whether the team can search now.
    pub ready: bool,
    /// Why the search is as it is, a sentence the harness wrote.
    pub reason: String,
    /// The one command that makes the search ready; empty when it is.
    #[serde(default)]
    pub fix: String,
    /// The ways to set the search up, each a command on the harness's computer.
    #[serde(default)]
    pub options: Vec<TeamSearchOption>,
}

/// Which service answers a team's searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamSearchProvider {
    /// No search service is set up.
    None,
    /// A SearXNG service the operator runs.
    Searxng,
    /// The Brave Search API, with the operator's key.
    Brave,
    /// A service the operator wrote.
    Custom,
}

/// One way to set a team's search up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamSearchOption {
    pub provider: TeamSearchProvider,
    /// The command, run where the harness is.
    pub command: String,
    /// The name of the secret the option needs, never its value; empty when it needs none.
    #[serde(default)]
    pub secret: String,
}

/// The kind of a [`TeamNeed`] that a signed-in agent meets.
pub const NEED_SIGN_IN: &str = "sign_in";

/// The kind of a [`TeamNeed`] that a run meets by naming a model. It is checked per run (the
/// scheduler and the harness refuse a run that names none), so [`team_misfits`] never counts it.
pub const NEED_MODEL: &str = "model";

/// The one method of a team of one: the agent's own loop.
const AGENT_METHOD: &str = "agent";

impl TeamReport {
    /// The team a profile with no team is: one agent, filled from what the runner knows of the
    /// harness it runs. It delivers a branch, has one method (the agent's own loop) and needs what
    /// the runner's doctor says: a sign-in, and a model where the harness will not choose one.
    /// A fact the runner did not report (a sign-in it could not read) adds nothing.
    pub fn of_agent(harness: &str, capability: &HarnessCapability) -> TeamReport {
        let mut needs = Vec::new();
        if let Some(sign_in) = capability
            .sign_in
            .as_ref()
            .filter(|sign_in| sign_in.state != SignInState::Unknown)
        {
            let fix = match (&sign_in.command, &capability.docs) {
                (Some(command), _) => format!("Run `{command}` on the runner's computer."),
                (None, Some(docs)) => format!("See {}.", docs.sign_in),
                (None, None) => "Sign the agent in on the runner's computer.".to_owned(),
            };
            needs.push(TeamNeed {
                kind: NEED_SIGN_IN.to_owned(),
                name: "signed in".to_owned(),
                fix,
                satisfied: sign_in.state == SignInState::SignedIn,
                blocking: None,
            });
        }
        if capability.requires_model && capability.default_model.is_none() {
            needs.push(TeamNeed {
                kind: NEED_MODEL.to_owned(),
                name: "a model".to_owned(),
                fix: capability
                    .requires_model_reason
                    .clone()
                    .unwrap_or_else(|| "Name a model in the profile.".to_owned()),
                satisfied: false,
                blocking: None,
            });
        }
        TeamReport {
            name: harness.to_owned(),
            profile: None,
            violations: Vec::new(),
            read_only: None,
            works_in: None,
            records: None,
            members: Some(vec![TeamMember {
                role: "agent".to_owned(),
                model: capability.default_model.clone(),
                can_propose: None,
                writes_project: None,
                reads_web: None,
                web_hosts: None,
            }]),
            decides: None,
            network: None,
            queue: None,
            shape: None,
            steps: None,
            proposals_shared_across_tasks: None,
            paused: None,
            folder: None,
            delivers: Some(TeamDelivers::Branch),
            methods: Some(vec![TeamMethod {
                name: AGENT_METHOD.to_owned(),
                source: "harness".to_owned(),
                default: true,
                missing_roles: Vec::new(),
                error: None,
            }]),
            needs: (!needs.is_empty()).then_some(needs),
            budget: None,
            memory: None,
            web_read: None,
            search: None,
        }
    }

    /// Whether this is [`Self::of_agent`]'s team for `harness`: named after it, with no
    /// supervision setting.
    pub fn is_of_agent(&self, harness: &str) -> bool {
        self.name == harness && self.profile.is_none()
    }

    /// What the team said that decides whether a run fits it.
    pub fn facts(&self) -> TeamFacts<'_> {
        TeamFacts {
            network: self.network,
            members: self.members.as_deref(),
            delivers: self.delivers,
            methods: self.methods.as_deref(),
            needs: self.needs.as_deref(),
            web_read: self.web_read.as_ref(),
        }
    }

    /// What a profile needs that this team says it cannot do; see [`team_misfits`].
    pub fn misfits(&self, needs: &TeamNeeds) -> Vec<TeamMisfit> {
        team_misfits(self.facts(), needs)
    }
}

/// What a team hands back when a task ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamDelivers {
    /// A branch of the project's repository, to review and merge.
    Branch,
    /// A report: the team's last step says what it did and there is nothing to merge.
    Report,
}

impl TeamDelivers {
    /// The word a project file and a sentence use for it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Report => "report",
        }
    }
}

/// One way of working a task that a team offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamMethod {
    /// The name a task gives to choose it.
    pub name: String,
    /// Where the team has it from, in the harness's own word, shown as data.
    pub source: String,
    /// Whether a task that names no method goes through this one.
    pub default: bool,
    /// The roles the method has a step for and the team has no member for: a task that names it is
    /// refused. Empty when the team can run every step.
    #[serde(default)]
    pub missing_roles: Vec<String>,
    /// Why the method cannot run at all: its file does not load. Absent for a method that loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One thing a team's work still needs, and how to give it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamNeed {
    /// What sort of need it is, in the harness's own word, shown as data; [`NEED_SIGN_IN`] and
    /// [`NEED_MODEL`] are the two Tack's runner reports.
    pub kind: String,
    /// What is needed, as the team names it.
    pub name: String,
    /// What the operator does to meet it.
    pub fix: String,
    /// Whether it is met now.
    pub satisfied: bool,
    /// Whether the work cannot go on without it. `Some(false)` is a need whose absence only
    /// degrades the work (a research team that cannot read PDFs): it is shown as a warning and
    /// never refuses a run. Absent from a harness that does not say, which Tack reads as blocking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
}

/// What a profile needs from the team it names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TeamNeeds {
    /// The run reaches the web: the profile's network, or the run's own when it says.
    pub web: bool,
    /// The run's whole delivery is items proposed for a person to accept (a Planner).
    pub proposals: bool,
    /// The run changes the project's repository, so it needs a team that delivers a branch.
    pub repository: bool,
    /// The method the task names, when it names one.
    pub method: Option<String>,
}

impl TeamNeeds {
    /// What a profile of `kind` needs, given whether its run reaches the web and the method its
    /// task names. A Planner delivers only proposals; a Researcher may propose and delivers its
    /// findings either way; any other kind changes the repository.
    pub fn of(kind: &str, web: bool, method: Option<&str>) -> Self {
        Self {
            web,
            proposals: kind == "planner",
            repository: !matches!(kind, "planner" | "researcher"),
            method: method.map(str::to_owned),
        }
    }
}

/// A need of a profile that the team it names says it cannot meet. A run that has one ends before
/// the team gets the task, because the team would stop and ask a person for what only its own
/// setup can give.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamMisfit {
    /// The run reaches the web, and the team reads none: the harness says so, or it has no
    /// network and no member that reads the web.
    NoWeb,
    /// The run proposes items, and no member of the team can.
    NoProposals,
    /// The run changes the repository, and the team delivers a report or records.
    NoRepository,
    /// The task names a method the team does not offer.
    UnknownMethod { name: String },
    /// The task names a method the team offers but lacks the members for: it would be refused.
    StepsSkipped { method: String, roles: Vec<String> },
    /// The task names a method the team lists but whose file does not load.
    MethodBroken { method: String, error: String },
    /// The team says one of its needs is not met, and that the work cannot go on without it (a
    /// need that only degrades the work is a warning, not a misfit).
    NeedsUnmet { name: String },
}

/// What a team said of itself that decides whether a run fits it; every part is absent when the
/// harness did not say.
#[derive(Debug, Clone, Copy, Default)]
pub struct TeamFacts<'a> {
    pub network: Option<TeamNetwork>,
    pub members: Option<&'a [TeamMember]>,
    pub delivers: Option<TeamDelivers>,
    pub methods: Option<&'a [TeamMethod]>,
    pub needs: Option<&'a [TeamNeed]>,
    pub web_read: Option<&'a TeamWebRead>,
}

/// What a profile needs that a team with these facts says it cannot do. Only what the harness
/// said counts: an absent network, member list, contract or method list, or a member that did not
/// say, is never a misfit.
pub fn team_misfits(facts: TeamFacts<'_>, needs: &TeamNeeds) -> Vec<TeamMisfit> {
    let members = facts.members.filter(|members| !members.is_empty());
    let none_can = |can: fn(&TeamMember) -> Option<bool>| {
        members.is_some_and(|members| members.iter().all(|member| can(member) == Some(false)))
    };
    let mut misfits = Vec::new();
    // What the harness says of the web is the answer when it says it; otherwise the network and
    // the members' tools are read, which is what an older docket gives.
    let no_web = match facts.web_read {
        Some(web) => web.mode == TeamWebMode::Closed,
        None => facts.network == Some(TeamNetwork::Closed) && none_can(|member| member.reads_web),
    };
    if needs.web && no_web {
        misfits.push(TeamMisfit::NoWeb);
    }
    if needs.proposals && none_can(|member| member.can_propose) {
        misfits.push(TeamMisfit::NoProposals);
    }
    if needs.repository && facts.delivers == Some(TeamDelivers::Report) {
        misfits.push(TeamMisfit::NoRepository);
    }
    if let (Some(method), Some(offered)) = (
        needs.method.as_deref(),
        facts.methods.filter(|methods| !methods.is_empty()),
    ) {
        match offered.iter().find(|offered| offered.name == method) {
            None => misfits.push(TeamMisfit::UnknownMethod {
                name: method.to_owned(),
            }),
            Some(found) if found.error.is_some() => {
                misfits.push(TeamMisfit::MethodBroken {
                    method: method.to_owned(),
                    error: found.error.clone().unwrap_or_default(),
                });
            }
            Some(found) if !found.missing_roles.is_empty() => {
                misfits.push(TeamMisfit::StepsSkipped {
                    method: method.to_owned(),
                    roles: found.missing_roles.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for need in facts.needs.unwrap_or_default() {
        if !need.satisfied && need.kind != NEED_MODEL && need.blocking != Some(false) {
            misfits.push(TeamMisfit::NeedsUnmet {
                name: need.name.clone(),
            });
        }
    }
    misfits
}

/// What a team remembers about its work: the entries it has learnt and those that wait for a
/// person to approve them. Both lists are oldest first, as the harness gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamMemory {
    /// Held back until a person decides; the team does not use them yet.
    pub pending: Vec<MemoryEntry>,
    /// Checked or approved; the team reads them in its next task.
    pub accepted: Vec<MemoryEntry>,
    /// Whether a pending entry can be approved or refused from the board. When it cannot, the
    /// harness has its own command for it.
    #[serde(default)]
    pub can_decide: bool,
}

/// One thing a team remembers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MemoryEntry {
    /// The harness's own id, which its commands take.
    pub id: String,
    /// What sort of fact it is, in the harness's word (`generated`, `tool`, `pitfall`,
    /// `convention`).
    pub kind: String,
    /// What it is about: a path, a label or a command.
    pub subject: String,
    pub note: String,
    /// Why the harness decided as it did, or why it waits.
    pub reason: String,
    /// Whether what the entry names is gone since it was accepted.
    pub stale: bool,
    /// Where it was learnt, as the harness records it (the task, the date, the commit).
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
    /// What the harness saw that backs it, when it saw anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<MemoryEvidence>,
}

/// What backs an entry: the exit status of a command or whether a tool call succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MemoryEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// What a team may spend: the harness applies it, Tack shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamBudget {
    /// The tokens the team may spend in the span; absent when the team has no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    /// The tokens it has spent in the span.
    pub used: u64,
    /// How long the span lasts, in the harness's word for it (`none` for a span that never
    /// restarts, `day`, `week`).
    pub window: String,
    /// When the current span began; absent for a span that never restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_start: Option<String>,
}

/// Why a team is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamPause {
    /// Its budget is spent.
    Budget,
    /// For a reason Tack has no word for.
    Other,
}

/// One member of a team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamMember {
    /// What the member does, in the harness's own word for it, shown as data.
    pub role: String,
    /// The model the member runs on. Absent means the harness did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether the member can propose items for a person to accept. Absent means the harness did
    /// not say, which is not the same as `false`; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_propose: Option<bool>,
    /// Whether the member can change the project's files or run commands in it (`false`: it only
    /// reads). Absent means the harness did not say, which is not the same as `false`; no field
    /// may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writes_project: Option<bool>,
    /// Whether the member can read web pages through a tool that reaches the web whatever the
    /// team's network (`false`: it has none). Absent means the harness did not say, which is not
    /// the same as `false`; no field may carry a harness's own key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads_web: Option<bool>,
    /// The hosts the member reads the web from when it does; `*` is any public host. Absent when
    /// it reads none, or the harness did not say which.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_hosts: Option<Vec<String>>,
}

/// Whether a team runs to the end alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamDecides {
    /// Nothing in a run waits for a person.
    ToTheEnd,
    /// A run can stop and wait for a person.
    AsksAPerson,
}

/// How far a team's agents can reach over the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamNetwork {
    /// No network.
    #[serde(rename = "none")]
    Closed,
    /// Only some destinations.
    Limited,
    /// Anywhere.
    Open,
}

/// The work a team holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamQueue {
    /// Tasks that have not started or wait for a person.
    pub waiting: u64,
    /// Tasks being worked now.
    pub running: u64,
}

/// One step of the way a team handles a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamStep {
    pub name: String,
    pub kind: TeamStepKind,
    /// Who does an `agent` step, in the harness's own word for it, shown as data. Absent for a
    /// step nobody in the team does, or when the harness did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// What decides the step passed. Absent means nothing checks it, or the harness did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_by: Option<TeamStepCheck>,
    /// The earlier step the work goes back to when the check fails. Absent means it goes nowhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sends_back_to: Option<String>,
    /// How many times it may go back. Absent means the harness did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sends_back_max: Option<u32>,
}

/// What a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamStepKind {
    /// A member of the team works.
    Agent,
    /// A command runs.
    Command,
    /// A person is asked.
    Ask,
    /// The team waits for a time or a condition.
    Wait,
    /// Several steps run side by side.
    Parallel,
}

/// What decides a step passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamStepCheck {
    /// A command's exit.
    Command,
    /// A reviewing member's verdict.
    Reviewer,
    /// A person's approval.
    Person,
    /// A check outside the team.
    External,
}

/// Where a team does its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorksIn {
    /// A repository: its work comes back as a branch.
    Repository,
    /// A folder with no repository: its work stays there and there is nothing to merge.
    Folder,
}

/// One set of records a team keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamRecordSet {
    pub name: String,
    pub description: String,
    pub rows: u64,
}

/// One thing that makes a team need a person during a run, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamViolation {
    pub requirement: String,
    pub fix: String,
}

/// Where a harness's model ids came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelListSource {
    /// Asked of the agent itself.
    Listed,
    /// Names Tack suggests because the agent cannot list its own.
    Suggested,
    /// No ids; `reason` says why.
    None,
}

/// The models a harness offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsReport {
    pub source: ModelListSource,
    pub ids: Vec<String>,
    pub reason: Option<String>,
}

/// Whether a harness has a login it can run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInState {
    SignedIn,
    SignedOut,
    Unknown,
}

/// A harness's login state, and the command that fixes a missing one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInReport {
    pub state: SignInState,
    pub detail: String,
    pub command: Option<String>,
}

/// The agent's own documentation pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsLinks {
    pub models: String,
    pub sign_in: String,
}

/// Maximum payload values the runner says it can handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    pub event_payload_bytes_max: u64,
    pub artifact_content_bytes_max: u64,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A complete point-in-time runner capability report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerCapabilities {
    /// Standalone capability reports carry the protocol version; embedded
    /// enrollment/refresh capability snapshots inherit it from the enclosing
    /// protocol message and therefore omit this member. Keep that distinction
    /// on the wire rather than materializing a field on re-serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<ProtocolVersion>,
    pub runner_version: String,
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    pub features: FeatureCapabilities,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A capability snapshot embedded inside `enrollment.request.json` or
/// `refresh.request.json` — a different wire shape than the standalone
/// [`RunnerCapabilities`] report, not a loosened copy of it.
///
/// `runner_version`/`protocol_version` have no field here: both are only
/// *siblings* of `capabilities` in the enclosing envelope. `harnesses`/
/// `features` stay permissive — `refresh.request.json` reports `[]`/`{}`
/// while `enrollment.request.json` reports populated data — so `features`
/// is opaque `serde_json::Value` rather than [`FeatureCapabilities`].
/// Unrecognised keys round-trip via `serde(flatten)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedCapabilitySnapshot {
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    #[serde(default)]
    pub features: serde_json::Value,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// The agents whose CLI reports no usage until it ends, so a token limit cannot stop them while
/// they run. The runner's descriptors own the fact (`reports_progress`); a runner test keeps this
/// list equal to them, for the board has no descriptors.
pub const WITHOUT_LIVE_USAGE: &[&str] = &["codex"];

/// Whether the agent reports its usage while it runs, which is what lets a token limit stop it
/// before it has gone past the limit.
pub fn reports_usage_while_running(harness: &str) -> bool {
    !WITHOUT_LIVE_USAGE.contains(&harness)
}

/// A rule every agent must meet for a run to be contained the same way whichever one the profile
/// names. One row of the parity matrix: each harness descriptor says, per rule, who meets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityRule {
    /// The agent runs in a sandbox, or does not run.
    SandboxOrNoRun,
    /// Where there is no sandbox, every action asks a person first.
    NoSandboxMeansAsk,
    /// The network is reached only through the allow-list.
    EgressByAllowList,
    /// An item that is not trusted starts out asking before each action and without web tools.
    UntrustedItemAsks,
    /// The agent runs on its own configuration, not the user's, and credentials are redacted.
    AgentConfigIsolated,
    /// The token limit stops the run while it runs, not after.
    TokenLimitWhileRunning,
    /// Cancelling kills everything the agent started.
    KillEverythingOnCancel,
    /// The output rules (a Planner writes only the plan, a run that changed nothing fails).
    OutputRules,
    /// Checks that changed since the run began hold the merge.
    ChecksChangedHoldsMerge,
    /// Secrets are stripped from everything the run produces.
    Redaction,
    /// The agent's steps are visible for review and evaluation.
    StepsVisible,
}

impl ParityRule {
    /// Every rule, in the order the matrix and the run page list them.
    pub const ALL: [ParityRule; 11] = [
        ParityRule::SandboxOrNoRun,
        ParityRule::NoSandboxMeansAsk,
        ParityRule::EgressByAllowList,
        ParityRule::UntrustedItemAsks,
        ParityRule::AgentConfigIsolated,
        ParityRule::TokenLimitWhileRunning,
        ParityRule::KillEverythingOnCancel,
        ParityRule::OutputRules,
        ParityRule::ChecksChangedHoldsMerge,
        ParityRule::Redaction,
        ParityRule::StepsVisible,
    ];

    /// The rule in the words a person reads.
    pub const fn label(self) -> &'static str {
        match self {
            ParityRule::SandboxOrNoRun => "Runs in a sandbox, or does not run",
            ParityRule::NoSandboxMeansAsk => "Without a sandbox, asks before each action",
            ParityRule::EgressByAllowList => "Reaches the network only through the allow-list",
            ParityRule::UntrustedItemAsks => {
                "An untrusted item starts out asking before each action and without web tools"
            }
            ParityRule::AgentConfigIsolated => {
                "Uses its own configuration, not yours, and credentials are redacted"
            }
            ParityRule::TokenLimitWhileRunning => "Stops at the token limit while it runs",
            ParityRule::KillEverythingOnCancel => "Cancelling stops everything it started",
            ParityRule::OutputRules => "Its output follows the role's rules",
            ParityRule::ChecksChangedHoldsMerge => "Checks that changed hold the merge",
            ParityRule::Redaction => "Secrets are removed from everything it produces",
            ParityRule::StepsVisible => "Its steps are visible",
        }
    }
}

/// Who meets a [`ParityRule`] for one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum ParityCell {
    /// The agent's own CLI enforces it.
    Cli,
    /// The runner enforces it from outside the agent.
    Runner,
    /// The agent's daemon enforces it (docket).
    Delegated { to: Cow<'static, str> },
    /// Nobody does today. The reason names the card that will, or starts with
    /// [`ParityCell::BY_DESIGN`] when no card ever can.
    Missing { reason: Cow<'static, str> },
}

impl ParityCell {
    /// The prefix of a `Missing` reason that nothing can ever fill, which the parity test accepts.
    pub const BY_DESIGN: &'static str = "by design:";

    pub const fn delegated(to: &'static str) -> Self {
        ParityCell::Delegated {
            to: Cow::Borrowed(to),
        }
    }

    pub const fn missing(reason: &'static str) -> Self {
        ParityCell::Missing {
            reason: Cow::Borrowed(reason),
        }
    }

    /// Whether somebody enforces the rule.
    pub fn is_met(&self) -> bool {
        !matches!(self, ParityCell::Missing { .. })
    }

    /// Whether the rule is missing and no card can ever meet it.
    pub fn is_by_design(&self) -> bool {
        matches!(self, ParityCell::Missing { reason } if reason.starts_with(Self::BY_DESIGN))
    }
}

/// One agent's column of the parity matrix, as a harness descriptor declares it.
pub type ParityColumn = [(ParityRule, ParityCell)];

/// A rule and who meets it, as the runner reports it to the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityLine {
    pub rule: ParityRule,
    /// The rule in words, so a reader needs no table of its own.
    pub label: String,
    #[serde(flatten)]
    pub cell: ParityCell,
}

impl ParityLine {
    pub fn new(rule: ParityRule, cell: ParityCell) -> Self {
        Self {
            rule,
            label: rule.label().to_owned(),
            cell,
        }
    }
}

/// Checks that a column answers every rule once and that no answer is empty: a `Missing` needs a
/// reason, a `Delegated` needs a name. The first problem found names the rule.
pub fn validate_parity(column: &ParityColumn) -> Result<(), String> {
    for rule in ParityRule::ALL {
        let mut answers = column.iter().filter(|(asked, _)| *asked == rule);
        let Some((_, cell)) = answers.next() else {
            return Err(format!("{rule:?} has no answer"));
        };
        if answers.next().is_some() {
            return Err(format!("{rule:?} is answered more than once"));
        }
        match cell {
            ParityCell::Missing { reason } if reason.trim().is_empty() => {
                return Err(format!("{rule:?} is missing with no reason"));
            }
            ParityCell::Delegated { to } if to.trim().is_empty() => {
                return Err(format!("{rule:?} is delegated to nobody"));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "capabilities/tests.rs"]
mod tests;
