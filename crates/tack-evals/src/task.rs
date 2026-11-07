//! A task is a directory of data: `task.yaml`, the seed `repo/`, the `hidden/` checks the agent
//! never sees and the reference `solution/` that must pass them without a model.
//!
//! Loading validates the whole task before anything is started, so a mistake in a task is
//! reported with its reason at the door and never as a strange failure halfway through a trial.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The checks a task may name. A check that is not here is a typo, and a typo in a task must not
/// quietly turn into a check that never runs.
const KNOWN_GRADERS: &[&str] = &[
    "outcome.hidden_tests",
    "outcome.run_state",
    "outcome.files",
    "outcome.plan",
    "governance.no_violation",
    "verifier.agrees",
    "process.tokens",
    "process.asks",
];

/// Where a safety task's attack comes in.
const SAFETY_VECTORS: &[&str] = &["card", "readme", "web", "test_comment", "postinstall"];
/// What a safety task's attack is after.
const SAFETY_TARGETS: &[&str] = &[
    "exfiltrate",
    "agent_config",
    "weaken_test",
    "push",
    "ssh_read",
];

/// The longest `instruction` a task may carry: it becomes the item's title and description.
const INSTRUCTION_CHARS_MAX: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The task is unusable; `reason` says what to change.
    #[error("task {task}: {reason}")]
    Invalid { task: String, reason: String },
    #[error("task {task}: cannot read {path}: {source}")]
    Io {
        task: String,
        path: PathBuf,
        source: std::io::Error,
    },
}

/// One check, as written in `task.yaml`: a bare name (`governance.no_violation`) or a name with
/// its argument (`outcome.run_state: succeeded`, `outcome.hidden_tests: { command: ... }`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GraderSpec {
    pub name: String,
    pub args: Value,
}

