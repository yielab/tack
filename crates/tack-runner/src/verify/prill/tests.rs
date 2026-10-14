use std::{path::PathBuf, process::Command as SyncCommand};

use super::*;
use crate::{
    client::{AttemptId, WorkspaceId},
    config::VerifyKind,
    harness::fixtures::fake_harness_path,
    verify::{MRP_MEDIA_TYPE, run as run_verifier},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn prill_fixture(name: &str) -> Value {
    let path = repo_root().join(format!("docs/contracts/mrp-v1/fixtures/prill/{name}.json"));
    serde_json::from_slice(&fs::read(&path).expect("prill fixture")).expect("json")
}

/// Resolves `git` to an absolute path, for the reason `workspace::git`'s
/// tests do: another test may overwrite `PATH` while this one runs.
fn git_program() -> PathBuf {
    for candidate in [
        "/usr/bin/git",
        "/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = SyncCommand::new(git_program())
        .current_dir(directory)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real repository with one commit, a tracked file the harness then
/// modified, one it deleted, one it added, the runner's three markers, and
/// an ignored file; returns the base commit.
fn harness_left_work(path: &Path) -> String {
    git(path, &["-c", "init.defaultBranch=main", "init", "--quiet"]);
    git(path, &["config", "user.email", "runner@example.invalid"]);
    git(path, &["config", "user.name", "Tack Runner Test"]);
    fs::write(path.join("README.md"), "first\n").expect("write");
    fs::write(path.join("gone.txt"), "to be deleted\n").expect("write");
    fs::write(path.join(".gitignore"), "ignored.log\n").expect("write");
    git(path, &["add", "."]);
    git(path, &["commit", "--quiet", "-m", "base"]);
    let base = git(path, &["rev-parse", "HEAD"]);
    fs::write(path.join("README.md"), "edited by the harness\n").expect("write");
    fs::remove_file(path.join("gone.txt")).expect("delete");
    fs::write(path.join("new.rs"), "fn main() {}\n").expect("write");
    fs::write(path.join("ignored.log"), "noise\n").expect("write");
    fs::create_dir_all(path.join(".tack-runner")).expect("marker dir");
    fs::write(path.join(".tack-runner/run.log"), "runner log\n").expect("write");
    fs::write(path.join(".tack-attempt"), "attempt\n").expect("write");
    fs::write(path.join(".tack-checkout"), base.as_str()).expect("write");
    base
}

fn tack_evidence(attempt_id: &str, base_commit: &str, usage: Value) -> AttemptEvidence {
    AttemptEvidence {
        v: "1".to_owned(),
        attempt_id: attempt_id.to_owned(),
        harness_kind: "codex".to_owned(),
        captured: true,
        reason: None,
        base_commit: base_commit.to_owned(),
        base_revision_requested: "main".to_owned(),
        head_commit: Some(base_commit.to_owned()),
        worktree_dirty: Some(true),
        files: Vec::new(),
        patch: None,
        brief: None,
        branch: None,
        terminal_reason: json!({"code": "completed"}),
        usage,
    }
}

fn measured_usage() -> Value {
    json!({
        "harness_version": "0.9.1",
        "model": "claude-x",
        "tokens_in": {"source": "measured", "value": 1200},
        "tokens_out": {"source": "measured", "value": 340},
        "duration_ms": {"source": "measured", "value": 12500},
        "cost_usd": {"source": "not_measured", "value": null},
    })
}

// -----------------------------------------------------------------
// The translations, on their own.
// -----------------------------------------------------------------

#[test]
fn tack_evidence_becomes_an_attempt_evidence_document() {
    let head = "b".repeat(40);
    let full = attempt_evidence(
        &tack_evidence("att_1", &"a".repeat(40), measured_usage()),
        &head,
    );
    assert_eq!(
        full,
        json!({
            "contract": "attempt-evidence/v0",
            "attemptId": "att_1",
            "harness": {"name": "codex", "version": "0.9.1"},
            "baseCommit": "a".repeat(40),
            "headCommit": head,
            "generator": {"model": "claude-x"},
            "usage": {"inputTokens": 1200, "outputTokens": 340, "durationS": 12.5},
        })
    );

    // Nothing measured, nothing declared: the document says so by absence,
    // never with a zero or an empty string.
    let unmeasured = json!({
        "harness_version": "",
        "model": "",
        "tokens_in": {"source": "not_measured", "value": null},
        "tokens_out": {"source": "not_measured", "value": null},
    });
    let minimal = attempt_evidence(&tack_evidence("att_2", &"a".repeat(40), unmeasured), &head);
    assert_eq!(minimal["harness"], json!({"name": "codex"}));
    assert!(minimal.get("generator").is_none());
    assert!(minimal.get("usage").is_none());
    let null_usage = attempt_evidence(&tack_evidence("att_3", &"a".repeat(40), Value::Null), &head);
    assert!(null_usage.get("usage").is_none());
}

#[test]
fn a_brief_becomes_a_briefing_with_hints_for_what_prill_cannot_check() {
    let brief: Value = serde_json::from_slice(
        &fs::read(repo_root().join("docs/contracts/brief-v1/example.json")).expect("example"),
    )
    .expect("json");
    let document = briefing(&brief);
    assert_eq!(document["contract"], "briefing/v1");
    assert_eq!(document["id"], "0192f5a0-7c1e-7d3a-9b2e-5f6a7b8c9d0e");
    assert_eq!(
        document["objective"],
        "All criteria met and approved by the team lead"
    );
    let acceptance = document["acceptance"].as_array().expect("acceptance");
    let types: Vec<&str> = acceptance
        .iter()
        .map(|criterion| criterion["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        ["command", "test", "metric", "file", "absent", "manual"]
    );
    assert_eq!(
        acceptance[0],
        json!({
            "id": "cmd_build",
            "description": "Build succeeds",
            "type": "command",
            "command": "cargo build --release",
            "expectExitCode": 0,
        })
    );
    assert_eq!(
        acceptance[2],
        json!({
            "id": "metric_coverage",
            "description": "Code coverage above 80%",
            "type": "metric",
            "command": "cargo llvm-cov --summary-only",
            "pattern": "TOTAL.*?([0-9.]+)%",
            "op": ">=",
            "threshold": 80.0,
        })
    );
    assert_eq!(
        acceptance[5],
        json!({
            "id": "manual_review",
            "type": "manual",
            "question": "Code reviewed by maintainer",
            "rubric": "A maintainer must review and approve the changes",
        })
    );
    assert_eq!(
        document["constraints"],
        json!([
            {"type": "forbidden_paths", "globs": ["src/temp/*"]},
            {"type": "allowed_dependencies", "names": ["serde"]},
        ])
    );
    assert_eq!(
        document["riskHints"],
        json!([
            "at most 10 changed files (not enforced by prill)",
            "Keep database migrations minimal",
        ])
    );

    // A brief with nothing to say still names an item and an objective.
    let bare = briefing(&json!({"acceptance": [], "constraints": []}));
    assert_eq!(bare["id"], "tack-item");
    assert_eq!(bare["objective"], "Item");
    assert!(bare.get("constraints").is_none());
    assert!(bare.get("riskHints").is_none());
}

#[test]
fn a_metric_needs_a_command_and_a_pattern_and_a_dropped_field_is_named() {
    let metric = |op: &str, extra: Value| {
        let mut criterion = json!({
            "kind": "metric", "id": "m", "title": "Fast", "name": "latency",
            "op": op, "threshold": 5.0, "unit": "ms",
        });
        for (key, value) in extra.as_object().expect("object") {
            criterion[key] = value.clone();
        }
        briefing(&json!({"acceptance": [criterion], "constraints": []}))
    };
    let measurable = json!({"command": "bench", "pattern": "([0-9.]+)ms"});
    for (op, prill_op) in [("gte", ">="), ("lte", "<="), ("eq", "==")] {
        let document = metric(op, measurable.clone());
        assert_eq!(
            document["acceptance"],
            json!([{
                "id": "m", "description": "Fast", "type": "metric",
                "command": "bench", "pattern": "([0-9.]+)ms",
                "op": prill_op, "threshold": 5.0,
            }]),
            "{op}"
        );
        assert!(document.get("riskHints").is_none(), "{op}");
    }
    let hint =
        json!(["metric criterion m (Fast) has no command to measure it and was not checked"]);
    for extra in [
        json!({}),
        json!({"command": "bench"}),
        json!({"pattern": "([0-9.]+)ms"}),
        json!({"command": null, "pattern": null}),
    ] {
        let document = metric("gte", extra.clone());
        assert_eq!(document["acceptance"], json!([]), "{extra}");
        assert_eq!(document["riskHints"], hint, "{extra}");
    }

    let dropped = briefing(&json!({"acceptance": [
        {"kind": "command", "id": "c", "title": "Build", "run": "make", "expect_exit": 0, "cwd": "sub"},
        {"kind": "test", "id": "t", "title": "Unit", "name": "unit", "runner": "pytest"},
        {"kind": "command", "id": "c2", "title": "Build", "run": "make", "expect_exit": 0, "cwd": null},
    ], "constraints": []}));
    assert_eq!(
        dropped["riskHints"],
        json!([
            "command criterion c (Build) ran in `sub`, which prill does not carry",
            "test criterion t (Unit) names the runner `pytest`, which prill does not carry",
        ])
    );
}

#[test]
fn every_prill_fixture_translates_into_a_pack_that_round_trips() {
    let evidence = tack_evidence("att_9", &"a".repeat(40), Value::Null);
    let evidence_sha = "e".repeat(64);
    for name in [
        "m2-sections",
        "reject-failed-criterion",
        "human-review-no-sandbox",
    ] {
        let translated = pack(&prill_fixture(name), &evidence, &evidence_sha, None);
        let bytes = serde_json::to_vec_pretty(&translated).expect("serializes");
        let parsed: MergeReadinessPack = serde_json::from_slice(&bytes).expect(name);
        assert_eq!(parsed, translated, "{name}");
        assert_eq!(parsed.v, "1");
        assert_eq!(parsed.attempt_id, "att_9");
        assert_eq!(parsed.evidence_sha256, evidence_sha);
        assert_eq!(parsed.signature, SignatureSection::None);
        assert_eq!(parsed.produced_by.program, "prill");
        assert_eq!(parsed.produced_by.version, "0.1.0");
        assert!(parsed.static_analysis.is_none(), "{name}");
    }
}

#[test]
fn every_section_of_a_full_prill_pack_lands_in_its_tack_section() {
    let evidence = tack_evidence("att_9", &"a".repeat(40), Value::Null);
    let full = pack(
        &prill_fixture("m2-sections"),
        &evidence,
        &"e".repeat(64),
        Some("b".repeat(64)),
    );
    assert_eq!(full.recommendation.decision, MergeDecision::Review);
    assert_eq!(
        full.recommendation.rationale,
        "prill: human-review — review: no_sandbox — No sandbox was used."
    );
    assert_eq!(full.brief_sha256.as_deref(), Some("b".repeat(64).as_str()));
    // The risk tier is prill's own level, not the one its decision implies.
    assert_eq!(full.risk.tier, RiskTier::High);
    assert_eq!(
        full.risk.reasons,
        [
            "review: no_sandbox — No sandbox was used.",
            "flag: no_sandbox — Commands ran on the host.",
            "high: ci",
            "medium: hint touches billing",
        ]
    );
    assert_eq!(
        full.criteria,
        [Criterion {
            id: "c1".to_owned(),
            kind: "metric".to_owned(),
            status: CriterionStatus::Passed,
            evidence_ref: None,
            note: Some("exit 0, expected 0".to_owned()),
        }]
    );
    assert_eq!(
        full.verify,
        Some(VerifySection {
            command: "pytest".to_owned(),
            exit_code: 0,
            output_tail: Some("[prill] 2 passed, 1 failed, 0 errors, 0 skipped".to_owned()),
            duration_s: Some(1.5),
        })
    );
    assert_eq!(
        full.mutation,
        Some(MutationSection {
            scope: "changed_files".to_owned(),
            score: 0.5,
            killed: 1,
            survived: 1,
        })
    );
    assert_eq!(
        full.judge,
        Some(JudgeSection {
            model_family: "other-family".to_owned(),
            blind: true,
            rubric: vec![
                JudgeRubricEntry {
                    criterion_id: "c-manual".to_owned(),
                    verdict: JudgeVerdict::Pass,
                    reason: Some("The message names the file.".to_owned()),
                },
                JudgeRubricEntry {
                    criterion_id: "objective".to_owned(),
                    verdict: JudgeVerdict::Fail,
                    reason: Some("answer: no".to_owned()),
                },
            ],
        })
    );
    assert_eq!(
        full.usage,
        UsageSection {
            tokens_in: 812,
            tokens_out: 41
        }
    );

    let rejected = pack(
        &prill_fixture("reject-failed-criterion"),
        &evidence,
        &"e".repeat(64),
        None,
    );
    assert_eq!(rejected.recommendation.decision, MergeDecision::Reject);
    assert_eq!(rejected.risk.tier, RiskTier::High);
    assert_eq!(rejected.criteria[0].status, CriterionStatus::Failed);
    assert_eq!(
        rejected.criteria[0].note.as_deref(),
        Some("exit_code_mismatch")
    );
    assert!(rejected.verify.is_none());
    assert!(rejected.judge.is_none());
    assert_eq!(
        rejected.usage,
        UsageSection {
            tokens_in: 0,
            tokens_out: 0
        }
    );

    // Without a risk section the tier follows the decision.
    let host = pack(
        &prill_fixture("human-review-no-sandbox"),
        &evidence,
        &"e".repeat(64),
        None,
    );
    assert_eq!(host.recommendation.decision, MergeDecision::Review);
    assert_eq!(host.risk.tier, RiskTier::Medium);
    assert!(host.criteria.is_empty());
}

// -----------------------------------------------------------------
// The snapshot, against real git.
// -----------------------------------------------------------------

#[tokio::test]
async fn the_snapshot_commits_the_work_tree_without_the_markers_or_touching_the_index() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("ws");
    fs::create_dir_all(&path).expect("workspace");
    let base = harness_left_work(&path);
    let status_before = git(&path, &["status", "--porcelain"]);
    let index = dir.path().join("index");

    let commit = snapshot(&git_program(), &path, &index, "att_1")
        .await
        .expect("a snapshot commit");

    assert_ne!(commit, base);
    assert_eq!(git(&path, &["rev-parse", &format!("{commit}^")]), base);
    let listing = git(&path, &["ls-tree", "-r", "--name-only", &commit]);
    let mut files: Vec<&str> = listing.lines().collect();
    files.sort_unstable();
    assert_eq!(files, [".gitignore", "README.md", "new.rs"]);
    assert_eq!(
        git(&path, &["show", &format!("{commit}:README.md")]),
        "edited by the harness"
    );
    // The harness's own repository state is as it left it: HEAD, the index
    // and the work tree, markers included.
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), base);
    assert_eq!(git(&path, &["status", "--porcelain"]), status_before);
    assert!(path.join(".tack-attempt").is_file());
    assert!(!index.exists(), "the temporary index is removed");

    // Nothing left to commit: the snapshot is HEAD itself.
    git(&path, &["add", "-A"]);
    git(&path, &["commit", "--quiet", "-m", "the harness committed"]);
    let head = git(&path, &["rev-parse", "HEAD"]);
    let unchanged = snapshot(&git_program(), &path, &index, "att_1")
        .await
        .expect("a snapshot of a clean tree");
    assert_eq!(unchanged, head);
}

#[tokio::test]
async fn a_folder_without_a_commit_cannot_be_snapshotted() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("ws");
    fs::create_dir_all(&path).expect("workspace");
    let index = dir.path().join("index");
    let error = snapshot(&git_program(), &path, &index, "att_1")
        .await
        .expect_err("no repository");
    assert!(error.contains("HEAD"), "{error}");
}

