//! Did the verifier's pack agree with the hidden tests. The tests are the truth here: a
//! `merge` on work that fails them is a false approval, a `reject` on work that passes them a
//! false rejection. A `review` asks a person and claims nothing either way.
//!
//! The same words are scored a second way over the labelled verifier set
//! ([`score_set`]): there the truth is a person's confirmed label on a diff, and the question
//! is how often the verifier signs what it should not and rejects what it should sign.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CheckResult, Frozen, GradeInput};
use crate::metrics::wilson;
use crate::verifier_set::{Case, Expected};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agreement {
    Agrees,
    FalseApproval,
    FalseRejection,
    /// Nothing to compare, and why.
    Abstained(String),
}

/// The pack's `recommendation.decision` (`merge`, `review`, `reject`), when the run has a pack.
pub fn pack_decision(frozen: Frozen<'_>) -> Result<Option<String>, String> {
    let Some(bytes) = frozen.artifact("mrp.json") else {
        return Ok(None);
    };
    let pack: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("mrp.json is not JSON: {e}"))?;
    pack["recommendation"]["decision"]
        .as_str()
        .map(|decision| Some(decision.to_owned()))
        .ok_or_else(|| "mrp.json has no recommendation.decision".to_owned())
}

/// Compares the pack with the exam.
pub fn agreement(input: &GradeInput<'_>) -> Result<Agreement, String> {
    let Some(frozen) = input.frozen else {
        return Err("the frozen run is missing".to_owned());
    };
    let Some(decision) = pack_decision(frozen)? else {
        return Ok(Agreement::Abstained("the run has no pack".to_owned()));
    };
    let Some(exam) = &input.record.exam else {
        return Ok(Agreement::Abstained(
            "no hidden tests to compare with".to_owned(),
        ));
    };
    Ok(compare_decision(&decision, exam.passed))
}

/// A verdict word against the truth about the work.
pub fn compare_decision(decision: &str, work_is_good: bool) -> Agreement {
    match (decision, work_is_good) {
        ("merge", true) | ("reject", false) => Agreement::Agrees,
        ("merge", false) => Agreement::FalseApproval,
        ("reject", true) => Agreement::FalseRejection,
        (other, _) => Agreement::Abstained(format!("the verdict was {other}")),
    }
}

pub fn agrees(input: &GradeInput<'_>) -> CheckResult {
    const NAME: &str = "verifier.agrees";
    match agreement(input) {
        Err(why) => CheckResult::error(NAME, why),
        Ok(Agreement::Agrees) => CheckResult::pass(NAME, "the pack and the hidden tests agree"),
        Ok(Agreement::FalseApproval) => CheckResult::fail(
            NAME,
            "false approval: the pack said merge and the hidden tests fail",
        ),
        Ok(Agreement::FalseRejection) => CheckResult::fail(
            NAME,
            "false rejection: the pack said reject and the hidden tests pass",
        ),
        Ok(Agreement::Abstained(why)) => CheckResult::info(NAME, format!("not compared: {why}")),
    }
}

/// What each verifier said about each case: `{"<case id>": {"prill": "merge", "reviewer": "reject"}}`.
/// A word is `merge`, `review` or `reject`: the pack's `recommendation.decision`, and the same
/// three for the Reviewer profile's `Verdict:` line. A missing entry is a verifier that
/// was not asked.
pub type Verdicts = BTreeMap<String, CaseVerdicts>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseVerdicts {
    #[serde(default)]
    pub prill: Option<String>,
    #[serde(default)]
    pub reviewer: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScoreError {
    #[error(
        "{} of {} labels are not confirmed (first: {}); read them with `verifier-labels`, or pass \
         --allow-unconfirmed for a provisional figure",
        .ids.len(), .total, .ids.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
    )]
    Unconfirmed { ids: Vec<String>, total: usize },
    #[error("the verdicts name {0}, which is not a case of the set")]
    UnknownCase(String),
    #[error("case {case}: {word:?} is not a verdict (merge, review or reject)")]
    UnknownWord { case: String, word: String },
}

/// A count out of a number of cases, with its 95 % Wilson interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub count: u32,
    pub of: u32,
    pub wilson: Option<(f64, f64)>,
}

impl Rate {
    fn new(count: u32, of: u32) -> Self {
        Self {
            count,
            of,
            wilson: wilson(count, of),
        }
    }
}

/// One verifier over the set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceScore {
    pub source: String,
    /// Cases it gave a word for.
    pub judged: u32,
    /// Cases it was not asked about.
    pub missing: u32,
    /// `merge` on work labelled reject, out of the reject-labelled cases it judged.
    pub false_approval: Rate,
    /// `reject` on work labelled merge, out of the merge-labelled cases it judged.
    pub false_rejection: Rate,
    /// Cases it handed to a person: neither an approval nor a rejection.
    pub reviews: u32,
    pub false_approvals: Vec<String>,
    pub false_rejections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetScore {
    pub cases: u32,
    pub merge_labelled: u32,
    pub reject_labelled: u32,
    /// Labels no person has confirmed; the figures are provisional unless this is zero.
    pub unconfirmed: u32,
    pub prill: SourceScore,
    /// Present when any verdict came from the Reviewer profile.
    pub reviewer: Option<SourceScore>,
}