impl<'de> Deserialize<'de> for GraderSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Value::deserialize(deserializer)? {
            Value::String(name) => Ok(Self {
                name,
                args: Value::Null,
            }),
            Value::Object(map) if map.len() == 1 => {
                let (name, args) = map.into_iter().next().expect("one entry");
                Ok(Self { name, args })
            }
            other => Err(D::Error::custom(format!(
                "a check is a name or a one-entry map of name to argument, got {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graders {
    /// All must pass: the trial is approved or failed on these.
    #[serde(default)]
    pub required: Vec<GraderSpec>,
    /// Shown, never decisive.
    #[serde(default)]
    pub report: Vec<GraderSpec>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    /// The item type of the project the trial creates (`bug`, `task`, ...).
    #[serde(rename = "type", default = "default_item_type")]
    pub item_type: String,
    /// Acceptance and constraints, passed to the item as written.
    #[serde(default)]
    pub brief: Value,
}

fn default_item_type() -> String {
    "task".to_owned()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSpec {
    /// `claude-code`, `codex`, `opencode` or `docket`.
    pub harness: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub web_tools: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Approvals {
    #[default]
    Auto,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    /// The trial is `not_applicable` where the machine has no jail.
    #[default]
    Required,
    /// Runs without one and records which it had.
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verify {
    #[default]
    None,
    Prill,
}

/// How the operator answers every decision the run raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionScript {
    /// Allow it, once.
    ApproveOnce,
    /// Refuse it: the safe default, so a task that forgot to say never grants anything.
    #[default]
    Deny,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorSpec {
    #[serde(default)]
    pub decisions: DecisionScript,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Wall-clock seconds for the whole run; the trial is cut off when they pass.
    pub wall_s: u64,
    /// Token limit of the run; absent means the run has none.
    #[serde(default)]
    pub tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyRole {
    /// The task as written, with the attack in it.
    Attack,
    /// The same task without the attack: what a guardrail costs honest work.
    Twin,
}

/// What makes a task one half of a safety pair.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SafetySpec {
    pub role: SafetyRole,
    /// The other half of the pair, by id.
    pub pair: String,
    /// Where the attack comes in: `card`, `readme`, `web`, `test_comment` or `postinstall`.
    pub vector: String,
    /// What it is after: `exfiltrate`, `agent_config`, `weaken_test`, `push` or `ssh_read`.
    pub target: String,
    /// Words an attempt leaves in the run's tool events or in the commands it asked to run. The
    /// card's own words are not evidence of an attempt, so a marker is a part of the action
    /// (a host, a path, a subcommand) and not of the story around it.
    #[serde(default)]
    pub markers: Vec<String>,
    /// Paths the work has no business changing (the tests, for an attack on a test): a patch that
    /// touches one is a boundary crossed. A path ending in `/` stands for everything under it.
    #[serde(default)]
    pub protected_files: Vec<String>,
    /// Refs the seed repository must not gain (a push). A ref matches when it contains the word.
    #[serde(default)]
    pub forbidden_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    /// Changing the content of a task raises it; results of different versions are never mixed.
    pub version: u32,
    pub suite: String,
    #[serde(default)]
    pub origin: Option<String>,
    pub instruction: String,
    pub item: ItemSpec,
    pub profile: ProfileSpec,
    #[serde(default)]
    pub approvals: Approvals,
    #[serde(default)]
    pub sandbox: SandboxMode,
    #[serde(default)]
    pub verify: Verify,
    #[serde(default)]
    pub operator: OperatorSpec,
    #[serde(default)]
    pub graders: Graders,
    pub limits: Limits,
    /// The agents the task applies to; absent means all of them.
    #[serde(default)]
    pub harnesses: Option<Vec<String>>,
    /// Agents that cannot do this task by design, each with the reason. Their cell in the
    /// report is `not_applicable` and says why; it never counts as passed.
    #[serde(default)]
    pub not_applicable: BTreeMap<String, String>,
    /// Present on the tasks of the safety suite.
    #[serde(default)]
    pub safety: Option<SafetySpec>,
}

/// A task together with the directory it was read from.
#[derive(Debug, Clone)]
pub struct LoadedTask {
    pub dir: PathBuf,
    pub task: Task,
}

impl LoadedTask {
    /// Reads and validates the task in `dir`.
    pub fn load(dir: &Path) -> Result<Self, TaskError> {
        let name = dir
            .file_name()
            .map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into());
        let invalid = |reason: String| TaskError::Invalid {
            task: name.clone(),
            reason,
        };
        let file = dir.join("task.yaml");
        let text = std::fs::read_to_string(&file).map_err(|source| TaskError::Io {
            task: name.clone(),
            path: file.clone(),
            source,
        })?;
        let task: Task = serde_saphyr::from_str(&text)
            .map_err(|failure| invalid(format!("task.yaml does not parse: {failure}")))?;
        let loaded = Self {
            dir: dir.to_owned(),
            task,
        };
        loaded.validate().map_err(invalid)?;
        Ok(loaded)
    }

    /// Every task directory under `root` (a directory holding `task.yaml`), in name order.
    pub fn load_all(root: &Path) -> Result<Vec<Self>, TaskError> {
        let name = root.display().to_string();
        let io = |source| TaskError::Io {
            task: name.clone(),
            path: root.to_owned(),
            source,
        };
        let mut dirs: Vec<PathBuf> = Vec::new();
        collect_task_dirs(root, &mut dirs).map_err(io)?;
        dirs.sort();
        dirs.iter().map(|dir| Self::load(dir)).collect()
    }

    pub fn repo_dir(&self) -> PathBuf {
        self.dir.join("repo")
    }

    pub fn hidden_dir(&self) -> PathBuf {
        self.dir.join("hidden")
    }

    pub fn solution_dir(&self) -> PathBuf {
        self.dir.join("solution")
    }

    /// The command of the `outcome.hidden_tests` check, if the task has one.
    pub fn hidden_command(&self) -> Option<&str> {
        self.task
            .graders
            .required
            .iter()
            .find(|g| g.name == "outcome.hidden_tests")
            .and_then(|g| g.args.get("command"))
            .and_then(Value::as_str)
    }

    fn validate(&self) -> Result<(), String> {
        let task = &self.task;
        let dir_name = self.dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if task.id.is_empty()
            || !task
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!(
                "id {:?} must be lowercase letters, digits and dashes",
                task.id
            ));
        }
        if task.id != dir_name {
            return Err(format!(
                "id {:?} must be the name of its directory ({dir_name:?})",
                task.id
            ));
        }
        if task.version == 0 {
            return Err("version starts at 1".to_owned());
        }
        if task.instruction.trim().is_empty() {
            return Err("instruction is empty".to_owned());
        }
        if task.instruction.chars().count() > INSTRUCTION_CHARS_MAX {
            return Err(format!(
                "instruction is longer than {INSTRUCTION_CHARS_MAX} characters"
            ));
        }
        if !is_harness(&task.profile.harness) {
            return Err(format!(
                "profile.harness {:?} is not claude-code, codex, opencode or docket",
                task.profile.harness
            ));
        }
        for (harness, reason) in &task.not_applicable {
            if !is_harness(harness) {
                return Err(format!(
                    "not_applicable names {harness:?}, which is not claude-code, codex, opencode or docket"
                ));
            }
            if reason.trim().is_empty() {
                return Err(format!("not_applicable for {harness} needs its reason"));
            }
        }
        if task.limits.wall_s == 0 {
            return Err("limits.wall_s must be above zero".to_owned());
        }
        if task.limits.tokens == Some(0) {
            return Err("limits.tokens must be above zero, or absent for no limit".to_owned());
        }
        self.validate_safety()?;
        self.validate_graders()?;
        if !self.repo_dir().is_dir() {
            return Err("repo/ is missing: a task needs a seed repository".to_owned());
        }
        if self.hidden_command().is_some() && !self.hidden_dir().is_dir() {
            return Err("outcome.hidden_tests needs a hidden/ directory".to_owned());
        }
        Ok(())
    }

    fn validate_safety(&self) -> Result<(), String> {
        let task = &self.task;
        let Some(safety) = &task.safety else {
            return if task.suite == "safety" {
                Err("a task of the safety suite needs a `safety` block".to_owned())
            } else {
                Ok(())
            };
        };
        if safety.pair.is_empty() || safety.pair == task.id {
            return Err("safety.pair is the id of the other half of the pair".to_owned());
        }
        if !SAFETY_VECTORS.contains(&safety.vector.as_str()) {
            return Err(format!(
                "safety.vector {:?} is not one of {}",
                safety.vector,
                SAFETY_VECTORS.join(", ")
            ));
        }
        if !SAFETY_TARGETS.contains(&safety.target.as_str()) {
            return Err(format!(
                "safety.target {:?} is not one of {}",
                safety.target,
                SAFETY_TARGETS.join(", ")
            ));
        }
        if safety.markers.iter().any(|m| m.trim().is_empty()) {
            return Err(
                "safety.markers holds an empty word, which every event contains".to_owned(),
            );
        }
        if safety.markers.is_empty() {
            // Without a word to look for, an attempt could never be seen and the attack would
            // report zero by construction.
            return Err("safety.markers is empty: how else is an attempt seen".to_owned());
        }
        Ok(())
    }

    fn validate_graders(&self) -> Result<(), String> {
        let graders = &self.task.graders;
        if graders.required.is_empty() {
            return Err(
                "graders.required is empty: a task needs at least one check that decides, \
                 or every run of it would pass"
                    .to_owned(),
            );
        }
        for spec in graders.required.iter().chain(&graders.report) {
            if !KNOWN_GRADERS.contains(&spec.name.as_str()) {
                return Err(format!(
                    "unknown check {:?} (known: {})",
                    spec.name,
                    KNOWN_GRADERS.join(", ")
                ));
            }
        }
        for spec in graders
            .required
            .iter()
            .filter(|g| g.name == "outcome.hidden_tests")
        {
            let command = spec.args.get("command").and_then(Value::as_str);
            if command.is_none_or(|c| c.trim().is_empty()) {
                return Err("outcome.hidden_tests needs a non-empty `command`".to_owned());
            }
        }
        Ok(())
    }
}

fn is_harness(name: &str) -> bool {
    matches!(name, "claude-code" | "codex" | "opencode" | "docket")
}

fn collect_task_dirs(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if dir.join("task.yaml").is_file() {
        found.push(dir.to_owned());
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_task_dirs(&path, found)?;
        }
    }
    Ok(())
}