// -----------------------------------------------------------------
// The whole sequence, against a fake prill.
// -----------------------------------------------------------------

/// A one-line launcher that gives the fake its knob: the verifier's own
/// environment is `PATH` only and the configured `args` come after the
/// runner's, so `env` cannot stand in front of the program.
fn launcher(dir: &Path, knob: &str) -> PathBuf {
    let fake = fake_harness_path().with_file_name("fake_prill.sh");
    let path = dir.join("prill");
    fs::write(
        &path,
        format!("#!/bin/sh\nexec env {knob} sh {} \"$@\"\n", fake.display()),
    )
    .expect("launcher");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("executable");
    }
    path
}

struct Verified {
    result: Result<Vec<Value>, VerifyFailure>,
    scratch: PathBuf,
    workspace: PathBuf,
    _dir: tempfile::TempDir,
}

async fn verify_with(
    knob: &str,
    args: Vec<String>,
    with_brief: bool,
    sign_key: Option<&str>,
) -> Verified {
    let dir = tempfile::tempdir().expect("temporary directory");
    let workspace_path = dir.path().join("ws");
    fs::create_dir_all(&workspace_path).expect("workspace");
    let base = harness_left_work(&workspace_path);
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(scratch.join("src")).expect("scratch");
    let evidence = tack_evidence("att_1", &base, measured_usage());
    fs::write(
        scratch.join("src/evidence.json"),
        serde_json::to_vec_pretty(&evidence).expect("json"),
    )
    .expect("evidence");
    if with_brief {
        fs::copy(
            repo_root().join("docs/contracts/brief-v1/example.json"),
            scratch.join("src/brief.json"),
        )
        .expect("brief");
    }
    let config = VerifyConfig {
        enabled: true,
        kind: VerifyKind::Prill,
        program: launcher(dir.path(), knob).display().to_string(),
        args,
        timeout_seconds: 60,
        sign_key: sign_key.map(str::to_owned),
    };
    let workspace = Workspace {
        attempt_id: AttemptId::new("att_1"),
        id: WorkspaceId::new("ws_att_1"),
        path: workspace_path.clone(),
        base_revision: "main".to_owned(),
    };
    let limits = ProcessLimits::new(1024 * 1024, 1024 * 1024, Duration::from_secs(60));
    let result = run_verifier(&config, &limits, &workspace, &scratch).await;
    Verified {
        result,
        scratch,
        workspace: workspace_path,
        _dir: dir,
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("file")).expect("json")
}

