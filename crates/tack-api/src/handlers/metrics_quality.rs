//! The quality figures of the factory metrics: what the agents' work came to in the window, as
//! rates over rows with their counts beside them, and the same rates cut by profile, agent,
//! model, team and prompt-template fingerprint. The arithmetic is here and pure; the rows come
//! from `tack_db::queries::metrics`. A rate with nothing to measure is `null` with its reason.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use tack_core::run_outcome::{never_started, work_outcome};
use tack_db::queries::metrics::{QualityAttempt, QuestionRow};
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::MergeDecision;
use utoipa::ToSchema;

use super::metrics::{DENOMINATOR_ZERO, Ratio, ratio};

/// The number of runs of a recurring task that pass^k asks all to have succeeded.
const PASS_K: i64 = 3;

const NOT_RECORDED: &str = "Not recorded";
const NO_TEAM: &str = "No team";

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkQuality {
    /// Runs that succeeded ÷ runs that ended after starting (failed or succeeded).
    pub success_rate: Ratio,
    /// Tasks whose first attempt a person accepted ÷ tasks whose first attempt a person reviewed.
    pub first_pass_acceptance: Ratio,
    /// Attempts after a task's first ÷ tasks with an attempt. A value above 1 is more than one extra round per task.
    pub rework_per_item: Ratio,
    /// Pull requests reverted ÷ pull requests merged or reverted.
    pub revert_after_merge: Ratio,
    /// One entry per verifier, in order: `prill`, then `reviewer`.
    pub verifier_agreement: Vec<VerifierAgreement>,
    pub pass_pow_k: PassPowK,
    /// Questions put to a person, by agent and kind.
    pub questions: Vec<QuestionNoise>,
    /// Runs that did not succeed, by the reason they gave.
    pub stops: Vec<StopCount>,
    /// The success, first-pass and revert rates by each way of cutting the runs.
    pub cuts: Vec<QualityCut>,
}

/// How often the verifier's word matched what became of the work. A task is *bad* when a person
/// rejected it or its pull request was reverted, and *good* when a person accepted it and it was
/// not reverted; any other task has no label and is not counted.
#[derive(Debug, Serialize, ToSchema)]
pub struct VerifierAgreement {
    /// Who said it: `prill` (the pack Tack reads) or `reviewer` (the Reviewer profile's verdict line).
    pub verifier: String,
    /// Said merge on good work, or reject on bad work.
    pub agreed: i64,
    /// Said merge on bad work.
    pub false_approvals: i64,
    /// Said reject on good work.
    pub false_rejections: i64,
    /// Labelled work the verifier handed to a person (`review`): neither right nor wrong.
    pub handed_to_a_person: i64,
    /// Labelled work this verifier said nothing about: no readable pack, or no Reviewer run.
    pub without_a_word: i64,
    /// False approvals ÷ bad work the verifier judged.
    pub false_approval_rate: Ratio,
    /// False rejections ÷ good work the verifier judged.
    pub false_rejection_rate: Ratio,
}

/// Of the recurring tasks with at least `k` runs, the average chance that `k` of their runs all succeed.
#[derive(Debug, Serialize, ToSchema)]
pub struct PassPowK {
    pub k: i64,
    /// Recurring tasks with at least `k` runs in the window.
    pub tasks_measured: i64,
    /// Recurring tasks with fewer runs, left out.
    pub tasks_too_short: i64,
    pub value: Option<f64>,
    /// `denominator_zero` when no recurring task has `k` runs.
    pub null_reason: Option<String>,
}

