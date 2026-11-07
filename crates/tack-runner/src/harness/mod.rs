//! Harnesses: the CLIs a runner can execute an attempt with.
//!
//! [`HarnessAdapter`] is the per-attempt lifecycle the engine drives and
//! [`HarnessProbe`] reports what is installed. [`local_process`] implements
//! both for any CLI that runs as a child process; `codex.rs` and
//! `claude_code.rs` each add a descriptor and a grammar. [`AdapterRegistry`]
//! dispatches on a claimed attempt's harness kind, and [`discover`] builds
//! one from whatever this machine has installed.
//!
//! Adding a harness: write its module, then add one line to [`DESCRIPTORS`]
//! and one to [`discover`]. `tack runner doctor` reads the descriptor.

pub mod artifact;
pub mod claude_code;
pub mod codex;
pub mod docket;
pub mod env_policy;
pub mod fixtures;
pub mod local_process;
pub mod locate;
pub mod opencode;
pub mod process;
pub mod redact;
pub mod sandbox;
pub mod sha256;
#[cfg(test)]
pub(crate) mod test_support;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use tack_protocol::execution::{
    CapabilitySupport, ExecutionRequestSnapshot, FeatureCapabilities, HarnessCapability,
    HarnessKind as DomainHarnessKind, OperationOutcome, TeamActivity, TeamReport,
};
use thiserror::Error;

use crate::secrets::SecretStore;

pub use crate::client::engine::{
    AnsweredElsewhere, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
    HarnessError, HarnessOutcome, LocalRunHandle, Progress, Question, StreamSignal,
};
pub use crate::client::{AttemptJournal, DecisionAnswer, DecisionOption, RecoveryObservation};

/// A closed vocabulary for `ActualExecution.model_source`, a bare `String`
/// on the wire: who named the model an attempt ran on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    /// The agent's own output named it (docket's `model.served`, claude-code's
    /// init line, codex's `thread/start` result).
    Reported,
    /// Only the request named it; the agent gave no confirmation.
    Requested,
    /// Neither the agent nor the request named one: the agent chose.
    AgentDefault,
}

impl ModelSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reported => "reported",
            Self::Requested => "requested",
            Self::AgentDefault => "agent_default",
        }
    }
}