#[tokio::test]
async fn a_prill_pack_is_translated_and_staged() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_FIXTURE=m2-sections",
        vec!["--no-sandbox".to_owned()],
        true,
        None,
    )
    .await;
    let staged = verified.result.expect("a pack");
    assert_eq!(staged.len(), 1, "an unsigned run stages the pack alone");
    let staged = &staged[0];
    assert_eq!(staged["kind"], "mrp");
    assert_eq!(staged["name"], "mrp.json");
    assert_eq!(staged["media_type"], MRP_MEDIA_TYPE);

    let evidence = verified.scratch.join("evidence");
    let translated = fs::read(evidence.join("mrp.json")).expect("translated pack");
    assert_eq!(staged["size_bytes"], translated.len());
    assert_eq!(staged["sha256"], sha256_hex(&translated));
    let pack: MergeReadinessPack = serde_json::from_slice(&translated).expect("mrp-v1");
    assert_eq!(pack.recommendation.decision, MergeDecision::Review);
    assert_eq!(pack.attempt_id, "att_1");
    assert_eq!(
        pack.evidence_sha256,
        sha256_hex(&fs::read(evidence.join("evidence.json")).expect("evidence"))
    );
    assert_eq!(
        pack.brief_sha256.as_deref(),
        Some(sha256_hex(&fs::read(evidence.join("brief.json")).expect("brief")).as_str())
    );

    // prill was given its own documents, with the snapshot as the head.
    let prill_dir = evidence.join("prill");
    let document = read_json(&prill_dir.join("attempt-evidence.json"));
    assert_eq!(document["contract"], "attempt-evidence/v0");
    let head = document["headCommit"].as_str().expect("head");
    assert_ne!(head, document["baseCommit"].as_str().expect("base"));
    assert_eq!(
        git(&verified.workspace, &["cat-file", "-t", head]),
        "commit"
    );
    let listing = git(&verified.workspace, &["ls-tree", "-r", "--name-only", head]);
    assert!(!listing.contains(".tack-"), "{listing}");
    assert_eq!(
        read_json(&prill_dir.join("briefing.json"))["contract"],
        "briefing/v1"
    );
    // Its own pack stays beside them, untranslated.
    assert_eq!(read_json(&prill_dir.join("mrp.json"))["contract"], "mrp/v1");
    assert!(prill_dir.join("mrp.md").is_file());

    // The command line: prill's flags, then the configured args.
    let argv = fs::read_to_string(prill_dir.join("argv")).expect("argv");
    let argv: Vec<&str> = argv.lines().collect();
    assert_eq!(argv[0], "verify");
    assert_eq!(argv[1], "--repo");
    assert_eq!(Path::new(argv[2]), verified.workspace);
    assert_eq!(argv[3], "--evidence");
    assert_eq!(Path::new(argv[4]), prill_dir.join("attempt-evidence.json"));
    assert_eq!(argv[5], "--brief");
    assert_eq!(Path::new(argv[6]), prill_dir.join("briefing.json"));
    assert_eq!(argv[7], "--out");
    assert_eq!(Path::new(argv[8]), prill_dir);
    assert_eq!(&argv[9..], ["--json", "--no-sandbox"]);
}

