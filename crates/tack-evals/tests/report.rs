//! The summary, `compare` and `nightly` over trials written by hand.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tack_evals::grade::{grade_file, grade_trial};
use tack_evals::record::{TrialStatus, append};
use tack_evals::report::{compare, summarize, summary_markdown, verdict, warnings};
use tack_evals::task::LoadedTask;

use crate::common::{load, write_task};
use crate::synthetic::{fail_exam, freeze, good_record, standard_task, write_epochs};

fn near(got: f64, want: f64) {
    assert!((got - want).abs() < 1e-3, "{got} is not {want}");
}

/// Two tasks in `root`: the second gets the version given.
fn task_dirs(root: &Path, ids: &[&str]) -> Vec<LoadedTask> {
    ids.iter().map(|id| standard_task(root, id)).collect()
}

fn graded(out: &Path, tasks: &[LoadedTask]) -> Vec<tack_evals::grade::GradedTrial> {
    grade_file(&out.join("trials.jsonl"), tasks).unwrap()
}

#[test]
fn the_summary_counts_only_what_counts() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("out");
    let tasks = task_dirs(root.path(), &["alpha", "beta"]);
    write_epochs(&out, &tasks[0], &[true, true, true]);
    write_epochs(&out, &tasks[1], &[true, false, false]);
    // A fourth epoch of beta that never reached the model: infra, left out of the rate.
    let mut silent = good_record(&tasks[1], 4);
    silent.status = TrialStatus::TimedOut;
    silent.counted_tokens = None;
    freeze(&out, &silent, &[]);
    append(&out, &silent).unwrap();

    let summary = summarize(&graded(&out, &tasks));
    assert_eq!(summary.trials, 7);
    assert_eq!(summary.failures.agent, 2);
    assert_eq!(summary.failures.infra, 1);
    assert_eq!(summary.failures.counted(), 2);
    let beta = &summary.tasks[1];
    assert_eq!((beta.passed, beta.counted), (1, 3));
    near(summary.pass_at_1.unwrap(), 4.0 / 6.0);
    let (low, high) = summary.wilson.unwrap();
    assert!(low < 4.0 / 6.0 && 4.0 / 6.0 < high);
    assert!(summary.clustered_standard_error.unwrap() > 0.0);
    // k is the fewest counted trials of any task: 3.
    let pow = summary.pass_pow_k.unwrap();
    assert_eq!(pow.k, 3);
    near(pow.value, 0.5);
    near(summary.pass_at_k.unwrap().value, 1.0);
    assert_eq!(summary.tokens_per_accepted_run.unwrap().p50, 1_000);

    let text = summary_markdown(&summary, None);
    assert!(text.contains("pass@1 0.67"), "{text}");
    assert!(text.contains("infra 1 (left out)"), "{text}");
}

#[test]
fn the_verifier_counts_are_over_the_packs_that_said_merge_or_reject() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("out");
    let task = standard_task(root.path(), "verified");
    for (epoch, (word, exam_passes)) in [
        ("merge", true),
        ("merge", false),
        ("reject", false),
        ("reject", true),
        ("review", true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut record = good_record(&task, epoch as u32 + 1);
        if !exam_passes {
            fail_exam(&mut record);
        }
        let pack = format!(r#"{{"recommendation":{{"decision":"{word}","rationale":""}}}}"#);
        freeze(&out, &record, &[("artifacts/attempt-1/mrp.json", &pack)]);
        append(&out, &record).unwrap();
    }
    let summary = summarize(&graded(&out, &[task]));
    let v = summary.verifier;
    assert_eq!((v.merge, v.false_approvals), (2, 1));
    assert_eq!((v.reject, v.false_rejections), (2, 1));
    assert!(summary_markdown(&summary, None).contains("false approval 1/2"));
}

fn nightly(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("nightly")
        .args(args)
        .output()
        .expect("run tack-evals")
}

fn paths(out: &Path, name: &str) -> PathBuf {
    out.join(name).join("trials.jsonl")
}

/// A base where `alpha` and `beta` passed all three epochs.
fn perfect_base(root: &Path) -> Vec<LoadedTask> {
    let tasks = task_dirs(root, &["alpha", "beta"]);
    let base = root.join("base");
    write_epochs(&base, &tasks[0], &[true; 3]);
    write_epochs(&base, &tasks[1], &[true; 3]);
    tasks
}

#[test]
fn compare_pairs_two_synthetic_runs() {
    let root = tempfile::tempdir().unwrap();
    let tasks = perfect_base(root.path());
    let head = root.path().join("head");
    write_epochs(&head, &tasks[0], &[true, false, false]);
    write_epochs(&head, &tasks[1], &[true; 3]);

    let output = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("compare")
        .args(["--base".as_ref(), paths(root.path(), "base").as_os_str()])
        .args(["--head".as_ref(), paths(root.path(), "head").as_os_str()])
        .args(["--out".as_ref(), root.path().join("report").as_os_str()])
        .arg("--tasks")
        .arg(root.path().join("alpha"))
        .arg(root.path().join("beta"))
        .output()
        .expect("run tack-evals");
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("| alpha@1 | claude-code | 3/3 | 1/3 |"),
        "{text}"
    );
    assert!(
        text.contains("| beta@1 | claude-code | 3/3 | 3/3 |"),
        "{text}"
    );
    // Two tasks, one dropped by 2/3: the mean is -1/3 and two tasks cannot tell it from noise.
    assert!(text.contains("-0.33"), "{text}");
    assert!(text.contains("no detectable difference"), "{text}");
    assert!(root.path().join("report/compare.md").is_file());

    let base = summarize(&graded(&root.path().join("base"), &tasks));
    let head = summarize(&graded(&root.path().join("head"), &tasks));
    let comparison = compare(&base, &head);
    assert_eq!(comparison.paired.len(), 2);
    assert_eq!(verdict(&comparison), "no detectable difference");
}

