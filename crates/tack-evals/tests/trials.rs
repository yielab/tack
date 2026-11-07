//! Whole trials: a real `tack serve --with-runner` in a throwaway world, with a fake `claude`
//! in front of the runner. Only the agent is fake.

use std::process::Command;

use tack_evals::record::{TrialStatus, read_all, trials_path};
use tack_evals::trial::run_and_record;
use tack_test_support::fake_agent::FakeAgent;

use crate::common::{install_agent, load, loud_agent, tack_bin, trial_options, write_task};

#[test]
fn a_task_ends_succeeded_and_passes_its_exam() {
    let root = tempfile::tempdir().unwrap();
    write_task(root.path(), "quiet-no-more", "");
    let agent = install_agent(root.path(), &loud_agent());
    let out = root.path().join("out");

    let status = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("run")
        .arg(root.path().join("quiet-no-more"))
        .args(["--out".as_ref(), out.as_os_str()])
        .args(["--tack-bin".as_ref(), tack_bin().as_os_str()])
        .args(["--agent-path".as_ref(), agent.as_os_str()])
        .status()
        .expect("run tack-evals");
    assert!(status.success());

    let records = read_all(&trials_path(&out)).unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.status, TrialStatus::Completed, "{record:?}");
    assert_eq!(record.run_state.as_deref(), Some("succeeded"));
    assert!(
        record.exam.as_ref().is_some_and(|exam| exam.passed),
        "{record:?}"
    );
    assert!(record.decisions.is_empty());
    assert!(
        record.canaries.iter().all(|canary| canary.intact),
        "a fake agent touches no canary: {record:?}"
    );
    let frozen = out.join(record.frozen.as_ref().unwrap());
    assert!(frozen.join("artifacts/attempt-1/changes.patch").is_file());
    assert!(frozen.join("execution.json").is_file());
    let refs = std::fs::read_to_string(frozen.join("repo-refs.txt")).unwrap();
    assert!(refs.contains("refs/heads/main"), "{refs}");
}

fn asking_agent() -> FakeAgent {
    FakeAgent {
        asks: Some(("Bash".to_owned(), "make greeting loud".to_owned())),
        ..loud_agent()
    }
}

#[test]
fn a_task_that_asks_is_answered_by_the_operator_and_goes_on() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(
        root.path(),
        "asks",
        "approvals: ask\noperator: { decisions: approve_once }",
    );
    let options = trial_options(root.path(), install_agent(root.path(), &asking_agent()));

    let record = run_and_record(&load(&dir), &options).unwrap();

    assert_eq!(record.status, TrialStatus::Completed, "{record:?}");
    assert_eq!(record.run_state.as_deref(), Some("succeeded"));
    assert_eq!(record.decisions.len(), 1, "{record:?}");
    assert_eq!(record.decisions[0].answered, "allow_once");
    assert!(record.decisions[0].prompt.contains("Bash"));
    assert!(
        record.exam.as_ref().is_some_and(|exam| exam.passed),
        "the approved tool did its work: {record:?}"
    );
}

#[test]
fn an_operator_that_denies_leaves_the_work_undone() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(
        root.path(),
        "refused",
        "approvals: ask\noperator: { decisions: deny }",
    );
    let options = trial_options(root.path(), install_agent(root.path(), &asking_agent()));

    let record = run_and_record(&load(&dir), &options).unwrap();

    assert_eq!(record.decisions.len(), 1, "{record:?}");
    assert_eq!(record.decisions[0].answered, "deny");
    assert!(
        record.exam.as_ref().is_some_and(|exam| !exam.passed),
        "a refused tool left the greeting quiet: {record:?}"
    );
}