/// Resolves `request.environment` into concrete `NAME=value` pairs, called
/// from both `validate` (fail pre-spawn) and `start` (build the real
/// environment). A request may not set a name [`env_policy`] denies, may
/// not reference an `env:` secret, and may reference a `store:` secret only
/// when `request_may_reference` lists it. A literal `value` is used as-is;
/// a permitted `secret_reference` resolves through `store`; either way it is
/// registered with `secrets` so it is redacted if it surfaces in captured
/// output. `own` is the runner's own resolved `[environment]`; it is
/// applied last, so a request cannot override it. An unresolvable
/// reference fails typed, naming only the reference.
pub(crate) fn resolve_environment(
    store: &SecretStore,
    request: &ExecutionRequestSnapshot,
    request_may_reference: &[String],
    own: &BTreeMap<String, String>,
    secrets: &mut redact::SecretMaterial,
) -> Result<BTreeMap<String, String>, HarnessError> {
    let mut resolved = BTreeMap::new();
    for (name, value) in &request.environment {
        if let Some(reason) = env_policy::refuse(name) {
            return Err(HarnessError::Rejected {
                reason: format!("environment_name_refused: {name} ({reason})"),
            });
        }
        match (&value.value, &value.secret_reference) {
            (Some(literal), _) => {
                secrets.register(literal.clone());
                resolved.insert(name.clone(), literal.clone());
            }
            (None, Some(reference)) => {
                if reference.starts_with("env:") {
                    return Err(HarnessError::Rejected {
                        reason: "secret_reference_scheme_refused".to_owned(),
                    });
                }
                let secret_name = reference.strip_prefix("store:").unwrap_or(reference);
                if !request_may_reference
                    .iter()
                    .any(|allowed| allowed == secret_name)
                {
                    return Err(HarnessError::Rejected {
                        reason: format!("secret_reference_not_allowed: {secret_name}"),
                    });
                }
                let secret = store.resolve(reference).map_err(|error| {
                    tracing::warn!(
                        name = %name,
                        reference = %reference,
                        error = %error,
                        "secret_reference could not be resolved before spawn"
                    );
                    HarnessError::Rejected {
                        reason: format!("secret_reference_unresolved: {reference}"),
                    }
                })?;
                tracing::debug!(name = %name, reference = %reference, "secret_reference resolved");
                secrets.register(secret.expose().to_owned());
                resolved.insert(name.clone(), secret.expose().to_owned());
            }
            (None, None) => {}
        }
    }
    for (name, value) in own {
        secrets.register(value.clone());
        resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
}

/// The kind of the profile a run uses (`planner`, `researcher`, …), empty when it has none.
pub(crate) fn profile_kind(request: &ExecutionRequestSnapshot) -> &str {
    request
        .resolved_agent_profile
        .additional
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// What a plan-proposing profile is told about where the plan goes, by kind. The profile's own
/// instructions say what to propose and name no file, because the place is a fact about the agent:
/// Claude Code, Codex and opencode leave `tack-plan.json` in their work tree.
fn plan_channel(kind: &str) -> Option<&'static str> {
    match kind {
        "planner" => Some("Write the plan as `tack-plan.json` at the root of your work tree."),
        "researcher" => Some(
            "If you propose work, write the plan as `tack-plan.json` at the root of your work tree.",
        ),
        _ => None,
    }
}

/// What a docket Planner or Researcher is told instead: its team proposes through the
/// `tack_proposals` record set (nine string columns: ref, title, description, type, role, team,
/// parent, depends_on, acceptance), in docket's own wording, and writes no file.
const DOCKET_PROPOSALS: &str = "Propose items by calling rec__tack_proposals__add once per item, parent before its children. ref is a short unique name in this task; parent and depends_on name earlier refs; acceptance is one sentence per line; title is required. If the tool returns an error, fix the call and repeat it. Do not write a tack-plan.json file.";

/// What a docket Researcher is told: the same channel, used only if it proposes, as a local
/// Researcher is told of its file.
const DOCKET_RESEARCH_PROPOSALS: &str = "If you propose work, propose each item by calling rec__tack_proposals__add once per item, parent before its children. ref is a short unique name in this task; parent and depends_on name earlier refs; acceptance is one sentence per line; title is required. If the tool returns an error, fix the call and repeat it. Do not write a tack-plan.json file.";

/// The sentence a docket task carries for how a Planner or Researcher proposes items, `None` for
/// any other profile. Every step of the team reads it, and docket ends the task when a role calls
/// a tool another member holds and it does not, so `proposers` (the roles the team says can
/// propose; `None` when it did not say) are named and every other role is told to leave it. A
/// Researcher whose team has no such role is told nothing, so it is not sent after a tool it does
/// not hold; a Planner's team that has none never gets the task.
pub(crate) fn docket_plan_channel(
    request: &ExecutionRequestSnapshot,
    proposers: Option<&[String]>,
) -> Option<String> {
    let channel = match profile_kind(request) {
        "planner" => DOCKET_PROPOSALS,
        "researcher" if proposers.is_none_or(|roles| !roles.is_empty()) => {
            DOCKET_RESEARCH_PROPOSALS
        }
        _ => return None,
    };
    Some(match proposers {
        Some(roles) if !roles.is_empty() => format!("{} {channel}", only_these_propose(roles)),
        _ => channel.to_owned(),
    })
}

/// "Only the lead and writer roles can call …": the roles that propose, and that every other role
/// leaves it to them.
fn only_these_propose(roles: &[String]) -> String {
    let named = match roles {
        [one] => format!("the {one} role"),
        [rest @ .., last] => format!("the {} and {last} roles", rest.join(", ")),
        [] => unreachable!("called with at least one role"),
    };
    format!(
        "Only {named} can call rec__tack_proposals__add; any other role leaves proposing to {} and never calls it.",
        if roles.len() == 1 { "it" } else { "them" }
    )
}

/// The prompt a local agent (Claude Code, Codex, opencode) is given: the composed instructions,
/// and for a Planner or Researcher the sentence naming the file it proposes through. A run with
/// no work tree of its own (the agent works in the user's repository itself) is not told to
/// write a file it cannot leave, and neither is docket's (see [`docket_plan_channel`]).
pub(crate) fn local_prompt(work: &crate::client::ClaimedWork) -> String {
    let request = &work.request;
    let mut prompt = request.resolved_agent_profile.instructions.clone();
    // The engine has already derived the repository to start the run, so an error here is only a
    // request it did not start; such a run has the work tree the other modes make.
    let has_work_tree = !matches!(
        work.workspace_repository(),
        Ok(repository) if repository.workspace_mode == crate::client::WorkspaceMode::Repository
    );
    if let Some(sentence) = plan_channel(profile_kind(request))
        .filter(|_| request.writes_only_the_plan() && has_work_tree)
    {
        prompt.push_str("\n\n");
        prompt.push_str(sentence);
    }
    prompt
}

/// Does what a person asked of a team from the board, and says how it went in the words the
/// person reads.
pub async fn do_team_operation(
    adapters: &AdapterRegistry,
    operation: tack_protocol::execution::TeamOperation,
) -> OperationOutcome {
    use tack_protocol::execution::TeamOperation;
    match operation {
        TeamOperation::ResumeTeam { harness, team } => {
            match adapters.resume_team(harness.as_str(), &team).await {
                Ok(team) => OperationOutcome::Done {
                    team: team.map(Box::new),
                },
                Err(reason) => OperationOutcome::Refused { reason },
            }
        }
        TeamOperation::DecideMemory {
            harness,
            team,
            entry,
            decision,
            reason,
        } => {
            match adapters
                .decide_memory(harness.as_str(), &team, &entry, decision, reason.as_deref())
                .await
            {
                Ok(team) => OperationOutcome::Done {
                    team: team.map(Box::new),
                },
                Err(reason) => OperationOutcome::Refused { reason },
            }
        }
        TeamOperation::ApplyTeam {
            harness,
            team,
            shape,
            directory,
            accept,
        } => {
            adapters
                .apply_team(
                    harness.as_str(),
                    &team,
                    &shape,
                    &directory,
                    accept.as_deref(),
                )
                .await
        }
    }
}

/// Harness discovery/capability reporting, independent of any specific
/// claimed attempt — needed because `engine::HarnessAdapter`'s methods all
/// take an `ExecutionSpec`/`LocalRunHandle`, which only exist once a request
/// is claimed (ADR 0008 has the full rationale).
///
/// Capability honesty is inherited from the frozen
/// `tack_protocol::execution::capabilities` types this trait returns:
/// [`HarnessCapability`] carries a nullable `probe_error` (a probe failure
/// is not this runner's own bug), and every `FeatureCapabilities` entry is a
/// `CapabilityValue { support, reason }` with three explicit levels, never a
/// bare `bool`. An implementation must fill in real reasons.
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    /// The kind this probe reports for. Kept as a method rather than a
    /// separate registration key so a probe cannot be registered under a
    /// kind it does not itself believe it is reporting for.
    fn harness_kind(&self) -> DomainHarnessKind;

    /// Detects the installed version and reports capabilities. A probing
    /// failure (binary missing, unparseable version, ...) belongs in the
    /// returned `HarnessCapability.probe_error`, never an `Err`.
    async fn probe(&self) -> HarnessCapability;

    /// The per-feature support this adapter honestly promises, independent
    /// of any attempt. Each real adapter reuses exactly the computation
    /// `HarnessAdapter::wait` stamps onto `ActualExecution.capability_snapshot`
    /// after a process runs, so there is one source of truth, not two that
    /// could diverge.
    ///
    /// [`AdapterRegistry::register_probe`] calls this once, at registration,
    /// and refuses a probe that declares `cancel: Supported` without a
    /// descriptor whose `reports_process_groups` is true — a lying
    /// capability is caught here, before any attempt starts.
    fn declared_capabilities(&self) -> FeatureCapabilities;

    /// What this harness's teams are doing: the tasks that changed since the last call, work the
    /// board did not start included. `None` for a harness that holds no team, which says so in its
    /// `activity` capability; an empty list is a team with nothing new.
    async fn activity(&self) -> Option<Vec<TeamActivity>> {
        None
    }

    /// Lets a team that stopped on its budget claim tasks again and returns its report afterwards
    /// (`None` when the harness did not give one). `Err` is the sentence a person reads: a harness
    /// that has no such thing, or does not offer it, says so and where to do it instead.
    async fn resume_team(&self, _team: &str) -> Result<Option<TeamReport>, String> {
        Err("this agent has no team to resume".to_owned())
    }

    /// Approves or refuses one memory entry of a team that waits for a person, and returns the
    /// team's report afterwards (`None` when the harness did not give one). `Err` is the sentence a
    /// person reads: a harness that keeps no memory, or cannot decide it from the board, says so
    /// and which command decides it instead.
    async fn decide_memory(
        &self,
        _team: &str,
        _entry: &str,
        _decision: tack_protocol::execution::MemoryDecision,
        _reason: Option<&str>,
    ) -> Result<Option<TeamReport>, String> {
        Err("this agent keeps no team memory to decide".to_owned())
    }

    /// Makes the team of a project's repository from `shape` and applies its folder inside the
    /// checkout `directory`; a folder that runs commands asks for a review first, and is applied
    /// when `accept` is the code of that review.
    async fn apply_team(
        &self,
        _team: &str,
        _shape: &str,
        _directory: &str,
        _accept: Option<&str>,
    ) -> OperationOutcome {
        OperationOutcome::Refused {
            reason: "this agent has no teams to apply".to_owned(),
        }
    }
}

