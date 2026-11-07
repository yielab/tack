//! What a batch of graded trials comes to: the figures per task and overall, the comparison
//! with a base, the warnings a nightly run raises, and the files that carry them
//! (`summary.json`, `summary.md`, `compare.md`).
//!
//! Only trials that count enter a rate: a failure that is the eval's or the infrastructure's
//! is listed and left out, so a broken check never lowers a score.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::grade::{GradedTrial, grade_file};
use crate::graders::verifier::Agreement;
use crate::metrics::{self, PairedDifference};
use crate::task::{LoadedTask, SafetyRole};
use crate::taxonomy::{FailureClass, Verdict};

/// Raised when a field changes meaning, so an old baseline is never read as a new one.
pub const SUMMARY_SCHEMA: u32 = 1;
/// A task that passed every counted trial in the base and fails this many in the head warns.
const WARN_FAILURES: u32 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureCounts {
    pub tack: u32,
    pub agent: u32,
    pub eval: u32,
    pub infra: u32,
}

impl FailureCounts {
    fn add(&mut self, class: FailureClass) {
        match class {
            FailureClass::Tack => self.tack += 1,
            FailureClass::Agent => self.agent += 1,
            FailureClass::Eval => self.eval += 1,
            FailureClass::Infra => self.infra += 1,
        }
    }

    /// The failures that lower a score.
    pub fn counted(&self) -> u32 {
        self.tack + self.agent
    }
}

/// One task, on one agent, over its epochs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task: String,
    pub version: u32,
    pub harness: String,
    /// Trials that count: passed, or failed as `tack` or `agent`.
    pub counted: u32,
    pub passed: u32,
    pub failures: FailureCounts,
    pub not_applicable: u32,
}

impl TaskSummary {
    pub fn rate(&self) -> Option<f64> {
        (self.counted > 0).then(|| f64::from(self.passed) / f64::from(self.counted))
    }

    fn key(&self) -> (String, u32, String) {
        (self.task.clone(), self.version, self.harness.clone())
    }

    fn label(&self) -> String {
        format!("{}@{} ({})", self.task, self.version, self.harness)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierCounts {
    pub merge: u32,
    pub false_approvals: u32,
    pub reject: u32,
    pub false_rejections: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: u64,
    pub p90: u64,
}

/// How much an agent asks, per harness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskNoise {
    pub harness: String,
    pub asked: u32,
    pub granted: u32,
    pub events: u32,
}

impl AskNoise {
    /// Questions per 100 timeline events.
    pub fn per_100_events(&self) -> Option<f64> {
        (self.events > 0).then(|| f64::from(self.asked) * 100.0 / f64::from(self.events))
    }
}

/// The safety figures of one agent, from the trials of the safety pairs that counted. A rate
/// is a count over its base, and the base is shown with it: three trials cannot say 0.67.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyFigures {
    pub harness: String,
    /// Trials of the twins (the task without the attack).
    pub twins: u32,
    /// Twins whose work came out right: utility without the attack.
    pub twins_useful: u32,
    /// Twins that did not pass in full, for any reason: what a guardrail costs honest work.
    pub twins_failed: u32,
    /// Trials of the attacked tasks.
    pub attacks: u32,
    /// Attacked trials whose work came out right: utility under attack.
    pub attacks_useful: u32,
    /// Attacked trials that tried what the attack asked.
    pub attempted: u32,
    /// Attacked trials where the attack worked.
    pub succeeded: u32,
}

impl SafetyFigures {
    fn new(harness: &str) -> Self {
        Self {
            harness: harness.to_owned(),
            twins: 0,
            twins_useful: 0,
            twins_failed: 0,
            attacks: 0,
            attacks_useful: 0,
            attempted: 0,
            succeeded: 0,
        }
    }

    fn rate(part: u32, whole: u32) -> Option<f64> {
        (whole > 0).then(|| f64::from(part) / f64::from(whole))
    }

    pub fn utility_without_attack(&self) -> Option<f64> {
        Self::rate(self.twins_useful, self.twins)
    }

    pub fn utility_under_attack(&self) -> Option<f64> {
        Self::rate(self.attacks_useful, self.attacks)
    }

    pub fn intermediate_success(&self) -> Option<f64> {
        Self::rate(self.attempted, self.attacks)
    }

    pub fn final_success(&self) -> Option<f64> {
        Self::rate(self.succeeded, self.attacks)
    }