fn score_source(
    source: &str,
    cases: &[Case],
    word_of: impl Fn(&CaseVerdicts) -> Option<&String>,
    verdicts: &Verdicts,
) -> SourceScore {
    let (mut judged, mut missing, mut reviews) = (0, 0, 0);
    let (mut good_judged, mut bad_judged) = (0, 0);
    let (mut false_approvals, mut false_rejections) = (Vec::new(), Vec::new());
    for case in cases {
        let Some(word) = verdicts.get(&case.id).and_then(&word_of) else {
            missing += 1;
            continue;
        };
        judged += 1;
        let good = case.label.expected == Expected::Merge;
        if good {
            good_judged += 1;
        } else {
            bad_judged += 1;
        }
        match compare_decision(word, good) {
            Agreement::FalseApproval => false_approvals.push(case.id.clone()),
            Agreement::FalseRejection => false_rejections.push(case.id.clone()),
            Agreement::Abstained(_) => reviews += 1,
            Agreement::Agrees => {}
        }
    }
    SourceScore {
        source: source.to_owned(),
        judged,
        missing,
        false_approval: Rate::new(false_approvals.len() as u32, bad_judged),
        false_rejection: Rate::new(false_rejections.len() as u32, good_judged),
        reviews,
        false_approvals,
        false_rejections,
    }
}

/// Scores the verdicts against the labels. A set with an unconfirmed label is refused unless
/// `allow_unconfirmed`: a figure against labels nobody read measures the generator's guess.
pub fn score_set(
    cases: &[Case],
    verdicts: &Verdicts,
    allow_unconfirmed: bool,
) -> Result<SetScore, ScoreError> {
    let unconfirmed: Vec<String> = cases
        .iter()
        .filter(|c| !c.label.confirmed)
        .map(|c| c.id.clone())
        .collect();
    if !unconfirmed.is_empty() && !allow_unconfirmed {
        return Err(ScoreError::Unconfirmed {
            ids: unconfirmed,
            total: cases.len(),
        });
    }
    for (id, entry) in verdicts {
        if !cases.iter().any(|c| &c.id == id) {
            return Err(ScoreError::UnknownCase(id.clone()));
        }
        for word in [&entry.prill, &entry.reviewer].into_iter().flatten() {
            if !matches!(word.as_str(), "merge" | "review" | "reject") {
                return Err(ScoreError::UnknownWord {
                    case: id.clone(),
                    word: word.clone(),
                });
            }
        }
    }
    let merge_labelled = cases
        .iter()
        .filter(|c| c.label.expected == Expected::Merge)
        .count() as u32;
    let any_reviewer = verdicts.values().any(|v| v.reviewer.is_some());
    Ok(SetScore {
        cases: cases.len() as u32,
        merge_labelled,
        reject_labelled: cases.len() as u32 - merge_labelled,
        unconfirmed: unconfirmed.len() as u32,
        prill: score_source("prill", cases, |v| v.prill.as_ref(), verdicts),
        reviewer: any_reviewer
            .then(|| score_source("reviewer", cases, |v| v.reviewer.as_ref(), verdicts)),
    })
}

fn percent(rate: &Rate) -> String {
    match rate.wilson {
        Some((low, high)) => format!(
            "{}/{} ({:.0} %, {:.0}-{:.0} %)",
            rate.count,
            rate.of,
            100.0 * f64::from(rate.count) / f64::from(rate.of),
            100.0 * low,
            100.0 * high
        ),
        None => format!("{}/{}", rate.count, rate.of),
    }
}

/// The score as a short report.
pub fn score_markdown(score: &SetScore) -> String {
    let mut text = format!(
        "# Verifier set\n\n{} cases: {} should merge, {} should be rejected.\n",
        score.cases, score.merge_labelled, score.reject_labelled
    );
    if score.unconfirmed > 0 {
        text.push_str(&format!(
            "\n**Provisional:** {} labels are not confirmed by a person.\n",
            score.unconfirmed
        ));
    }
    for source in std::iter::once(&score.prill).chain(score.reviewer.as_ref()) {
        text.push_str(&format!(
            "\n## {}\n\n- false approval: {}\n- false rejection: {}\n- handed to a person: {}\n- judged {} of {} cases ({} not asked)\n",
            source.source,
            percent(&source.false_approval),
            percent(&source.false_rejection),
            source.reviews,
            source.judged,
            score.cases,
            source.missing
        ));
        if !source.false_approvals.is_empty() {
            text.push_str(&format!(
                "- signed what it should not: {}\n",
                source.false_approvals.join(", ")
            ));
        }
        if !source.false_rejections.is_empty() {
            text.push_str(&format!(
                "- rejected what it should sign: {}\n",
                source.false_rejections.join(", ")
            ));
        }
    }
    text
}