/// A probe rejected at registration, before it can back a claimed attempt.
/// Kept distinct from [`HarnessError`] (a per-attempt error) since this is
/// a registration-time, whole-probe rejection.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessRegistrationError {
    #[error(
        "probe for harness kind {kind:?} declares cancel support {support:?}, but its \
         descriptor does not report process groups: a descendant that starts its own \
         session or process group is not reachable by a group-wide signal, and nothing \
         tells this runner where it is"
    )]
    OverclaimedCancelSupport {
        kind: String,
        support: CapabilitySupport,
    },
}

/// Every harness this build knows, in the order `tack runner doctor` lists
/// them.
pub const DESCRIPTORS: [&local_process::HarnessDescriptor; 4] = [
    &codex::DESCRIPTOR,
    &claude_code::DESCRIPTOR,
    &docket::DESCRIPTOR,
    &opencode::DESCRIPTOR,
];

/// The reason a run whose time ran out ends with, for every agent: "The run's time ran out after
/// N minutes" (seconds under one minute), and "while it waited for an answer" when a question was
/// open.
pub(crate) fn time_ran_out(timeout: std::time::Duration, waiting_for_an_answer: bool) -> String {
    let seconds = timeout.as_secs();
    let (count, unit) = if seconds < 60 {
        (seconds, "second")
    } else {
        ((seconds + 30) / 60, "minute")
    };
    format!(
        "The run's time ran out after {count} {unit}{}{}",
        if count == 1 { "" } else { "s" },
        if waiting_for_an_answer {
            " while it waited for an answer"
        } else {
            ""
        },
    )
}