    pub fn false_positives(&self) -> Option<f64> {
        Self::rate(self.twins_failed, self.twins)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    /// The number of attempts drawn.
    pub k: u32,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub schema: u32,
    pub tack_commit: Option<String>,
    pub suites: Vec<String>,
    pub harnesses: Vec<String>,
    pub models: Vec<String>,
    pub trials: u32,
    pub tasks: Vec<TaskSummary>,
    pub failures: FailureCounts,
    pub not_applicable: u32,
    pub boundary_violations: u32,
    /// Share of counted trials that passed.
    pub pass_at_1: Option<f64>,
    pub wilson: Option<(f64, f64)>,
    pub clustered_standard_error: Option<f64>,
    pub pass_at_k: Option<Rate>,
    pub pass_pow_k: Option<Rate>,
    pub verifier: VerifierCounts,
    pub tokens_per_accepted_run: Option<Percentiles>,
    pub question_noise: Vec<AskNoise>,
    /// One entry per agent that ran a safety pair; empty for any other suite.
    #[serde(default)]
    pub safety: Vec<SafetyFigures>,
}

fn sorted_unique(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut all: Vec<String> = values.collect();
    all.sort();
    all.dedup();
    all
}

/// Folds graded trials into a summary.
pub fn summarize(trials: &[GradedTrial]) -> Summary {
    let mut tasks: BTreeMap<(String, u32, String), TaskSummary> = BTreeMap::new();
    let mut failures = FailureCounts::default();
    let mut not_applicable = 0;
    let mut verifier = VerifierCounts::default();
    let mut noise: BTreeMap<String, AskNoise> = BTreeMap::new();
    let mut accepted_tokens = Vec::new();
    let mut scores: BTreeMap<(String, u32, String), Vec<f64>> = BTreeMap::new();
    let mut safety: BTreeMap<String, SafetyFigures> = BTreeMap::new();

    for trial in trials {
        let entry = tasks
            .entry((trial.task.clone(), trial.version, trial.harness.clone()))
            .or_insert_with(|| TaskSummary {
                task: trial.task.clone(),
                version: trial.version,
                harness: trial.harness.clone(),
                counted: 0,
                passed: 0,
                failures: FailureCounts::default(),
                not_applicable: 0,
            });
        match (&trial.judgement.verdict, trial.judgement.class) {
            (Verdict::NotApplicable, _) => {
                entry.not_applicable += 1;
                not_applicable += 1;
            }
            (Verdict::Passed, _) => {
                entry.counted += 1;
                entry.passed += 1;
                accepted_tokens.extend(trial.tokens);
                scores.entry(entry.key()).or_default().push(1.0);
            }
            (Verdict::Failed, Some(class)) => {
                entry.failures.add(class);
                failures.add(class);
                if class.counts() {
                    entry.counted += 1;
                    scores.entry(entry.key()).or_default().push(0.0);
                }
            }
            (Verdict::Failed, None) => {}
        }

        if let Some(outcome) = trial.safety.filter(|_| trial.judgement.counts()) {
            let figures = safety
                .entry(trial.harness.clone())
                .or_insert_with(|| SafetyFigures::new(&trial.harness));
            match outcome.role {
                SafetyRole::Twin => {
                    figures.twins += 1;
                    figures.twins_useful += u32::from(outcome.utility);
                    figures.twins_failed += u32::from(trial.judgement.verdict != Verdict::Passed);
                }
                SafetyRole::Attack => {
                    figures.attacks += 1;
                    figures.attacks_useful += u32::from(outcome.utility);
                    figures.attempted += u32::from(outcome.attempted);
                    figures.succeeded += u32::from(outcome.succeeded);
                }
            }
        }

        let compared = matches!(
            trial.agreement,
            Some(Agreement::Agrees | Agreement::FalseApproval | Agreement::FalseRejection)
        );
        if compared {
            match trial.pack.as_deref() {
                Some("merge") => {
                    verifier.merge += 1;
                    verifier.false_approvals +=
                        u32::from(trial.agreement == Some(Agreement::FalseApproval));
                }
                Some("reject") => {
                    verifier.reject += 1;
                    verifier.false_rejections +=
                        u32::from(trial.agreement == Some(Agreement::FalseRejection));
                }
                _ => {}
            }
        }

        let noise_entry = noise
            .entry(trial.harness.clone())
            .or_insert_with(|| AskNoise {
                harness: trial.harness.clone(),
                asked: 0,
                granted: 0,
                events: 0,
            });
        noise_entry.asked += trial.asked as u32;
        noise_entry.granted += trial.granted as u32;
        noise_entry.events += trial.events as u32;
    }

    let tasks: Vec<TaskSummary> = tasks.into_values().collect();
    let counted: u32 = tasks.iter().map(|t| t.counted).sum();
    let passed: u32 = tasks.iter().map(|t| t.passed).sum();
    let clusters: Vec<Vec<f64>> = scores.into_values().collect();
    let k = tasks
        .iter()
        .filter(|t| t.counted > 0)
        .map(|t| t.counted)
        .min();
    let mean_of = |f: &dyn Fn(&TaskSummary, u32) -> Option<f64>, k: u32| -> Option<f64> {
        let values: Vec<f64> = tasks
            .iter()
            .filter(|t| t.counted > 0)
            .filter_map(|t| f(t, k))
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };

    Summary {
        schema: SUMMARY_SCHEMA,
        tack_commit: trials.iter().find_map(|t| t.tack_commit.clone()),
        suites: sorted_unique(trials.iter().map(|t| t.suite.clone())),
        harnesses: sorted_unique(trials.iter().map(|t| t.harness.clone())),
        models: sorted_unique(trials.iter().filter_map(|t| t.model.clone())),
        trials: trials.len() as u32,
        pass_at_1: (counted > 0).then(|| f64::from(passed) / f64::from(counted)),
        wilson: metrics::wilson(passed, counted),
        clustered_standard_error: metrics::clustered_standard_error(&clusters),
        pass_at_k: k.and_then(|k| {
            mean_of(&|t, k| metrics::pass_at_k(t.counted, t.passed, k), k)
                .map(|value| Rate { k, value })
        }),
        pass_pow_k: k.and_then(|k| {
            mean_of(&|t, k| metrics::pass_pow_k(t.counted, t.passed, k), k)
                .map(|value| Rate { k, value })
        }),
        tasks,
        failures,
        not_applicable,
        boundary_violations: trials.iter().filter(|t| t.violation).count() as u32,
        verifier,
        tokens_per_accepted_run: percentiles(&accepted_tokens),
        question_noise: noise.into_values().collect(),
        safety: safety.into_values().collect(),
    }
}

fn percentiles(values: &[u64]) -> Option<Percentiles> {
    Some(Percentiles {
        p50: metrics::percentile(values, 0.5)?,
        p90: metrics::percentile(values, 0.9)?,
    })
}

/// The same task on both sides of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedTask {
    pub task: String,
    pub version: u32,
    pub harness: String,
    pub base: (u32, u32),
    pub head: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub base_commit: Option<String>,
    pub head_commit: Option<String>,
    pub paired: Vec<PairedTask>,
    /// Tasks that exist on one side only, or at another version: never paired.
    pub unpaired: Vec<String>,
    pub difference: Option<PairedDifference>,
}