/// What one kind of agent asked a person, and how it went.
#[derive(Debug, Serialize, ToSchema)]
pub struct QuestionNoise {
    /// The agent; `Not recorded` when the run never named one.
    pub harness: String,
    /// What was asked (the decision's kind).
    pub class: String,
    pub asked: i64,
    /// Steps the agent reported in all, whatever it asked: what the rate is over.
    pub actions: i64,
    /// Questions of this kind per hundred steps; absent without a step.
    pub per_100_actions: Option<f64>,
    /// Answered with a go-ahead ÷ answered by a person.
    pub granted: Ratio,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StopCount {
    /// The code the run ended with, `ceiling` for a team's ceiling that sent no code, `cancelled`, or `unspecified`.
    pub reason: String,
    pub count: i64,
    /// The run was turned away before the agent did anything.
    pub not_started: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QualityCut {
    /// `profile`, `harness`, `model`, `team` or `template`.
    pub by: String,
    pub rows: Vec<CutRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CutRow {
    pub name: String,
    pub attempts: i64,
    pub success_rate: Ratio,
    pub first_pass_acceptance: Ratio,
    pub revert_after_merge: Ratio,
}

/// Whether the attempt counts as a success (`Some(true)`), a failure of the work (`Some(false)`)
/// or neither: still going, cancelled, or turned away before it started.
fn outcome(attempt: &QualityAttempt) -> Option<bool> {
    let state: ExecutionState = attempt.state.parse().ok()?;
    work_outcome(state, reason_of(attempt).as_ref())
}

fn reason_of(attempt: &QualityAttempt) -> Option<Value> {
    attempt
        .terminal_reason
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
}

fn success_rate(attempts: &[&QualityAttempt]) -> Ratio {
    let outcomes: Vec<bool> = attempts.iter().filter_map(|a| outcome(a)).collect();
    ratio(
        outcomes.iter().filter(|ok| **ok).count() as i64,
        outcomes.len() as i64,
        DENOMINATOR_ZERO,
    )
}

fn first_pass(attempts: &[&QualityAttempt]) -> Ratio {
    let reviewed: Vec<&&QualityAttempt> = attempts
        .iter()
        .filter(|a| a.first_of_item && a.verdict.is_some())
        .collect();
    let accepted = reviewed
        .iter()
        .filter(|a| a.verdict.as_deref() == Some("accepted"))
        .count();
    ratio(accepted as i64, reviewed.len() as i64, DENOMINATOR_ZERO)
}

fn revert_rate(attempts: &[&QualityAttempt]) -> Ratio {
    let reverted = attempts
        .iter()
        .filter(|a| a.pull_request.as_deref() == Some("reverted"))
        .count();
    let merged = attempts
        .iter()
        .filter(|a| a.pull_request.as_deref() == Some("merged"))
        .count();
    ratio(
        reverted as i64,
        (reverted + merged) as i64,
        DENOMINATOR_ZERO,
    )
}

fn rework_per_item(attempts: &[&QualityAttempt]) -> Ratio {
    let items: HashSet<&str> = attempts.iter().map(|a| a.item_id.as_str()).collect();
    let later = attempts.iter().filter(|a| !a.first_of_item).count();
    ratio(later as i64, items.len() as i64, DENOMINATOR_ZERO)
}

/// What became of the work, as far as a person and the repository say. `None` when neither has.
fn work_is_good(attempt: &QualityAttempt) -> Option<bool> {
    if attempt.pull_request.as_deref() == Some("reverted")
        || attempt.verdict.as_deref() == Some("rejected")
    {
        return Some(false);
    }
    (attempt.verdict.as_deref() == Some("accepted")).then_some(true)
}

/// The hand table: the verifier's word against `work_is_good`.
///
/// | it says   | good work | bad work |
/// |-----------|-----------|----------|
/// | merge     | agreed    | false approval |
/// | reject    | false rejection | agreed |
/// | review    | handed to a person | handed to a person |
fn verifier_agreement(
    verifier: &str,
    attempts: &[&QualityAttempt],
    words: &HashMap<String, MergeDecision>,
) -> VerifierAgreement {
    let (mut agreed, mut false_approvals, mut false_rejections) = (0, 0, 0);
    let (mut handed, mut without_a_word) = (0, 0);
    let (mut good_judged, mut bad_judged) = (0, 0);
    for attempt in attempts {
        let Some(good) = work_is_good(attempt) else {
            continue;
        };
        let Some(word) = words.get(&attempt.attempt_id) else {
            without_a_word += 1;
            continue;
        };
        match (word, good) {
            (MergeDecision::Review, _) => handed += 1,
            (MergeDecision::Merge, true) => {
                agreed += 1;
                good_judged += 1;
            }
            (MergeDecision::Merge, false) => {
                false_approvals += 1;
                bad_judged += 1;
            }
            (MergeDecision::Reject, false) => {
                agreed += 1;
                bad_judged += 1;
            }
            (MergeDecision::Reject, true) => {
                false_rejections += 1;
                good_judged += 1;
            }
        }
    }
    VerifierAgreement {
        verifier: verifier.into(),
        agreed,
        false_approvals,
        false_rejections,
        handed_to_a_person: handed,
        without_a_word,
        false_approval_rate: ratio(false_approvals, bad_judged, DENOMINATOR_ZERO),
        false_rejection_rate: ratio(false_rejections, good_judged, DENOMINATOR_ZERO),
    }
}

/// C(c, k) ÷ C(n, k): the chance that `k` runs drawn from `n`, `c` of which succeeded, all did.
fn pass_pow(n: i64, c: i64, k: i64) -> f64 {
    if c < k {
        return 0.0;
    }
    (0..k).map(|i| (c - i) as f64 / (n - i) as f64).product()
}

fn pass_pow_k(attempts: &[&QualityAttempt]) -> PassPowK {
    // A run of a recurring task is one of its occurrences, judged by that occurrence's newest
    // attempt in the window.
    let mut runs: BTreeMap<&str, BTreeMap<&str, bool>> = BTreeMap::new();
    for attempt in attempts {
        let (Some(root), Some(ok)) = (attempt.recurring_root.as_deref(), outcome(attempt)) else {
            continue;
        };
        runs.entry(root)
            .or_default()
            .insert(attempt.item_id.as_str(), ok);
    }
    let (mut measured, mut too_short, mut sum) = (0, 0, 0.0);
    for occurrences in runs.values() {
        let n = occurrences.len() as i64;
        if n < PASS_K {
            too_short += 1;
            continue;
        }
        measured += 1;
        sum += pass_pow(
            n,
            occurrences.values().filter(|ok| **ok).count() as i64,
            PASS_K,
        );
    }
    PassPowK {
        k: PASS_K,
        tasks_measured: measured,
        tasks_too_short: too_short,
        value: (measured > 0).then(|| sum / measured as f64),
        null_reason: (measured == 0).then(|| DENOMINATOR_ZERO.into()),
    }
}

fn stops(attempts: &[&QualityAttempt]) -> Vec<StopCount> {
    let mut counts: BTreeMap<(String, bool), i64> = BTreeMap::new();
    for attempt in attempts {
        let Ok(state) = attempt.state.parse::<ExecutionState>() else {
            continue;
        };
        let key = match state {
            ExecutionState::Cancelled => ("cancelled".to_owned(), false),
            ExecutionState::Failed => {
                let reason = reason_of(attempt);
                let code = reason
                    .as_ref()
                    .and_then(|r| r.get("code"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let never = never_started(state, reason.as_ref());
                (code.unwrap_or_else(|| default_reason(never)), never)
            }
            _ => continue,
        };
        *counts.entry(key).or_default() += 1;
    }
    let mut rows: Vec<StopCount> = counts
        .into_iter()
        .map(|((reason, not_started), count)| StopCount {
            reason,
            count,
            not_started,
        })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.reason.cmp(&b.reason)));
    rows
}

/// A refused run that sent no code is the team's ceiling (docket words it in the message).
fn default_reason(never_started: bool) -> String {
    if never_started {
        "ceiling"
    } else {
        "unspecified"
    }
    .to_owned()
}

fn questions(rows: &[QuestionRow], actions: &[(Option<String>, i64)]) -> Vec<QuestionNoise> {
    let steps: HashMap<&Option<String>, i64> = actions.iter().map(|(h, n)| (h, *n)).collect();
    rows.iter()
        .map(|row| {
            let total = steps.get(&row.harness).copied().unwrap_or(0);
            QuestionNoise {
                harness: row.harness.clone().unwrap_or_else(|| NOT_RECORDED.into()),
                class: row.class.clone(),
                asked: row.asked,
                actions: total,
                per_100_actions: (total > 0).then(|| row.asked as f64 * 100.0 / total as f64),
                granted: ratio(row.granted, row.answered, DENOMINATOR_ZERO),
            }
        })
        .collect()
}

fn cut(
    attempts: &[&QualityAttempt],
    by: &str,
    key: impl Fn(&QualityAttempt) -> String,
) -> QualityCut {
    let mut groups: BTreeMap<String, Vec<&QualityAttempt>> = BTreeMap::new();
    for attempt in attempts {
        groups.entry(key(attempt)).or_default().push(attempt);
    }
    let mut rows: Vec<CutRow> = groups
        .into_iter()
        .map(|(name, group)| CutRow {
            name,
            attempts: group.len() as i64,
            success_rate: success_rate(&group),
            first_pass_acceptance: first_pass(&group),
            revert_after_merge: revert_rate(&group),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.attempts
            .cmp(&a.attempts)
            .then_with(|| a.name.cmp(&b.name))
    });
    QualityCut {
        by: by.into(),
        rows,
    }
}

fn or_not_recorded(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| NOT_RECORDED.into())
}

/// What the Reviewer profile said about each attempt it reviewed.
fn reviewer_words(attempts: &[&QualityAttempt]) -> HashMap<String, MergeDecision> {
    attempts
        .iter()
        .filter_map(|attempt| {
            let word = match attempt.reviewer_decision.as_deref()? {
                "merge" => MergeDecision::Merge,
                "review" => MergeDecision::Review,
                "reject" => MergeDecision::Reject,
                _ => return None,
            };
            Some((attempt.attempt_id.clone(), word))
        })
        .collect()
}

/// Every quality figure of the window from the attempts' rows, prill's word on each attempt
/// that has a pack, and the questions asked.
pub(super) fn work_quality(
    attempts: &[QualityAttempt],
    packs: &HashMap<String, MergeDecision>,
    question_rows: &[QuestionRow],
    actions: &[(Option<String>, i64)],
) -> WorkQuality {
    let all: Vec<&QualityAttempt> = attempts.iter().collect();
    WorkQuality {
        success_rate: success_rate(&all),
        first_pass_acceptance: first_pass(&all),
        rework_per_item: rework_per_item(&all),
        revert_after_merge: revert_rate(&all),
        verifier_agreement: vec![
            verifier_agreement("prill", &all, packs),
            verifier_agreement("reviewer", &all, &reviewer_words(&all)),
        ],
        pass_pow_k: pass_pow_k(&all),
        questions: questions(question_rows, actions),
        stops: stops(&all),
        cuts: vec![
            cut(&all, "profile", |a| or_not_recorded(&a.profile)),
            cut(&all, "harness", |a| or_not_recorded(&a.harness)),
            cut(&all, "model", |a| or_not_recorded(&a.model)),
            cut(&all, "team", |a| {
                a.team.clone().unwrap_or_else(|| NO_TEAM.into())
            }),
            cut(&all, "template", |a| or_not_recorded(&a.template)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(id: &str) -> QualityAttempt {
        QualityAttempt {
            attempt_id: id.into(),
            request_id: id.into(),
            item_id: id.into(),
            state: "succeeded".into(),
            terminal_reason: None,
            profile: None,
            harness: None,
            model: None,
            team: None,
            template: None,
            first_of_item: true,
            recurring_root: None,
            verdict: None,
            pull_request: None,
            pack_reference: None,
            reviewer_decision: None,
        }
    }

    /// The four cases of the hand table, one attempt each, plus the ones that are not counted.
    #[test]
    fn the_verifier_agreement_matches_the_hand_table() {
        let mut good_merged = attempt("good-merge");
        good_merged.verdict = Some("accepted".into());
        let mut bad_merged = attempt("bad-merge");
        bad_merged.verdict = Some("accepted".into());
        bad_merged.pull_request = Some("reverted".into());
        let mut bad_rejected = attempt("bad-reject");
        bad_rejected.verdict = Some("rejected".into());
        let mut good_rejected = attempt("good-reject");
        good_rejected.verdict = Some("accepted".into());
        let mut handed = attempt("handed");
        handed.verdict = Some("accepted".into());
        let mut no_pack = attempt("no-pack");
        no_pack.verdict = Some("rejected".into());
        let unlabelled = attempt("unlabelled");
        let packs = HashMap::from([
            ("good-merge".to_owned(), MergeDecision::Merge),
            ("bad-merge".to_owned(), MergeDecision::Merge),
            ("bad-reject".to_owned(), MergeDecision::Reject),
            ("good-reject".to_owned(), MergeDecision::Reject),
            ("handed".to_owned(), MergeDecision::Review),
            ("unlabelled".to_owned(), MergeDecision::Merge),
        ]);
        let attempts = [
            good_merged,
            bad_merged,
            bad_rejected,
            good_rejected,
            handed,
            no_pack,
            unlabelled,
        ];
        let refs: Vec<&QualityAttempt> = attempts.iter().collect();
        let a = verifier_agreement("prill", &refs, &packs);
        assert_eq!(
            (
                a.agreed,
                a.false_approvals,
                a.false_rejections,
                a.handed_to_a_person,
                a.without_a_word
            ),
            (2, 1, 1, 1, 1)
        );
        // Bad work judged: bad-merge and bad-reject. Good work judged: good-merge and good-reject.
        assert_eq!(
            (
                a.false_approval_rate.numerator,
                a.false_approval_rate.denominator
            ),
            (1, 2)
        );
        assert_eq!(
            (
                a.false_rejection_rate.numerator,
                a.false_rejection_rate.denominator
            ),
            (1, 2)
        );
    }

    /// The Reviewer's word is read from the attempts and counted beside prill's, never mixed into it.
    #[test]
    fn each_verifier_is_counted_on_its_own_word() {
        let mut approved_bad = attempt("approved-bad");
        approved_bad.verdict = Some("rejected".into());
        approved_bad.reviewer_decision = Some("merge".into());
        let mut rejected_bad = attempt("rejected-bad");
        rejected_bad.verdict = Some("rejected".into());
        rejected_bad.reviewer_decision = Some("reject".into());
        let mut rejected_good = attempt("rejected-good");
        rejected_good.verdict = Some("accepted".into());
        rejected_good.reviewer_decision = Some("reject".into());
        let mut only_prill = attempt("only-prill");
        only_prill.verdict = Some("accepted".into());
        let attempts = [approved_bad, rejected_bad, rejected_good, only_prill];
        let packs = HashMap::from([("only-prill".to_owned(), MergeDecision::Merge)]);
        let figures = work_quality(&attempts, &packs, &[], &[]).verifier_agreement;
        let counts = |a: &VerifierAgreement| {
            (
                a.verifier.clone(),
                a.agreed,
                a.false_approvals,
                a.false_rejections,
                a.without_a_word,
            )
        };
        assert_eq!(
            figures.iter().map(counts).collect::<Vec<_>>(),
            [
                ("prill".to_owned(), 1, 0, 0, 3),
                ("reviewer".to_owned(), 1, 1, 1, 1),
            ]
        );
    }

    #[test]
    fn pass_pow_is_the_chance_that_all_k_drawn_runs_succeed() {
        assert!((pass_pow(5, 3, 3) - 0.1).abs() < 1e-12);
        assert!((pass_pow(3, 3, 3) - 1.0).abs() < 1e-12);
        assert_eq!(pass_pow(4, 2, 3), 0.0);
    }

    #[test]
    fn a_turned_away_run_is_not_a_failure_of_the_work() {
        let mut refused = attempt("refused");
        refused.state = "failed".into();
        refused.terminal_reason = Some(r#"{"code":"harness_rejected"}"#.into());
        let mut failed = attempt("failed");
        failed.state = "failed".into();
        failed.terminal_reason = Some(r#"{"code":"timeout"}"#.into());
        let ok = attempt("ok");
        let attempts = [refused, failed, ok];
        let refs: Vec<&QualityAttempt> = attempts.iter().collect();
        let rate = success_rate(&refs);
        assert_eq!((rate.numerator, rate.denominator), (1, 2));
        let reasons: Vec<(String, bool)> = stops(&refs)
            .into_iter()
            .map(|s| (s.reason, s.not_started))
            .collect();
        assert!(reasons.contains(&("harness_rejected".to_owned(), true)));
        assert!(reasons.contains(&("timeout".to_owned(), false)));
    }
}