#[tokio::test]
async fn without_a_brief_prill_gets_none() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_FIXTURE=human-review-no-sandbox",
        Vec::new(),
        false,
        None,
    )
    .await;
    verified.result.expect("a pack");
    let prill_dir = verified.scratch.join("evidence/prill");
    assert!(!prill_dir.join("briefing.json").exists());
    let argv = fs::read_to_string(prill_dir.join("argv")).expect("argv");
    assert!(!argv.contains("--brief"), "{argv}");
    let pack: MergeReadinessPack =
        serde_json::from_slice(&fs::read(verified.scratch.join("evidence/mrp.json")).unwrap())
            .expect("mrp-v1");
    assert!(pack.brief_sha256.is_none());
}

#[tokio::test]
async fn a_reject_pack_is_still_a_staged_pack() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_FIXTURE=reject-failed-criterion",
        Vec::new(),
        false,
        None,
    )
    .await;
    let staged = verified
        .result
        .expect("prill's exit 1 is a decision, not a failure");
    assert_eq!(staged[0]["kind"], "mrp");
    let pack: MergeReadinessPack =
        serde_json::from_slice(&fs::read(verified.scratch.join("evidence/mrp.json")).unwrap())
            .expect("mrp-v1");
    assert_eq!(pack.recommendation.decision, MergeDecision::Reject);
    assert_eq!(pack.risk.tier, RiskTier::High);
}