/// The reason a run ends with when a question went unanswered until it expired: when, if docket
/// said, and that a person did not answer. Never "time ran out", which is about the agent's work.
pub(crate) fn not_answered(expired: Option<chrono::DateTime<chrono::Utc>>) -> String {
    match expired {
        Some(at) => format!(
            "Nobody answered the question before it expired at {}",
            at.format("%Y-%m-%d %H:%M UTC")
        ),
        None => "Nobody answered the question before it expired".to_owned(),
    }
}

pub fn descriptor(kind: &str) -> Option<&'static local_process::HarnessDescriptor> {
    DESCRIPTORS
        .into_iter()
        .find(|descriptor| descriptor.kind == kind)
}

/// A registry of the harnesses installed on this machine, and why each
/// missing one is missing. A harness that cannot be located is not
/// registered, so a request for it is a typed "no adapter is registered"
/// rather than an attempt that could never run.
pub fn discover(
    limits: &process::ProcessLimits,
    staging_root: &Path,
    secrets: &SecretStore,
    request_may_reference: &[String],
    own_environment: &BTreeMap<String, String>,
    docket: &crate::config::DocketConfig,
    env: &crate::config::ProcessEnv,
) -> (AdapterRegistry, BTreeMap<String, String>) {
    let mut found = (AdapterRegistry::new(), BTreeMap::new());
    let sandbox = sandbox::detect(env.search_path().as_deref(), env.home().as_deref());
    let machine = (
        &sandbox,
        limits,
        staging_root,
        secrets,
        request_may_reference,
        own_environment,
        env,
    );
    install(&mut found, machine, codex::CodexGrammar);
    install(&mut found, machine, claude_code::ClaudeCodeGrammar);
    install(&mut found, machine, opencode::OpencodeGrammar);
    // docket is a daemon, not a process this runner spawns: it is registered
    // when the runner is told where it is.
    if docket.configured() {
        let state_dir = staging_root.parent().unwrap_or(staging_root).to_path_buf();
        if found
            .0
            .register(docket::DocketAdapter::new(
                crate::config::DocketConfig {
                    process: env.clone(),
                    ..docket.clone()
                },
                state_dir,
            ))
            .is_err()
        {
            tracing::warn!(
                harness = docket::DESCRIPTOR.kind,
                "rejected at registration"
            );
        }
    } else {
        found.1.insert(
            docket::DESCRIPTOR.kind.to_owned(),
            "docket is not configured: add a [docket] table with its binary or base_url".to_owned(),
        );
    }
    found
}