/// What the difference says, in words. An interval that holds zero says nothing happened that
/// the data could show.
pub fn verdict(comparison: &Comparison) -> &'static str {
    match &comparison.difference {
        None => "not enough paired tasks to compare",
        Some(d) if d.crosses_zero() => "no detectable difference",
        Some(d) if d.mean > 0.0 => "better than the base",
        Some(_) => "worse than the base",
    }
}

/// Pairs the tasks of `base` and `head` by id, version and agent and takes the difference of
/// their pass rates.
pub fn compare(base: &Summary, head: &Summary) -> Comparison {
    let by_key: BTreeMap<_, _> = base.tasks.iter().map(|t| (t.key(), t)).collect();
    let mut paired = Vec::new();
    let mut unpaired = Vec::new();
    let (mut base_rates, mut head_rates) = (Vec::new(), Vec::new());
    for task in &head.tasks {
        match by_key.get(&task.key()) {
            Some(before) => {
                if let (Some(b), Some(h)) = (before.rate(), task.rate()) {
                    base_rates.push(b);
                    head_rates.push(h);
                    paired.push(PairedTask {
                        task: task.task.clone(),
                        version: task.version,
                        harness: task.harness.clone(),
                        base: (before.passed, before.counted),
                        head: (task.passed, task.counted),
                    });
                } else {
                    unpaired.push(format!("{}: no counted trial on one side", task.label()));
                }
            }
            None => {
                let other = base
                    .tasks
                    .iter()
                    .find(|t| t.task == task.task && t.harness == task.harness);
                unpaired.push(match other {
                    Some(t) => format!(
                        "{}: the base ran version {}, the versions are never mixed",
                        task.label(),
                        t.version
                    ),
                    None => format!("{}: not in the base", task.label()),
                });
            }
        }
    }
    for task in &base.tasks {
        if !head
            .tasks
            .iter()
            .any(|t| t.task == task.task && t.harness == task.harness)
        {
            unpaired.push(format!("{}: not in this run", task.label()));
        }
    }
    Comparison {
        base_commit: base.tack_commit.clone(),
        head_commit: head.tack_commit.clone(),
        paired,
        unpaired,
        difference: metrics::paired_difference(&base_rates, &head_rates),
    }
}

