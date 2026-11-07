//! Did the verifier's pack agree with the hidden tests. The tests are the truth here: a
//! `merge` on work that fails them is a false approval, a `reject` on work that passes them a
//! false rejection. A `review` asks a person and claims nothing either way.

use serde::{Deserialize, Serialize};

use super::{CheckResult, Frozen, GradeInput};

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
    Ok(match (decision.as_str(), exam.passed) {
        ("merge", true) | ("reject", false) => Agreement::Agrees,
        ("merge", false) => Agreement::FalseApproval,
        ("reject", true) => Agreement::FalseRejection,
        (other, _) => Agreement::Abstained(format!("the pack said {other}")),
    })
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