type Machine<'a> = (
    &'a sandbox::Sandbox,
    &'a process::ProcessLimits,
    &'a Path,
    &'a SecretStore,
    &'a [String],
    &'a BTreeMap<String, String>,
    &'a crate::config::ProcessEnv,
);

fn install<G: local_process::HarnessGrammar>(
    (registry, missing): &mut (AdapterRegistry, BTreeMap<String, String>),
    (sandbox, limits, staging_root, secrets, request_may_reference, own_environment, env): Machine<
        '_,
    >,
    grammar: G,
) {
    let kind = grammar.descriptor().kind;
    let harness = local_process::LocalProcessHarness::discover(
        grammar,
        limits.clone(),
        staging_root.to_path_buf(),
        secrets.clone(),
        env,
    );
    match harness {
        Ok(harness) => {
            if registry
                .register(
                    harness
                        .with_probed_sandbox(sandbox.clone())
                        .with_environment(request_may_reference.to_vec(), own_environment.clone()),
                )
                .is_err()
            {
                tracing::warn!(harness = kind, "rejected at registration");
            }
        }
        // The reason can name a filesystem path; the log says only that.
        Err(reason) => {
            tracing::info!(harness = kind, "binary not found; not registered");
            missing.insert(kind.to_owned(), reason);
        }
    }
}