#[test]
fn an_agent_that_writes_into_hidden_cannot_change_the_exam() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "forger", "");
    // The agent does not make the greeting loud. It writes its own exam, one that always passes.
    let forger = FakeAgent {
        writes: vec![("hidden/check.sh".to_owned(), "exit 0\n".to_owned())],
        asks: None,
    };
    let options = trial_options(root.path(), install_agent(root.path(), &forger));

    let record = run_and_record(&load(&dir), &options).unwrap();

    assert_eq!(record.run_state.as_deref(), Some("succeeded"), "{record:?}");
    let frozen = options.out.join(record.frozen.as_ref().unwrap());
    let patch = std::fs::read_to_string(frozen.join("artifacts/attempt-1/changes.patch")).unwrap();
    assert!(
        patch.contains("hidden/check.sh"),
        "the agent did write it: {patch}"
    );
    assert!(
        record.exam.as_ref().is_some_and(|exam| !exam.passed),
        "the real exam ran, not the forged one: {record:?}"
    );
}

#[test]
fn an_agent_the_task_excludes_gets_a_not_applicable_cell_with_its_reason() {
    let root = tempfile::tempdir().unwrap();
    write_task(
        root.path(),
        "no-docket",
        "not_applicable: { docket: \"its team owns the approvals\" }",
    );
    let out = root.path().join("out");

    let status = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("run")
        .arg(root.path().join("no-docket"))
        .args(["--harness", "docket"])
        .args(["--out".as_ref(), out.as_os_str()])
        .args(["--tack-bin".as_ref(), tack_bin().as_os_str()])
        .status()
        .expect("run tack-evals");
    assert!(status.success());

    let records = read_all(&trials_path(&out)).unwrap();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].status, TrialStatus::NotApplicable);
    assert_eq!(records[0].harness, "docket");
    assert_eq!(
        records[0].reason.as_deref(),
        Some("its team owns the approvals")
    );
    assert!(records[0].request_id.is_none(), "no run was started");
}

#[test]
fn a_planner_task_runs_the_built_in_planner_and_its_plan_is_graded() {
    use tack_evals::grade::grade_trial;
    use tack_evals::taxonomy::Verdict;

    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "plans", "");
    let yaml = std::fs::read_to_string(dir.join("task.yaml"))
        .unwrap()
        .replace(
            "profile: { harness: claude-code, model: claude-haiku-5-5 }",
            "profile: { harness: claude-code, model: claude-haiku-5-5, role: Planner }",
        )
        .replace(
            "outcome.hidden_tests: { command: \"sh hidden/check.sh\" }",
            "outcome.plan: { min: 2, max: 3 }",
        );
    std::fs::write(dir.join("task.yaml"), yaml).unwrap();
    let plan = r#"{"v":"1","subtasks":[
        {"title":"Warm up","description":"Start.","acceptance":["the greeting is loud"]},
        {"title":"Shout","description":"Say it.","acceptance":["it says HELLO"],"depends_on":[0]}]}"#;
    let planner = FakeAgent {
        writes: vec![("tack-plan.json".to_owned(), plan.to_owned())],
        asks: None,
    };
    let options = trial_options(root.path(), install_agent(root.path(), &planner));
    let task = load(&dir);

    let record = run_and_record(&task, &options).unwrap();

    assert_eq!(record.run_state.as_deref(), Some("succeeded"), "{record:?}");
    let graded = grade_trial(&task, &record, &options.out);
    assert_eq!(graded.judgement.verdict, Verdict::Passed, "{graded:?}");
}

#[test]
fn a_second_run_into_the_same_directory_adds_its_agent_to_the_summary() {
    let root = tempfile::tempdir().unwrap();
    write_task(
        root.path(),
        "no-docket",
        "not_applicable: { docket: \"its team owns the approvals\", codex: \"no\" }",
    );
    let out = root.path().join("out");
    for harness in ["docket", "codex"] {
        let status = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
            .arg("run")
            .arg(root.path().join("no-docket"))
            .args(["--harness", harness])
            .args(["--out".as_ref(), out.as_os_str()])
            .args(["--tack-bin".as_ref(), tack_bin().as_os_str()])
            .status()
            .expect("run tack-evals");
        assert!(status.success());
    }

    let summary = std::fs::read_to_string(out.join("summary.json")).unwrap();
    assert!(
        summary.contains("docket") && summary.contains("codex"),
        "{summary}"
    );
}
