use std::path::Path;

use tack_evals::task::LoadedTask;

use crate::common::{load, task_yaml, write_task};

fn refusal(dir: &Path) -> String {
    LoadedTask::load(dir)
        .expect_err("the task is refused")
        .to_string()
}

#[test]
fn a_task_without_required_graders_is_refused_with_its_reason() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "no-graders", "");
    let yaml = task_yaml("no-graders", "");
    let without = yaml.split("graders:").next().unwrap().to_owned() + "limits: { wall_s: 60 }\n";
    std::fs::write(dir.join("task.yaml"), without).unwrap();

    let reason = refusal(&dir);
    assert!(reason.contains("graders.required is empty"), "{reason}");
    assert!(reason.contains("no-graders"), "{reason}");
}

#[test]
fn a_task_that_does_not_parse_says_so() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "garbled", "");
    std::fs::write(dir.join("task.yaml"), "id: [unclosed\n").unwrap();
    assert!(refusal(&dir).contains("does not parse"));
}

#[test]
fn a_misspelled_check_is_refused_instead_of_never_running() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "typo", "");
    let yaml = task_yaml("typo", "").replace("governance.no_violation", "governance.no_violations");
    std::fs::write(dir.join("task.yaml"), yaml).unwrap();
    assert!(refusal(&dir).contains("unknown check"));
}

#[test]
fn a_hidden_check_needs_its_hidden_directory() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "no-exam", "");
    std::fs::remove_dir_all(dir.join("hidden")).unwrap();
    assert!(refusal(&dir).contains("needs a hidden/ directory"));
}

#[test]
fn the_id_is_the_name_of_the_directory() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "right-name", "");
    let moved = root.path().join("other-name");
    std::fs::rename(&dir, &moved).unwrap();
    assert!(refusal(&moved).contains("must be the name of its directory"));
}

#[test]
fn a_check_is_a_bare_name_or_a_name_with_its_argument() {
    let root = tempfile::tempdir().unwrap();
    let task = LoadedTask::load(&write_task(root.path(), "forms", "")).unwrap();
    let required = &task.task.graders.required;
    assert_eq!(required[0].name, "outcome.hidden_tests");
    assert_eq!(task.hidden_command(), Some("sh hidden/check.sh"));
    assert_eq!(required[1].args, "succeeded");
    assert!(required[2].args.is_null());
}

#[test]
fn a_cell_that_is_not_applicable_needs_a_known_agent_and_its_reason() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(
        root.path(),
        "n-a",
        "not_applicable: { docket: \"its team decides\" }",
    );
    assert_eq!(
        load(&dir)
            .task
            .not_applicable
            .get("docket")
            .map(String::as_str),
        Some("its team decides")
    );

    std::fs::write(
        dir.join("task.yaml"),
        task_yaml("n-a", "not_applicable: { dockett: \"typo\" }"),
    )
    .unwrap();
    assert!(refusal(&dir).contains("not_applicable names \"dockett\""));

    std::fs::write(
        dir.join("task.yaml"),
        task_yaml("n-a", "not_applicable: { docket: \"\" }"),
    )
    .unwrap();
    assert!(refusal(&dir).contains("needs its reason"));
}
