use std::process::Command;

use super::*;
use crate::client::{AttemptId, WorkspaceId, workspace::git::GitWorktreeProvisioner};

fn git(directory: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn a_changed_workspace_yields_a_patch_and_a_file_list() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipped: the git binary is not on PATH");
        return;
    }
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path().join("ws");
    fs::create_dir_all(&path).expect("workspace");
    assert!(git(&path, &["init", "--quiet"]));
    fs::write(path.join("keep.txt"), "keep\n").expect("keep");
    fs::write(path.join("gone.txt"), "gone\n").expect("gone");
    assert!(git(&path, &["add", "-A"]));
    assert!(git(&path, &["commit", "--quiet", "-m", "base"]));
    let base = Command::new("git")
        .current_dir(&path)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let base = String::from_utf8_lossy(&base.stdout).trim().to_owned();

    fs::remove_file(path.join("gone.txt")).expect("delete");
    fs::write(path.join("new.txt"), "new\n").expect("add");
    fs::create_dir_all(path.join(".tack-runner")).expect("runner dir");
    fs::write(path.join(".tack-runner/codex-run.log"), "log").expect("log");
    fs::write(path.join(".tack-attempt"), "attempt").expect("marker");

    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt"),
        id: WorkspaceId::new("ws_attempt"),
        path: path.clone(),
        base_revision: base.clone(),
    };
    let workspaces =
        WorkspaceManager::new(root.path().join("root"), GitWorktreeProvisioner::default());
    let (scratch, staged) = capture(
        &workspaces,
        &workspace,
        "codex".to_owned(),
        serde_json::json!({"code": "completed"}),
        serde_json::Value::Null,
        None,
    )
    .await;

    let names: Vec<_> = staged
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["changes.patch", "files.json", "evidence.json"]);
    let bytes = |index: usize| fs::read(staged[index]["staged_path"].as_str().unwrap()).unwrap();
    let patch = bytes(0);
    assert_eq!(staged[0]["sha256"], sha256_hex(&patch));
    assert!(String::from_utf8_lossy(&patch).contains("+new"));
    let files: Vec<FileChange> = serde_json::from_slice(&bytes(1)).unwrap();
    assert_eq!(
        files,
        [
            FileChange {
                path: "gone.txt".into(),
                op: FileOp::Deleted
            },
            FileChange {
                path: "new.txt".into(),
                op: FileOp::Added
            },
        ]
    );
    let evidence: AttemptEvidence = serde_json::from_slice(&bytes(2)).unwrap();
    assert!(evidence.captured && evidence.worktree_dirty == Some(true));
    assert_eq!(evidence.base_commit, base);
    assert_eq!(evidence.head_commit.as_deref(), Some(base.as_str()));
    assert_eq!(evidence.patch.unwrap().size_bytes, patch.len() as u64);
    fs::remove_dir_all(scratch).expect("scratch");
}

#[tokio::test]
async fn a_plan_file_is_staged_and_excluded_from_the_patch() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipped: the git binary is not on PATH");
        return;
    }
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path().join("ws");
    fs::create_dir_all(&path).expect("workspace");
    assert!(git(&path, &["init", "--quiet"]));
    fs::write(path.join("keep.txt"), "keep\n").expect("keep");
    assert!(git(&path, &["add", "-A"]));
    assert!(git(&path, &["commit", "--quiet", "-m", "base"]));
    let base = Command::new("git")
        .current_dir(&path)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let base = String::from_utf8_lossy(&base.stdout).trim().to_owned();
    fs::write(path.join("new.txt"), "new\n").expect("add");
    let plan = r#"{"v":"1","subtasks":[{"title":"t","description":"d"}]}"#;
    fs::write(path.join("tack-plan.json"), plan).expect("plan");

    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt"),
        id: WorkspaceId::new("ws_attempt"),
        path: path.clone(),
        base_revision: base,
    };
    let workspaces =
        WorkspaceManager::new(root.path().join("root"), GitWorktreeProvisioner::default());
    let run = || {
        capture(
            &workspaces,
            &workspace,
            "codex".to_owned(),
            serde_json::json!({"code": "completed"}),
            serde_json::Value::Null,
            None,
        )
    };

    let (scratch, staged) = run().await;
    let item = staged
        .iter()
        .find(|item| item["name"] == "tack-plan.json")
        .expect("the plan is staged");
    assert_eq!(item["kind"], "plan");
    assert_eq!(item["media_type"], "application/vnd.tack.plan+json");
    assert_eq!(
        fs::read(item["staged_path"].as_str().unwrap()).unwrap(),
        plan.as_bytes()
    );
    let patch = fs::read(staged[0]["staged_path"].as_str().unwrap()).unwrap();
    let patch = String::from_utf8_lossy(&patch);
    assert!(patch.contains("+new") && !patch.contains("tack-plan"));
    assert!(!scratch.join(PLAN_INVALID_FILE).exists());
    fs::remove_dir_all(scratch).expect("scratch");

    fs::write(path.join("tack-plan.json"), r#"{"v":"1"}"#).expect("invalid plan");
    let (scratch, staged) = run().await;
    assert!(staged.iter().all(|item| item["kind"] != "plan"));
    let reason = fs::read_to_string(scratch.join(PLAN_INVALID_FILE)).expect("reason");
    assert!(reason.contains("subtasks"), "{reason}");
    fs::remove_dir_all(scratch).expect("scratch");
}