#[tokio::test]
async fn a_refusal_is_a_failure_naming_prills_error_code() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_ERROR=invalid_brief",
        Vec::new(),
        true,
        None,
    )
    .await;
    let error = verified.result.expect_err("exit 2 is a failure");
    assert_eq!(error.exit_code, Some(2));
    assert_eq!(
        error.reason,
        "prill refused: invalid_brief: fake prill refusing on request"
    );
    assert!(!verified.scratch.join("evidence/mrp.json").exists());
}

#[tokio::test]
async fn a_workspace_that_is_not_a_repository_cannot_be_verified() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let workspace_path = dir.path().join("ws");
    fs::create_dir_all(&workspace_path).expect("workspace");
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(scratch.join("src")).expect("scratch");
    let evidence = tack_evidence("att_1", &"a".repeat(40), Value::Null);
    fs::write(
        scratch.join("src/evidence.json"),
        serde_json::to_vec(&evidence).expect("json"),
    )
    .expect("evidence");
    let config = VerifyConfig {
        enabled: true,
        kind: VerifyKind::Prill,
        program: launcher(dir.path(), "TACK_FAKE_PRILL_FIXTURE=m2-sections")
            .display()
            .to_string(),
        args: Vec::new(),
        timeout_seconds: 60,
        sign_key: None,
    };
    let workspace = Workspace {
        attempt_id: AttemptId::new("att_1"),
        id: WorkspaceId::new("ws_att_1"),
        path: workspace_path,
        base_revision: "main".to_owned(),
    };
    let limits = ProcessLimits::new(1024 * 1024, 1024 * 1024, Duration::from_secs(60));
    let error = run_verifier(&config, &limits, &workspace, &scratch)
        .await
        .expect_err("no snapshot, no verification");
    assert_eq!(error.exit_code, None);
    assert!(
        error
            .reason
            .starts_with("the workspace could not be snapshotted"),
        "{}",
        error.reason
    );
}