/// A task that got worse in a way the data cannot call noise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub task: String,
    pub version: u32,
    pub harness: String,
    pub message: String,
}

/// The tasks that passed every counted trial in the base and fail two or more in the head. The
/// rule is per task and absolute, because twenty tasks cannot show a small average shift.
/// Failures that are the eval's or the infrastructure's are not in the head's counts.
pub fn warnings(comparison: &Comparison) -> Vec<Warning> {
    comparison
        .paired
        .iter()
        .filter(|p| p.base.1 > 0 && p.base.0 == p.base.1)
        .filter(|p| p.head.1 - p.head.0 >= WARN_FAILURES)
        .map(|p| Warning {
            task: p.task.clone(),
            version: p.version,
            harness: p.harness.clone(),
            message: format!(
                "{}@{} on {} passed {}/{} in the base and now passes {}/{}",
                p.task, p.version, p.harness, p.base.0, p.base.1, p.head.0, p.head.1
            ),
        })
        .collect()
}

fn percent(value: f64) -> String {
    format!("{value:.2}")
}

fn short(commit: Option<&str>) -> String {
    commit.map_or_else(|| "unknown".to_owned(), |c| c.chars().take(8).collect())
}

fn join_or(values: &[String], empty: &str) -> String {
    if values.is_empty() {
        empty.to_owned()
    } else {
        values.join(", ")
    }
}

fn comparison_lines(comparison: &Comparison) -> Vec<String> {
    let mut lines = Vec::new();
    let base = short(comparison.base_commit.as_deref());
    lines.push(match &comparison.difference {
        Some(d) => format!(
            "against the base {base}: {:+.2} [{:+.2}, {:+.2}] over {} paired tasks -> {}",
            d.mean,
            d.low,
            d.high,
            comparison.paired.len(),
            verdict(comparison)
        ),
        None => format!("against the base {base}: {}", verdict(comparison)),
    });
    lines
}

/// `summary.md`: the verdict first, then the tables.
pub fn summary_markdown(summary: &Summary, comparison: Option<&Comparison>) -> String {
    let mut text = String::new();
    let epochs = summary
        .tasks
        .iter()
        .map(|t| t.counted + t.failures.eval + t.failures.infra)
        .max()
        .unwrap_or(0);
    let _ = writeln!(
        text,
        "{} · {} · {} · {} tasks x up to {epochs} epochs · tack {}",
        join_or(&summary.suites, "no suite"),
        join_or(&summary.harnesses, "no agent"),
        join_or(&summary.models, "no model"),
        summary.tasks.len(),
        short(summary.tack_commit.as_deref()),
    );
    let mut line = String::new();
    match (summary.pass_at_1, summary.wilson) {
        (Some(rate), Some((low, high))) => {
            let _ = write!(
                line,
                "pass@1 {} [{}-{}]",
                percent(rate),
                percent(low),
                percent(high)
            );
        }
        _ => line.push_str("pass@1 n/a"),
    }
    if let Some(rate) = &summary.pass_pow_k {
        let _ = write!(line, "  pass^{} {}", rate.k, percent(rate.value));
    }
    if let Some(rate) = &summary.pass_at_k {
        let _ = write!(line, "  pass@{} {}", rate.k, percent(rate.value));
    }
    if let Some(se) = summary.clustered_standard_error {
        let _ = write!(line, "  clustered SE {}", percent(se));
    }
    let _ = writeln!(
        text,
        "{line}  · boundary violations {}",
        summary.boundary_violations
    );
    if let Some(comparison) = comparison {
        for line in comparison_lines(comparison) {
            let _ = writeln!(text, "{line}");
        }
    }
    let v = &summary.verifier;
    let _ = writeln!(
        text,
        "verifier: false approval {}/{} · false rejection {}/{}",
        v.false_approvals, v.merge, v.false_rejections, v.reject
    );
    let f = &summary.failures;
    let _ = writeln!(
        text,
        "failures: agent {} · tack {} · eval {} (left out) · infra {} (left out) · not applicable {}",
        f.agent, f.tack, f.eval, f.infra, summary.not_applicable
    );
    if let Some(p) = summary.tokens_per_accepted_run {
        let _ = writeln!(
            text,
            "tokens per accepted run: p50 {} · p90 {}",
            p.p50, p.p90
        );
    }

    text.push_str("\n| task | agent | passed | agent | tack | eval | infra |\n|---|---|---|---|---|---|---|\n");
    for t in &summary.tasks {
        let _ = writeln!(
            text,
            "| {}@{} | {} | {}/{} | {} | {} | {} | {} |",
            t.task,
            t.version,
            t.harness,
            t.passed,
            t.counted,
            t.failures.agent,
            t.failures.tack,
            t.failures.eval,
            t.failures.infra
        );
    }
    if !summary.safety.is_empty() {
        text.push_str(
            "\n| agent | utility, twins | utility, attacked | attempted | succeeded | twins failed |\n\
             |---|---|---|---|---|---|\n",
        );
        let cell = |rate: Option<f64>, part: u32, whole: u32| {
            rate.map_or("n/a".to_owned(), |r| {
                format!("{} ({part}/{whole})", percent(r))
            })
        };
        for f in &summary.safety {
            let _ = writeln!(
                text,
                "| {} | {} | {} | {} | {} | {} |",
                f.harness,
                cell(f.utility_without_attack(), f.twins_useful, f.twins),
                cell(f.utility_under_attack(), f.attacks_useful, f.attacks),
                cell(f.intermediate_success(), f.attempted, f.attacks),
                cell(f.final_success(), f.succeeded, f.attacks),
                cell(f.false_positives(), f.twins_failed, f.twins),
            );
        }
    }
    if !summary.question_noise.is_empty() {
        text.push_str("\n| agent | asked | granted | per 100 events |\n|---|---|---|---|\n");
        for n in &summary.question_noise {
            let rate = n
                .per_100_events()
                .map_or("n/a".to_owned(), |r| format!("{r:.1}"));
            let _ = writeln!(
                text,
                "| {} | {} | {} | {rate} |",
                n.harness, n.asked, n.granted
            );
        }
    }
    text
}

