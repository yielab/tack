use std::path::Path;
use std::process::{Command, Output};

use crate::common::write_task;

fn check(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("check-solutions")
        .arg(path)
        .output()
        .expect("run tack-evals")
}

#[test]
fn a_reference_solution_that_passes_its_exam_exits_zero() {
    let root = tempfile::tempdir().unwrap();
    write_task(root.path(), "sound", "");
    let output = check(root.path());
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("ok       sound"));
}

#[test]
fn the_shipped_fixture_task_is_sound() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tasks");
    let output = check(&fixture);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_broken_reference_solution_exits_one_and_names_the_task() {
    let root = tempfile::tempdir().unwrap();
    write_task(root.path(), "sound", "");
    let broken = write_task(root.path(), "broken", "");
    std::fs::write(broken.join("solution/greeting.txt"), "still quiet\n").unwrap();

    let output = check(root.path());
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("BROKEN   broken"), "{said}");
    assert!(
        said.contains("ok       sound"),
        "the sound task still reports: {said}"
    );
}

#[test]
fn a_task_without_a_solution_is_broken() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "unsolved", "");
    std::fs::remove_dir_all(dir.join("solution")).unwrap();
    let output = check(root.path());
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn a_solution_given_as_a_patch_is_applied() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_task(root.path(), "patched", "");
    std::fs::remove_file(dir.join("solution/greeting.txt")).unwrap();
    std::fs::write(
        dir.join("solution/loud.patch"),
        "--- a/greeting.txt\n+++ b/greeting.txt\n@@ -1 +1 @@\n-hello\n+HELLO\n",
    )
    .unwrap();
    let output = check(root.path());
    assert!(output.status.success(), "{output:?}");
}