#[test]
fn a_task_at_another_version_is_never_paired() {
    let root = tempfile::tempdir().unwrap();
    let tasks = perfect_base(root.path());
    // `alpha` is rewritten at version 2 in a second directory and run there.
    let second = root.path().join("v2");
    write_task(&second, "alpha", "");
    let yaml = std::fs::read_to_string(second.join("alpha/task.yaml")).unwrap();
    std::fs::write(
        second.join("alpha/task.yaml"),
        yaml.replace("version: 1", "version: 2"),
    )
    .unwrap();
    let alpha2 = load(&second.join("alpha"));
    let head = root.path().join("head");
    write_epochs(&head, &alpha2, &[false; 3]);
    write_epochs(&head, &tasks[1], &[true; 3]);

    let mut all = tasks.clone();
    all.push(alpha2);
    let comparison = compare(
        &summarize(&graded(&root.path().join("base"), &all)),
        &summarize(&graded(&root.path().join("head"), &all)),
    );
    assert_eq!(comparison.paired.len(), 1, "{comparison:?}");
    assert!(
        comparison
            .unpaired
            .iter()
            .any(|l| l.contains("alpha@2") && l.contains("never mixed"))
    );
    assert_eq!(verdict(&comparison), "not enough paired tasks to compare");
}

#[test]
fn nightly_warns_on_a_task_that_went_from_three_of_three_to_one_of_three() {
    let root = tempfile::tempdir().unwrap();
    let tasks = perfect_base(root.path());
    let head = root.path().join("head");
    write_epochs(&head, &tasks[0], &[true, false, false]);
    write_epochs(&head, &tasks[1], &[true; 3]);
    let out = root.path().join("nightly-out");

    let output = nightly_over(root.path(), &head, &out);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("WARNING  alpha@1 on claude-code passed 3/3 in the base and now passes 1/3"),
        "{text}"
    );
    assert!(!text.contains("beta@1 on"), "{text}");
    let raised: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("warnings.json")).unwrap()).unwrap();
    assert_eq!(raised.as_array().unwrap().len(), 1);
    assert!(out.join("summary.md").is_file() && out.join("compare.md").is_file());

    let comparison = compare(
        &summarize(&graded(&root.path().join("base"), &tasks)),
        &summarize(&graded(&head, &tasks)),
    );
    assert_eq!(warnings(&comparison).len(), 1);
}

fn nightly_over(root: &Path, head: &Path, out: &Path) -> Output {
    nightly(&[
        root.join("alpha").to_str().unwrap(),
        root.join("beta").to_str().unwrap(),
        "--base",
        root.join("base/trials.jsonl").to_str().unwrap(),
        "--trials",
        head.join("trials.jsonl").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ])
}

#[test]
fn nightly_does_not_warn_on_infra_or_eval_failures_and_exits_zero() {
    let root = tempfile::tempdir().unwrap();
    let tasks = perfect_base(root.path());
    let head = root.path().join("head");
    // `alpha`: one pass, two runs that never reached the model. `beta`: one pass, two
    // trials the harness could not run.
    write_epochs(&head, &tasks[0], &[true]);
    for epoch in [2, 3] {
        let mut silent = good_record(&tasks[0], epoch);
        silent.status = TrialStatus::TimedOut;
        silent.counted_tokens = None;
        freeze(&head, &silent, &[]);
        append(&head, &silent).unwrap();
    }
    write_epochs(&head, &tasks[1], &[true]);
    for epoch in [2, 3] {
        let mut broken = good_record(&tasks[1], epoch);
        broken.status = TrialStatus::Error;
        broken.reason = Some("the server did not start".to_owned());
        broken.frozen = None;
        append(&head, &broken).unwrap();
    }

    let output = nightly_over(root.path(), &head, &root.path().join("nightly-out"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("WARNING"), "{text}");
    assert!(text.contains("0 warnings"), "{text}");
}

#[test]
fn nightly_does_not_warn_when_the_base_was_not_perfect() {
    let root = tempfile::tempdir().unwrap();
    let tasks = task_dirs(root.path(), &["alpha", "beta"]);
    let base = root.path().join("base");
    write_epochs(&base, &tasks[0], &[true, true, false]);
    write_epochs(&base, &tasks[1], &[true; 3]);
    let head = root.path().join("head");
    write_epochs(&head, &tasks[0], &[false; 3]);
    write_epochs(&head, &tasks[1], &[true; 3]);

    let output = nightly_over(root.path(), &head, &root.path().join("nightly-out"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn a_trial_of_an_unknown_task_cannot_be_judged() {
    let root = tempfile::tempdir().unwrap();
    let tasks = task_dirs(root.path(), &["alpha"]);
    write_epochs(&root.path().join("out"), &tasks[0], &[true]);
    let failure = grade_file(&root.path().join("out/trials.jsonl"), &[]).unwrap_err();
    assert!(format!("{failure:#}").contains("alpha@1"), "{failure:#}");
    // And a trial is judged the same when read again: the verdict is a function of the record.
    let again = grade_file(&root.path().join("out/trials.jsonl"), &tasks).unwrap();
    let record = &tack_evals::record::read_all(&root.path().join("out/trials.jsonl")).unwrap()[0];
    assert_eq!(
        again[0],
        grade_trial(&tasks[0], record, &root.path().join("out"))
    );
}
