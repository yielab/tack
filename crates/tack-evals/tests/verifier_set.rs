//! The verifier set: made from a task's reference solution and proved, labelled by a person,
//! and the verdicts scored against those labels.

use std::path::{Path, PathBuf};
use std::process::Command;

use tack_evals::graders::verifier::{ScoreError, Verdicts, score_markdown, score_set};
use tack_evals::load_tasks;
use tack_evals::verifier_set::generate::{Options, check_set, generate};
use tack_evals::verifier_set::{
    Case, Expected, Kind, Label, Proof, Reference, load_set, unconfirmed,
};

use crate::common::write_task;

fn case(id: &str, expected: Expected, confirmed: bool) -> Case {
    Case {
        id: id.to_owned(),
        dir: PathBuf::from(id),
        reference: Reference {
            task: "calc".to_owned(),
            task_version: 1,
            instruction: "do it".to_owned(),
            constraints: Vec::new(),
            allowed_files: None,
        },
        label: Label {
            expected,
            confirmed,
            kind: Kind::Mutant,
            family: None,
            operator: "x".to_owned(),
            reason: "x".to_owned(),
            proof: Proof {
                hidden_tests_passed: expected == Expected::Merge,
                visible_tests_passed: None,
                within_scope: None,
            },
            seed: 1,
        },
    }
}

/// Four cases: two to merge (a, b), two to reject (c, d).
fn four(confirmed: bool) -> Vec<Case> {
    vec![
        case("a", Expected::Merge, confirmed),
        case("b", Expected::Merge, confirmed),
        case("c", Expected::Reject, confirmed),
        case("d", Expected::Reject, confirmed),
    ]
}

fn verdicts(json: &str) -> Verdicts {
    serde_json::from_str(json).unwrap()
}

#[test]
fn a_merge_on_work_to_reject_is_a_false_approval_and_a_reject_on_good_work_a_false_rejection() {
    let score = score_set(
        &four(true),
        &verdicts(
            r#"{
              "a": {"prill": "merge"},  "b": {"prill": "reject"},
              "c": {"prill": "merge"},  "d": {"prill": "reject"}
            }"#,
        ),
        false,
    )
    .unwrap();
    let prill = &score.prill;
    assert_eq!(prill.false_approvals, ["c"]);
    assert_eq!(prill.false_rejections, ["b"]);
    assert_eq!(
        (prill.false_approval.count, prill.false_approval.of),
        (1, 2)
    );
    assert_eq!(
        (prill.false_rejection.count, prill.false_rejection.of),
        (1, 2)
    );
    assert!(prill.false_approval.wilson.is_some());
    assert_eq!(score.reviewer, None, "nobody asked the Reviewer");
}

#[test]
fn a_review_claims_nothing_and_a_case_not_asked_is_not_counted() {
    let score = score_set(
        &four(true),
        &verdicts(
            r#"{"a": {"prill": "review"}, "c": {"prill": "review"}, "d": {"prill": "reject"}}"#,
        ),
        false,
    )
    .unwrap();
    let prill = &score.prill;
    assert_eq!(prill.reviews, 2);
    assert_eq!(prill.missing, 1);
    assert!(prill.false_approvals.is_empty() && prill.false_rejections.is_empty());
    // The denominators are the cases it judged, by label.
    assert_eq!(
        (prill.false_approval.count, prill.false_approval.of),
        (0, 2)
    );
    assert_eq!(
        (prill.false_rejection.count, prill.false_rejection.of),
        (0, 1)
    );
}

#[test]
fn the_reviewer_is_scored_on_its_own_when_it_emits_a_verdict() {
    let score = score_set(
        &four(true),
        &verdicts(
            r#"{
              "a": {"prill": "merge", "reviewer": "merge"},
              "c": {"prill": "reject", "reviewer": "merge"},
              "d": {"prill": "reject", "reviewer": "reject"}
            }"#,
        ),
        false,
    )
    .unwrap();
    assert!(score.prill.false_approvals.is_empty());
    let reviewer = score.reviewer.as_ref().expect("the Reviewer judged");
    assert_eq!(reviewer.false_approvals, ["c"]);
    assert_eq!(reviewer.judged, 3);
    let text = score_markdown(&score);
    assert!(
        text.contains("## prill") && text.contains("## reviewer"),
        "{text}"
    );
    assert!(text.contains("signed what it should not: c"), "{text}");
}

#[test]
fn an_unconfirmed_set_is_refused_unless_allowed() {
    let mut cases = four(true);
    cases[2].label.confirmed = false;
    let empty = Verdicts::new();
    let refused = score_set(&cases, &empty, false).unwrap_err();
    assert_eq!(
        refused,
        ScoreError::Unconfirmed {
            ids: vec!["c".to_owned()],
            total: 4
        }
    );
    assert!(refused.to_string().contains("--allow-unconfirmed"));

    let provisional = score_set(&cases, &empty, true).unwrap();
    assert_eq!(provisional.unconfirmed, 1);
    assert!(score_markdown(&provisional).contains("Provisional"));
    assert_eq!(unconfirmed(&cases).len(), 1);
}