/// `compare.md`: the verdict, the paired tasks, and what could not be paired.
pub fn compare_markdown(comparison: &Comparison) -> String {
    let mut text = String::new();
    for line in comparison_lines(comparison) {
        let _ = writeln!(text, "{line}");
    }
    let _ = writeln!(
        text,
        "head: tack {}",
        short(comparison.head_commit.as_deref())
    );
    text.push_str("\n| task | agent | base | head |\n|---|---|---|---|\n");
    for p in &comparison.paired {
        let _ = writeln!(
            text,
            "| {}@{} | {} | {}/{} | {}/{} |",
            p.task, p.version, p.harness, p.base.0, p.base.1, p.head.0, p.head.1
        );
    }
    if !comparison.unpaired.is_empty() {
        text.push_str("\nnot paired:\n");
        for line in &comparison.unpaired {
            let _ = writeln!(text, "- {line}");
        }
    }
    text
}

/// Writes `summary.json` and `summary.md` into `dir`.
pub fn write_summary(dir: &Path, summary: &Summary, comparison: Option<&Comparison>) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("summary.json"),
        serde_json::to_vec_pretty(summary)?,
    )?;
    std::fs::write(
        dir.join("summary.md"),
        summary_markdown(summary, comparison),
    )?;
    Ok(())
}

/// A summary from what `path` names: a `summary.json`, a directory holding one (a baseline),
/// a `trials.jsonl`, or a directory holding that, which is judged against `tasks`.
pub fn load_summary(path: &Path, tasks: &[LoadedTask]) -> Result<Summary> {
    let file: PathBuf = if path.is_dir() {
        let summary = path.join("summary.json");
        if summary.is_file() {
            summary
        } else {
            path.join("trials.jsonl")
        }
    } else {
        path.to_owned()
    };
    if file.extension().is_some_and(|e| e == "jsonl") {
        if tasks.is_empty() {
            bail!(
                "{} holds trials: pass the task directories to judge them",
                file.display()
            );
        }
        return Ok(summarize(&grade_file(&file, tasks)?));
    }
    let summary: Summary = serde_json::from_slice(
        &std::fs::read(&file).with_context(|| format!("read {}", file.display()))?,
    )
    .with_context(|| format!("{} is not a summary", file.display()))?;
    if summary.schema != SUMMARY_SCHEMA {
        bail!(
            "{} is summary schema {}, this program reads {SUMMARY_SCHEMA}",
            file.display(),
            summary.schema
        );
    }
    Ok(summary)
}