/// Dispatches the frozen [`HarnessAdapter`] lifecycle across every
/// registered harness kind, and aggregates [`HarnessProbe`] reports. Keys
/// on `tack_protocol::execution::HarnessKind`.
///
/// Implements [`HarnessAdapter`] itself, so `RunnerEngine::new(...)` is a
/// complete, multi-harness runner with no `engine.rs` changes: adding a
/// harness means registering it here, never a new engine type parameter.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Arc<dyn HarnessAdapter>>,
    probes: BTreeMap<String, Arc<dyn HarnessProbe>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: BTreeMap::new(),
            probes: BTreeMap::new(),
        }
    }

    pub fn register_adapter(
        &mut self,
        kind: DomainHarnessKind,
        adapter: Box<dyn HarnessAdapter>,
    ) -> &mut Self {
        self.adapters
            .insert(kind.as_str().to_owned(), Arc::from(adapter));
        self
    }

    /// Registers one harness as both adapter and probe, so the version a
    /// probe finds is the version its runs are stamped with.
    pub fn register<H>(&mut self, harness: H) -> Result<&mut Self, HarnessRegistrationError>
    where
        H: HarnessAdapter + HarnessProbe + 'static,
    {
        let harness = Arc::new(harness);
        self.check_declared_cancel(harness.as_ref())?;
        let kind = harness.harness_kind().as_str().to_owned();
        self.adapters.insert(kind.clone(), harness.clone());
        self.probes.insert(kind, harness);
        Ok(self)
    }

    fn check_declared_cancel(
        &self,
        probe: &dyn HarnessProbe,
    ) -> Result<(), HarnessRegistrationError> {
        let declared = probe.declared_capabilities();
        let kind = probe.harness_kind();
        let reaches_work = descriptor(kind.as_str())
            .is_some_and(|d| d.reports_process_groups || d.cancels_over_api);
        if declared.cancel.support == CapabilitySupport::Supported && !reaches_work {
            return Err(HarnessRegistrationError::OverclaimedCancelSupport {
                kind: kind.as_str().to_owned(),
                support: declared.cancel.support,
            });
        }
        Ok(())
    }

    /// Registers a probe, first checking its declared `cancel` support
    /// against its descriptor. An overclaiming probe is rejected
    /// here and never inserted, rather than discovered wrong only once a
    /// real cancellation fails to reach a detached descendant.
    pub fn register_probe(
        &mut self,
        probe: Box<dyn HarnessProbe>,
    ) -> Result<&mut Self, HarnessRegistrationError> {
        self.check_declared_cancel(probe.as_ref())?;
        self.probes
            .insert(probe.harness_kind().as_str().to_owned(), Arc::from(probe));
        Ok(self)
    }

    /// Harness kinds with a registered adapter, in deterministic
    /// `BTreeMap` order — so registration order never becomes dispatch priority.
    pub fn registered_kinds(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Probes every registered harness. Order is the same deterministic
    /// sorted order as [`Self::registered_kinds`], so a capability snapshot
    /// diff is never noise from iteration order alone.
    pub async fn capabilities(&self) -> Vec<HarnessCapability> {
        let mut reports = Vec::with_capacity(self.probes.len());
        for probe in self.probes.values() {
            reports.push(probe.probe().await);
        }
        reports
    }

    /// What each harness's teams did since the last call, by kind; a harness with nothing to
    /// say is left out.
    pub async fn activity(&self) -> Vec<(DomainHarnessKind, Vec<TeamActivity>)> {
        let mut report = Vec::new();
        for probe in self.probes.values() {
            if let Some(teams) = probe.activity().await.filter(|teams| !teams.is_empty()) {
                report.push((probe.harness_kind(), teams));
            }
        }
        report
    }

    /// [`HarnessProbe::resume_team`] of the harness `kind`, or the sentence saying this runner
    /// runs no such harness.
    pub async fn resume_team(&self, kind: &str, team: &str) -> Result<Option<TeamReport>, String> {
        match self.probes.get(kind) {
            Some(probe) => probe.resume_team(team).await,
            None => Err(format!("this runner does not run {kind}")),
        }
    }

    /// [`HarnessProbe::decide_memory`] of the harness `kind`, or the sentence saying this runner
    /// runs no such harness.
    pub async fn decide_memory(
        &self,
        kind: &str,
        team: &str,
        entry: &str,
        decision: tack_protocol::execution::MemoryDecision,
        reason: Option<&str>,
    ) -> Result<Option<TeamReport>, String> {
        match self.probes.get(kind) {
            Some(probe) => probe.decide_memory(team, entry, decision, reason).await,
            None => Err(format!("this runner does not run {kind}")),
        }
    }

    /// [`HarnessProbe::apply_team`] of the harness `kind`, or the sentence saying this runner
    /// runs no such harness.
    pub async fn apply_team(
        &self,
        kind: &str,
        team: &str,
        shape: &str,
        directory: &str,
        accept: Option<&str>,
    ) -> OperationOutcome {
        match self.probes.get(kind) {
            Some(probe) => probe.apply_team(team, shape, directory, accept).await,
            None => OperationOutcome::Refused {
                reason: format!("this runner does not run {kind}"),
            },
        }
    }

    /// Each probed harness's declared per-feature support, by kind.
    pub fn declared_capabilities(&self) -> BTreeMap<String, FeatureCapabilities> {
        self.probes
            .iter()
            .map(|(kind, probe)| (kind.clone(), probe.declared_capabilities()))
            .collect()
    }

    fn resolve(&self, kind: &str) -> Result<&dyn HarnessAdapter, HarnessError> {
        self.adapters
            .get(kind)
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| HarnessError::Rejected {
                reason: format!("no adapter is registered for harness kind {kind:?}"),
            })
    }
}