#[test]
fn a_verdict_for_a_case_that_is_not_there_or_in_words_nobody_uses_is_an_error() {
    let cases = four(true);
    assert_eq!(
        score_set(&cases, &verdicts(r#"{"zz": {"prill": "merge"}}"#), false).unwrap_err(),
        ScoreError::UnknownCase("zz".to_owned())
    );
    assert!(matches!(
        score_set(&cases, &verdicts(r#"{"a": {"prill": "approve"}}"#), false).unwrap_err(),
        ScoreError::UnknownWord { .. }
    ));
}

fn generated(root: &Path, name: &str, options: &Options) -> PathBuf {
    let tasks = load_tasks(&[root.join("tasks")]).unwrap();
    let out = root.join(name);
    generate(&tasks, &out, options).unwrap();
    out
}

/// A tree of one task whose solution is a one-line file, so the only mutant is dropping it.
fn one_task(root: &Path) {
    write_task(&root.join("tasks"), "loud", "");
}

#[test]
fn a_generated_set_is_proved_labelled_and_reproducible() {
    let root = tempfile::tempdir().unwrap();
    one_task(root.path());
    let options = Options {
        seed: 7,
        good: 1,
        bad: 1,
    };
    let first = generated(root.path(), "one", &options);
    let second = generated(root.path(), "two", &options);

    let cases = load_set(&first).unwrap();
    assert_eq!(cases.len(), 2);
    let good = cases
        .iter()
        .find(|c| c.label.expected == Expected::Merge)
        .unwrap();
    let bad = cases
        .iter()
        .find(|c| c.label.expected == Expected::Reject)
        .unwrap();
    assert_eq!(good.label.kind, Kind::Reference);
    assert!(good.label.proof.hidden_tests_passed);
    assert_eq!(bad.label.kind, Kind::Mutant);
    assert!(
        !bad.label.proof.hidden_tests_passed,
        "a mutant is proved to fail"
    );
    assert!(cases.iter().all(|c| !c.label.confirmed));
    assert_eq!(unconfirmed(&cases).len(), 2);
    // Neither the id nor the reference tells the label.
    assert!(cases.iter().all(|c| c.id.starts_with("case-")));
    assert_eq!(good.reference.task, "loud");

    // Same tasks, same seed, same bytes.
    for case in &cases {
        let again = std::fs::read(second.join(&case.id).join("diff.patch")).unwrap();
        assert_eq!(std::fs::read(case.diff_path()).unwrap(), again);
    }
    let tasks = load_tasks(&[root.path().join("tasks")]).unwrap();
    assert_eq!(check_set(&first, &tasks).unwrap(), Vec::<String>::new());
}

#[test]
fn a_case_that_no_longer_holds_is_reported() {
    let root = tempfile::tempdir().unwrap();
    one_task(root.path());
    let out = generated(
        root.path(),
        "set",
        &Options {
            seed: 1,
            good: 1,
            bad: 1,
        },
    );
    // The exam gets laxer: the mutant now passes it.
    std::fs::write(root.path().join("tasks/loud/hidden/check.sh"), "true\n").unwrap();
    let tasks = load_tasks(&[root.path().join("tasks")]).unwrap();
    let problems = check_set(&out, &tasks).unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("the checks now say"), "{problems:?}");
}

#[test]
fn a_set_is_not_written_over_labels_a_person_confirmed() {
    let root = tempfile::tempdir().unwrap();
    one_task(root.path());
    let options = Options {
        seed: 1,
        good: 1,
        bad: 1,
    };
    let out = generated(root.path(), "set", &options);
    let tasks = load_tasks(&[root.path().join("tasks")]).unwrap();
    let refused = generate(&tasks, &out, &options).err().expect("refused");
    assert!(refused.to_string().contains("remove it first"), "{refused}");
}

#[test]
fn more_cases_than_the_tasks_can_prove_is_an_error_not_a_short_set() {
    let root = tempfile::tempdir().unwrap();
    one_task(root.path());
    let tasks = load_tasks(&[root.path().join("tasks")]).unwrap();
    let out = root.path().join("greedy");
    let failure = generate(
        &tasks,
        &out,
        &Options {
            seed: 1,
            good: 1,
            bad: 50,
        },
    )
    .err()
    .expect("too many mutants asked for");
    assert!(
        failure.to_string().contains("mutants could be proved"),
        "{failure}"
    );
    assert!(
        !out.exists(),
        "nothing is written for a set that falls short"
    );
}

fn shipped_set() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/tasks/verifier")
}

#[test]
fn the_shipped_set_has_fifteen_and_fifteen_and_every_case_still_holds() {
    let set = shipped_set();
    let cases = load_set(&set).unwrap();
    let merge = cases
        .iter()
        .filter(|c| c.label.expected == Expected::Merge)
        .count();
    assert_eq!((merge, cases.len() - merge), (15, 15));
    for case in &cases {
        assert_eq!(
            case.label.proof.hidden_tests_passed,
            case.label.expected == Expected::Merge && case.label.kind != Kind::Mutant
                || case.label.family.as_deref() == Some("scope"),
            "{}: the proof does not fit the label",
            case.id
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("gen-verifier-set")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/tasks/regression"))
        .arg("--out")
        .arg(&set)
        .arg("--check")
        .output()
        .expect("run tack-evals");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
