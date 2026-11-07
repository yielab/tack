//! `promote` against a stand-in for the board: the answers a server gives for one rejected
//! attempt, and a real repository at its base commit. Only the server is stood in for.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tack_evals::world::git;

/// Serves `answers` (path with query to JSON) until the test ends; anything else is a 404.
fn serve(answers: HashMap<String, Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let (status, body) = match answers.get(path) {
                Some(body) => ("200 OK", body.to_string()),
                None => ("404 Not Found", "{}".to_owned()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
}

/// A shop whose code, comments and paths all say who it belongs to.
struct Shop {
    _root: tempfile::TempDir,
    repo: PathBuf,
    base: String,
    out: PathBuf,
}

fn shop() -> Shop {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("acme-shop");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    std::fs::write(
        repo.join("src/cart.py"),
        format!(
            "# Acme Shop cart. Config lives in {}/conf\nKEY = \"sk-live-abcdef123456\"\n\
             def total(a, b):\n    return float(a) + float(b)\n",
            repo.display()
        ),
    )
    .unwrap();
    std::fs::write(repo.join("README.txt"), "Acme Shop (see /home/ana/Sites)\n").unwrap();
    git(&repo, &["add", "-A"]).unwrap();
    git(&repo, &["commit", "-q", "-m", "Acme Shop first"]).unwrap();
    let base = git(&repo, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_owned();
    let out = root.path().join("candidates");
    Shop {
        _root: root,
        repo,
        base,
        out,
    }
}

fn answers(shop: &Shop, harness: &str, review: Value) -> HashMap<String, Value> {
    let item = "11111111-1111-1111-1111-111111111111";
    HashMap::from([
        (
            "/api/executions?limit=2000".to_owned(),
            json!({"data": [{"request_id": "req_1"}]}),
        ),
        (
            "/api/executions/req_1/attempts".to_owned(),
            json!({"data": [{
                "attempt_id": "att_1",
                "attempt_number": 1,
                "base_revision": shop.base,
                "review": review,
            }]}),
        ),
        (
            "/api/executions/req_1".to_owned(),
            json!({"request_id": "req_1", "item_id": item, "requested_harness_kind": harness}),
        ),
        (
            format!("/api/items/{item}"),
            json!({
                "project_id": "22222222-2222-2222-2222-222222222222",
                "item_type": "bug",
                "title": "Acme Shop adds with floats",
                "description": "Totals drift. See /home/ana/Sites/acme-shop/src/cart.py",
                "priority": "high",
            }),
        ),
        (
            format!("/api/items/{item}/brief"),
            json!({
                "item_id": item,
                "created_at": "2026-11-01T00:00:00Z",
                "acceptance": [
                    {"kind": "manual", "id": "a1", "title": "Decimal", "text": "Decimal"},
                    {"kind": "test", "id": "a2", "title": "Cart", "name": "pytest tests/test_cart.py"},
                ],
                "constraints": [{"kind": "note", "text": "Acme Shop keeps its public API"}],
                "definition_of_done": "Totals are exact.",
                "risk": "low",
            }),
        ),
        (
            "/api/projects/22222222-2222-2222-2222-222222222222".to_owned(),
            json!({"name": "Acme Shop", "repository": shop.repo}),
        ),
    ])
}

fn rejected(shop: &Shop) -> Value {
    json!({
        "verdict": "rejected",
        "reason": format!(
            "Still floats in {}/src/cart.py; Acme Shop bills in cents. Found sk-live-zzzzzzzz1234 \
             in the diff, read from /home/ana/.config, twice.",
            shop.repo.display()
        ),
        "reviewed_by": "user:ana",
    })
}

fn promote(server: &str, shop: &Shop, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .args(["promote", "att_1", "--server", server])
        .args(["--out".as_ref(), shop.out.as_os_str()])
        .args(extra)
        .env_remove("TACK_API_TOKEN")
        .output()
        .expect("run tack-evals")
}

fn check(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .arg("check-solutions")
        .arg(path)
        .output()
        .expect("run tack-evals")
}

/// Every file under `dir` as text, bundle excluded.
fn texts(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            found.extend(texts(&path));
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            found.push((path, text));
        }
    }
    found
}

fn assert_nothing_real(dir: &Path) {
    for (path, text) in texts(dir) {
        let lower = text.to_lowercase();
        for real in ["sk-", "/home/", "acme"] {
            assert!(
                !lower.contains(real),
                "{real:?} left in {}:\n{text}",
                path.display()
            );
        }
    }
}

#[test]
fn a_rejected_attempt_becomes_a_candidate_with_nothing_real_in_it() {
    let shop = shop();
    let server = serve(answers(&shop, "claude-code", rejected(&shop)));

    let output = promote(&server, &shop, &[]);
    assert!(output.status.success(), "{output:?}");

    let dir = shop.out.join("att_1");
    for file in [
        "candidate.json",
        "item.json",
        "brief.json",
        "reason.txt",
        "repo.bundle",
    ] {
        assert!(dir.join(file).is_file(), "{file} is missing");
    }
    assert!(!dir.join("docket.yaml").exists(), "not a docket run");
    assert_nothing_real(&dir);

    let reason = std::fs::read_to_string(dir.join("reason.txt")).unwrap();
    assert!(
        reason.contains("Still floats in /repo/src/cart.py"),
        "{reason}"
    );
    assert!(
        reason.contains("My SaaS bills in cents") && reason.contains("read from ~, twice"),
        "{reason}"
    );

    // The bundle is a repository at the base commit with the same code, anonymised too.
    let clone = shop.out.join("unbundled");
    git(
        &shop.out,
        &[
            "clone",
            "-q",
            "-b",
            "base",
            &dir.join("repo.bundle").display().to_string(),
            &clone.display().to_string(),
        ],
    )
    .unwrap();
    let cart = std::fs::read_to_string(clone.join("src/cart.py")).unwrap();
    assert!(
        cart.contains("return float(a) + float(b)"),
        "the code is kept: {cart}"
    );
    assert_nothing_real(&clone);
    let history = git(&clone, &["log", "--format=%an %s"]).unwrap();
    assert_eq!(
        history.lines().count(),
        1,
        "none of the real history travels: {history}"
    );
}

#[test]
fn a_docket_run_carries_docket_s_task_fields() {
    let shop = shop();
    let server = serve(answers(&shop, "docket", rejected(&shop)));

    let output = promote(&server, &shop, &[]);
    assert!(output.status.success(), "{output:?}");

    let fields = std::fs::read_to_string(shop.out.join("att_1/docket.yaml")).unwrap();
    for wanted in [
        "objective: My SaaS adds with floats",
        "doneWhen: Totals are exact.",
        "pytest tests/test_cart.py",
        "My SaaS keeps its public API",
        "kind: bug",
    ] {
        assert!(fields.contains(wanted), "{wanted:?} not in\n{fields}");
    }
    assert_nothing_real(&shop.out.join("att_1"));
}

#[test]
fn work_nobody_refused_is_not_promoted_unless_a_revert_says_why() {
    let shop = shop();
    let server = serve(answers(&shop, "claude-code", Value::Null));

    let output = promote(&server, &shop, &[]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--reason"));
    assert!(!shop.out.join("att_1").exists());

    let output = promote(
        &server,
        &shop,
        &["--reason", "Reverted: broke Acme Shop's checkout"],
    );
    assert!(output.status.success(), "{output:?}");
    let candidate: Value =
        serde_json::from_slice(&std::fs::read(shop.out.join("att_1/candidate.json")).unwrap())
            .unwrap();
    assert_eq!(candidate["verdict"], "reverted");
    assert_nothing_real(&shop.out.join("att_1"));
}

#[test]
fn a_candidate_is_never_overwritten() {
    let shop = shop();
    let server = serve(answers(&shop, "claude-code", rejected(&shop)));
    assert!(promote(&server, &shop, &[]).status.success());
    std::fs::create_dir(shop.out.join("att_1/hidden")).unwrap();

    let again = promote(&server, &shop, &[]);
    assert_eq!(again.status.code(), Some(2), "{again:?}");
    assert!(
        shop.out.join("att_1/hidden").is_dir(),
        "the person's work stays"
    );
}

#[test]
fn a_secret_from_the_environment_is_taken_out_wherever_it_is() {
    let shop = shop();
    let mut all = answers(&shop, "claude-code", rejected(&shop));
    all.insert(
        "/api/executions/req_1/attempts".to_owned(),
        json!({"data": [{
            "attempt_id": "att_1", "attempt_number": 1, "base_revision": shop.base,
            "review": {"verdict": "rejected", "reason": "the password was zebra-umbrella-42"},
        }]}),
    );
    let server = serve(all);
    let output = Command::new(env!("CARGO_BIN_EXE_tack-evals"))
        .args([
            "promote",
            "att_1",
            "--server",
            &server,
            "--env-secret",
            "EVAL_FIXTURE_SECRET",
        ])
        .args(["--out".as_ref(), shop.out.as_os_str()])
        .env("EVAL_FIXTURE_SECRET", "zebra-umbrella-42")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let reason = std::fs::read_to_string(shop.out.join("att_1/reason.txt")).unwrap();
    assert_eq!(reason.trim(), "the password was [REDACTED]");
}

#[test]
fn check_solutions_refuses_a_candidate_until_it_has_its_exam_and_solution() {
    let shop = shop();
    let server = serve(answers(&shop, "claude-code", rejected(&shop)));
    assert!(promote(&server, &shop, &[]).status.success());

    let output = check(&shop.out);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("REFUSED  att_1"), "{said}");
    for lacks in ["hidden/", "solution/", "task.yaml"] {
        assert!(said.contains(lacks), "{lacks} not named: {said}");
    }

    // Pointing at the candidate itself says the same.
    let direct = check(&shop.out.join("att_1"));
    assert_eq!(direct.status.code(), Some(1), "{direct:?}");

    // What a person has written is no longer said to be missing.
    let dir = shop.out.join("att_1");
    std::fs::create_dir_all(dir.join("hidden")).unwrap();
    std::fs::write(dir.join("hidden/check.sh"), "true\n").unwrap();
    let still = check(&dir);
    let said = String::from_utf8_lossy(&still.stdout);
    assert_eq!(still.status.code(), Some(1), "{said}");
    assert!(!said.contains("hidden/"), "{said}");
    assert!(said.contains("solution/"), "{said}");
}