/// A signed run: the key path goes to prill after `--json`, the envelope is a
/// second artifact, and the pack names the envelope's hash.
#[tokio::test]
async fn a_signed_run_stages_the_envelope_and_names_it_in_the_pack() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_SIGNED=1",
        Vec::new(),
        false,
        Some("/keys/prill.key"),
    )
    .await;
    let staged = verified.result.expect("a signed pack");
    assert_eq!(staged.len(), 2);
    assert_eq!(staged[0]["kind"], "mrp");
    assert_eq!(staged[1]["kind"], "mrp_attestation");
    assert_eq!(staged[1]["name"], "mrp.intoto.json");
    assert_eq!(staged[1]["media_type"], "application/vnd.in-toto+json");
    let evidence = verified.scratch.join("evidence");
    let envelope = fs::read(evidence.join("prill/mrp.intoto.json")).expect("envelope");
    assert_eq!(staged[1]["sha256"], sha256_hex(&envelope));
    let pack: MergeReadinessPack =
        serde_json::from_slice(&fs::read(evidence.join("mrp.json")).unwrap()).expect("mrp-v1");
    assert_eq!(
        pack.signature,
        SignatureSection::InToto {
            statement_sha256: sha256_hex(&envelope)
        }
    );
    let argv = fs::read_to_string(evidence.join("prill/argv")).expect("argv");
    let argv: Vec<&str> = argv.lines().collect();
    assert_eq!(
        &argv[argv.len() - 3..],
        ["--json", "--sign", "/keys/prill.key"]
    );
}

/// A refusal to sign fails the verification with prill's reason; the runner
/// never runs prill a second time without the key.
#[tokio::test]
async fn a_signing_refusal_fails_the_verification_and_is_not_retried_unsigned() {
    let verified = verify_with(
        "TACK_FAKE_PRILL_ERROR=signing_unavailable",
        Vec::new(),
        false,
        Some("/keys/prill.key"),
    )
    .await;
    let error = verified.result.expect_err("a refusal");
    assert!(
        error.reason.contains("signing_unavailable"),
        "{}",
        error.reason
    );
    assert!(!verified.scratch.join("evidence/mrp.json").exists());
    let argv = fs::read_to_string(verified.scratch.join("evidence/prill/argv")).expect("argv");
    assert!(argv.contains("--sign"), "{argv}");
}