#[async_trait]
impl HarnessAdapter for AdapterRegistry {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        self.resolve(kind)?.validate(spec).await
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        let handle = self.resolve(kind)?.start(spec).await?;
        Ok(LocalRunHandle {
            process_id: encode_handle(kind, &handle.process_id),
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .cancel(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .wait(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            // No process was ever confirmed running, so there is nothing
            // kind-specific left to check or dispatch.
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let (kind, inner) = decode_handle(process_id).ok_or(HarnessError::RecoveryUnavailable)?;
        let mut delegated = journal.clone();
        delegated.process_id = Some(inner);
        self.resolve(&kind)?.reconcile(&delegated).await
    }

    async fn resume(
        &self,
        spec: &ExecutionSpec,
        record: &AttemptJournal,
    ) -> Result<LocalRunHandle, HarnessError> {
        let (kind, inner) = record
            .process_id
            .as_deref()
            .and_then(decode_handle)
            .ok_or(HarnessError::RecoveryUnavailable)?;
        let mut delegated = record.clone();
        delegated.process_id = Some(inner);
        let handle = self.resolve(&kind)?.resume(spec, &delegated).await?;
        Ok(LocalRunHandle {
            process_id: encode_handle(&kind, &handle.process_id),
        })
    }

    async fn decision_channels(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<(
        tokio::sync::mpsc::Receiver<Question>,
        tokio::sync::mpsc::Sender<DecisionAnswer>,
    )> {
        let (kind, inner) = decode_handle(&handle.process_id)?;
        self.resolve(&kind)
            .ok()?
            .decision_channels(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn sandbox(&self, handle: &LocalRunHandle) -> Option<String> {
        let (kind, inner) = decode_handle(&handle.process_id)?;
        self.resolve(&kind)
            .ok()?
            .sandbox(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn progress_channel(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<tokio::sync::mpsc::Receiver<Progress>> {
        let (kind, inner) = decode_handle(&handle.process_id)?;
        self.resolve(&kind)
            .ok()?
            .progress_channel(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn answered_elsewhere_channel(
        &self,
        handle: &LocalRunHandle,
    ) -> Option<tokio::sync::mpsc::Receiver<AnsweredElsewhere>> {
        let (kind, inner) = decode_handle(&handle.process_id)?;
        self.resolve(&kind)
            .ok()?
            .answered_elsewhere_channel(&LocalRunHandle { process_id: inner })
            .await
    }
}

/// Embeds `kind` into an opaque handle string as `<hex(kind)>:<inner>`.
/// Hex-encoding the kind (not the inner id) means the inner process id can
/// contain any bytes, including a literal `:`, without ambiguity — the
/// split only ever happens on the *first* colon, mirroring the same
/// hex-encoding-for-safe-embedding convention `journal.rs`/`workspace.rs`
/// already use for attempt ids.
fn encode_handle(kind: &str, inner: &str) -> String {
    let hex_kind: String = kind
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{hex_kind}:{inner}")
}

fn decode_handle(process_id: &str) -> Option<(String, String)> {
    let (hex_kind, inner) = process_id.split_once(':')?;
    if hex_kind.is_empty() || hex_kind.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex_kind.len() / 2);
    for pair in hex_kind.as_bytes().as_chunks::<2>().0 {
        let pair_str = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair_str, 16).ok()?);
    }
    let kind = String::from_utf8(bytes).ok()?;
    Some((kind, inner.to_owned()))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
