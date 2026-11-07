//! `promote`: from a run a person rejected (or reverted) to a candidate for the `capability`
//! suite.
//!
//! A candidate is what a task needs from reality and nothing a person has to invent: the
//! repository at the base commit, the item and its brief, why the work was refused and where the
//! evidence lives. It holds real names, so it is written anonymised and kept out of git. It is
//! not a task: the hidden exam and the reference solution are written by a person, because a task
//! nobody lived measures what its author imagined. [`refusals`] is how `check-solutions` says so.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::api::Api;
use crate::exam::export_revision;
use crate::world::git;

/// Marks a directory as a candidate; a task has `task.yaml` instead.
pub const CANDIDATE_FILE: &str = "candidate.json";
/// The bundle's one branch.
const BUNDLE_BRANCH: &str = "base";
const DOCKET_FILE: &str = "docket.yaml";
const REDACTED: &str = "[REDACTED]";
/// What a real project name becomes; a person writes the final wording when the candidate enters
/// the repository.
const PLACEHOLDER: &str = "My SaaS";
/// Names and secrets shorter than this would damage ordinary words, so they are not replaced.
const SHORTEST_REPLACED: usize = 4;
/// A token prefix with fewer characters after it is not a token (`sk-` in a sentence).
const SHORTEST_TOKEN_BODY: usize = 4;

/// Words that start a token, kept at a word boundary so `task-1` is not an API key.
const TOKEN_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "AKIA",
    "Bearer ",
];
const HOME_ROOTS: &[&str] = &["/home/", "/Users/", "/root/"];

/// Takes the real from a text: known secrets, token shapes, private keys, home paths, the
/// repository's location and the project's names.
pub struct Anonymiser {
    secrets: Vec<String>,
    roots: Vec<String>,
    /// Lowercase needle and what replaces it, longest needle first.
    names: Vec<(String, String)>,
    /// What could not be replaced safely, for the person to read the candidate against.
    pub warnings: Vec<String>,
}

impl Anonymiser {
    pub fn new(secrets: &[String], roots: &[String], names: &[String]) -> Self {
        let mut warnings = Vec::new();
        let secrets = secrets
            .iter()
            .filter(|secret| {
                let long_enough = secret.len() >= SHORTEST_REPLACED;
                if !long_enough {
                    warnings.push("a secret is too short to be replaced by value".to_owned());
                }
                long_enough
            })
            .cloned()
            .collect();
        let mut roots: Vec<String> = roots
            .iter()
            .map(|root| root.trim_end_matches('/').to_owned())
            .filter(|root| !root.is_empty())
            .collect();
        roots.sort_by_key(|root| std::cmp::Reverse(root.len()));

        let mut needles: Vec<(String, String)> = Vec::new();
        for name in names {
            let words: Vec<String> = name
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect();
            if words.join("").len() < SHORTEST_REPLACED {
                warnings.push(format!(
                    "the name {name:?} is too short to be replaced; read the candidate for it"
                ));
                continue;
            }
            for (separator, replacement) in [
                (" ", PLACEHOLDER),
                ("-", "my-saas"),
                ("_", "my_saas"),
                ("", "mysaas"),
            ] {
                let needle = words.join(separator);
                if replacement.to_lowercase().contains(&needle) {
                    warnings.push(format!(
                        "the name {name:?} is part of the placeholder and is left as it is"
                    ));
                } else if !needles.iter().any(|(seen, _)| *seen == needle) {
                    needles.push((needle, replacement.to_owned()));
                }
            }
        }
        needles.sort_by_key(|(needle, _)| std::cmp::Reverse(needle.len()));
        Self {
            secrets,
            roots,
            names: needles,
            warnings,
        }
    }

    pub fn text(&self, input: &str) -> String {
        let mut out = input.to_owned();
        for secret in &self.secrets {
            out = out.replace(secret, REDACTED);
        }
        // The repository's own location first: what is left of a path inside it is still useful.
        for root in &self.roots {
            out = out.replace(root, "/repo");
        }
        out = redact_private_keys(&out);
        out = redact_tokens(&out);
        out = redact_home_paths(&out);
        for (needle, replacement) in &self.names {
            out = replace_ignoring_case(&out, needle, replacement);
        }
        out
    }

    /// Every string in `value`, anonymised.
    pub fn value(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.value(v)).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, v)| (key.clone(), self.value(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Whether anonymising `text` would still change it: something real is left.
    pub fn leaves_something(&self, text: &str) -> bool {
        self.text(text) != text
    }
}

fn redact_tokens(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut skip_to) = (0, 0);
    for (at, _) in input.char_indices() {
        if at < skip_to {
            continue;
        }
        let tail = &input[at..];
        let Some(prefix) = TOKEN_PREFIXES
            .iter()
            .find(|prefix| tail.starts_with(**prefix))
        else {
            continue;
        };
        if input[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let body = tail[prefix.len()..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            .count();
        if body < SHORTEST_TOKEN_BODY {
            continue;
        }
        out.push_str(&input[copied..at]);
        if *prefix == "Bearer " {
            out.push_str(prefix);
        }
        out.push_str(REDACTED);
        skip_to = at + prefix.len() + body;
        copied = skip_to;
    }
    out.push_str(&input[copied..]);
    out
}

fn redact_private_keys(input: &str) -> String {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find(BEGIN) {
        let header_end = rest[start..].find('\n').map_or(rest.len(), |n| start + n);
        if !rest[start..header_end].contains("PRIVATE KEY") {
            out.push_str(&rest[..start + BEGIN.len()]);
            rest = &rest[start + BEGIN.len()..];
            continue;
        }
        out.push_str(&rest[..start]);
        out.push_str("[REDACTED KEY]");
        rest = match rest[start..].find(END) {
            Some(end) => {
                let after = start + end + END.len();
                rest[after..]
                    .find("-----")
                    .map_or("", |close| &rest[after + close + 5..])
            }
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// A home directory and everything after it becomes `~`. Any `/home/` goes, even inside a
/// relative path: a candidate that keeps one is a candidate that might keep a user name.
fn redact_home_paths(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    'scan: loop {
        let next = HOME_ROOTS.iter().filter_map(|root| rest.find(root)).min();
        let Some(at) = next else { break 'scan };
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let length = tail
            .find(|c: char| c.is_whitespace() || "\"'`)]}>,;|".contains(c))
            .unwrap_or(tail.len());
        out.push('~');
        rest = &tail[length..];
    }
    out.push_str(rest);
    out
}

fn replace_ignoring_case(haystack: &str, needle_lowercase: &str, with: &str) -> String {
    // ASCII lowering keeps every byte offset, so a match in one is a match in the other.
    let lowered = haystack.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut at = 0;
    while let Some(found) = lowered[at..].find(needle_lowercase) {
        out.push_str(&haystack[at..at + found]);
        out.push_str(with);
        at += found + needle_lowercase.len();
    }
    out.push_str(&haystack[at..]);
    out
}

#[derive(Debug, Clone, Default)]
pub struct PromoteOptions {
    /// Where candidates go (`evals/candidates`).
    pub out: PathBuf,
    /// The run the attempt belongs to; without it the server's recent runs are searched.
    pub run: Option<String>,
    /// A clone to read the base commit from, when the project's repository is not a local folder.
    pub repo: Option<PathBuf>,
    /// Why the work was refused, for an attempt the board holds no rejection of (a revert).
    pub reason: Option<String>,
    /// More names to take out, beside the project's and the repository's.
    pub names: Vec<String>,
    /// Values to take out wherever they appear (read from the environment by the caller).
    pub secrets: Vec<String>,
}

#[derive(Debug)]
pub struct Promoted {
    pub dir: PathBuf,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

/// Writes `options.out/<attempt id>/` from the attempt the board holds.
pub fn promote(api: &Api, attempt_id: &str, options: &PromoteOptions) -> Result<Promoted> {
    if attempt_id.is_empty()
        || !attempt_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("{attempt_id:?} is not an attempt id");
    }
    let dir = options.out.join(attempt_id);
    if dir.exists() {
        // A person may already be writing the exam in there.
        bail!(
            "{} already exists; remove it to promote again",
            dir.display()
        );
    }

    let (request_id, attempt) = find_attempt(api, attempt_id, options.run.as_deref())?;
    let number = attempt["attempt_number"]
        .as_u64()
        .context("the attempt has no number")?;
    let (verdict, reason) = refusal_of(&attempt, options.reason.as_deref())?;
    let base_revision = attempt["base_revision"]
        .as_str()
        .context("the attempt never recorded the commit it started from")?;

    let execution = api.get(&format!("/api/executions/{request_id}"))?;
    let item_id = execution["item_id"]
        .as_str()
        .context("the run has no item")?;
    let item = api.get(&format!("/api/items/{item_id}"))?;
    let brief = api.get_optional(&format!("/api/items/{item_id}/brief"))?;
    let project = api.get(&format!(
        "/api/projects/{}",
        item["project_id"]
            .as_str()
            .context("the item has no project")?
    ))?;
    let repository = project["repository"].as_str().map(str::to_owned);
    let source = options.repo.clone().or_else(|| {
        repository
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_dir())
    });
    let Some(source) = source else {
        bail!("the project's repository is not a folder on this machine: pass --repo <a clone>");
    };

    let mut roots = vec![source.display().to_string()];
    roots.extend(repository.iter().cloned());
    let mut names = options.names.clone();
    names.extend(project["name"].as_str().map(str::to_owned));
    names.extend(
        repository
            .iter()
            .chain(std::iter::once(&source.display().to_string()))
            .filter_map(|r| {
                r.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .map(str::to_owned)
            })
            .map(|last| last.trim_end_matches(".git").to_owned()),
    );
    let anon = Anonymiser::new(&options.secrets, &roots, &names);

    std::fs::create_dir_all(&options.out)?;
    let partial = options.out.join(format!(".{attempt_id}.partial"));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial)?;
    let built = build(
        &anon,
        &partial,
        &Facts {
            attempt_id,
            request_id: &request_id,
            number,
            verdict,
            reason: &reason,
            base_revision,
            execution: &execution,
            item: &item,
            brief: brief.as_ref(),
            source: &source,
        },
    );
    let files = match built {
        Ok(files) => files,
        Err(failure) => {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(failure);
        }
    };
    std::fs::rename(&partial, &dir)?;
    Ok(Promoted {
        dir,
        files,
        warnings: anon.warnings,
    })
}

struct Facts<'a> {
    attempt_id: &'a str,
    request_id: &'a str,
    number: u64,
    verdict: &'a str,
    reason: &'a str,
    base_revision: &'a str,
    execution: &'a Value,
    item: &'a Value,
    brief: Option<&'a Value>,
    source: &'a Path,
}

/// The attempt, and the run it belongs to.
fn find_attempt(api: &Api, attempt_id: &str, run: Option<&str>) -> Result<(String, Value)> {
    let requests: Vec<String> = match run {
        Some(run) => vec![run.to_owned()],
        None => api.get("/api/executions?limit=2000")?["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|run| run["request_id"].as_str().map(str::to_owned))
            .collect(),
    };
    for request_id in requests {
        let attempts = api.get(&format!("/api/executions/{request_id}/attempts"))?;
        if let Some(found) = attempts["data"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|attempt| attempt["attempt_id"] == attempt_id)
        {
            return Ok((request_id, found.clone()));
        }
    }
    bail!("no attempt {attempt_id} among the server's runs; name its run with --run")
}

/// Why the work was refused: a person's rejection on the board, else the reason given for a
/// revert (which the board has no record of).
fn refusal_of<'a>(attempt: &'a Value, given: Option<&'a str>) -> Result<(&'static str, String)> {
    let review = &attempt["review"];
    match (review["verdict"].as_str(), given) {
        (Some("rejected"), _) => Ok((
            "rejected",
            review["reason"].as_str().unwrap_or_default().to_owned(),
        )),
        (_, Some(reason)) if !reason.trim().is_empty() => Ok(("reverted", reason.to_owned())),
        (Some(other), _) => bail!(
            "the attempt was {other}: only refused work is promoted; for work that was merged \
             and later reverted, say why with --reason"
        ),
        (None, _) => bail!(
            "the attempt has no review: only refused work is promoted; for work that was merged \
             and later reverted, say why with --reason"
        ),
    }
}

/// Writes every file of the candidate into `dir` and checks that nothing real is left in the
/// text ones. Returns the file names.
fn build(anon: &Anonymiser, dir: &Path, facts: &Facts) -> Result<Vec<String>> {
    let harness = facts.execution["requested_harness_kind"]
        .as_str()
        .map(str::to_owned);
    let item = json!({
        "type": facts.item["item_type"],
        "title": facts.item["title"],
        "description": facts.item["description"],
        "priority": facts.item["priority"],
    });
    let write_json = |name: &str, value: &Value| -> Result<()> {
        std::fs::write(
            dir.join(name),
            serde_json::to_vec_pretty(&anon.value(value))?,
        )
        .with_context(|| format!("write {name}"))
    };
    let mut files = Vec::new();

    write_json("item.json", &item)?;
    files.push("item.json");
    if let Some(brief) = facts.brief {
        write_json("brief.json", &brief_of(brief))?;
        files.push("brief.json");
    }
    std::fs::write(
        dir.join("reason.txt"),
        format!("{}\n", anon.text(facts.reason)),
    )?;
    files.push("reason.txt");

    if harness.as_deref() == Some("docket") {
        let fields = docket_fields(facts.item, facts.brief);
        std::fs::write(
            dir.join(DOCKET_FILE),
            serde_saphyr::to_string(&anon.value(&fields)).context("write the docket fields")?,
        )?;
        files.push(DOCKET_FILE);
    }

    let run = format!("/api/executions/{}", facts.request_id);
    let attempt = format!("{run}/attempts/{}", facts.number);
    write_json(
        CANDIDATE_FILE,
        &json!({
            "schema": 1,
            "id": facts.attempt_id,
            "verdict": facts.verdict,
            "harness": harness,
            // Where the real run is, on the server it came from; nothing of it is copied.
            "evidence": {
                "attempt": attempt,
                "events": format!("{attempt}/events"),
                "artifacts": format!("{attempt}/artifacts"),
            },
            "next": "write hidden/ (the exam) and solution/ (the reference), unbundle repo.bundle \
                     into repo/ and add task.yaml; then check-solutions takes it as a task",
        }),
    )?;
    files.push(CANDIDATE_FILE);

    let scratch = tempfile::tempdir().context("scratch directory")?;
    bundle_base(anon, facts, scratch.path(), &dir.join("repo.bundle"))?;
    files.push("repo.bundle");

    for name in &files {
        if *name == "repo.bundle" {
            continue;
        }
        let text = std::fs::read_to_string(dir.join(name))?;
        if anon.leaves_something(&text) {
            bail!("{name} still holds something real after anonymising; no candidate was written");
        }
    }
    Ok(files.into_iter().map(str::to_owned).collect())
}

/// The brief as the board holds it, without its ids and dates.
fn brief_of(brief: &Value) -> Value {
    let mut kept = Map::new();
    for key in ["acceptance", "constraints", "definition_of_done", "risk"] {
        if let Some(value) = brief.get(key).filter(|v| !v.is_null()) {
            kept.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(kept)
}

/// The fields Tack sends docket as a task's `task` (objective, done-when, acceptance,
/// constraints, kind, risk), so docket's own `promote` can read the candidate.
fn docket_fields(item: &Value, brief: Option<&Value>) -> Value {
    let words = |key: &str, each: fn(&Value) -> Option<String>| -> Vec<String> {
        brief
            .and_then(|brief| brief.get(key)?.as_array())
            .map(|entries| entries.iter().filter_map(each).collect())
            .unwrap_or_default()
    };
    let mut fields = Map::new();
    fields.insert("objective".into(), item["title"].clone());
    if let Some(done) = brief
        .and_then(|brief| brief["definition_of_done"].as_str())
        .filter(|done| !done.trim().is_empty())
    {
        fields.insert("doneWhen".into(), json!(done));
    }
    for (key, entries) in [
        ("acceptance", words("acceptance", criterion)),
        ("constraints", words("constraints", constraint)),
    ] {
        if !entries.is_empty() {
            fields.insert(key.into(), json!(entries));
        }
    }
    if let Some(risk) = brief.and_then(|brief| brief["risk"].as_str()) {
        fields.insert("risk".into(), json!(risk));
    }
    if let Some(kind) = item["item_type"].as_str() {
        fields.insert("kind".into(), json!(kind));
    }
    Value::Object(fields)
}

fn criterion(entry: &Value) -> Option<String> {
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    let title = text("title").unwrap_or_default();
    let detail = match text("kind")? {
        "command" => format!(
            "`{}` exits {}",
            text("run")?,
            entry["expect_exit"].as_u64().unwrap_or(0)
        ),
        "test" => format!("`{}` passes", text("name")?),
        "file" => format!("{} exists", text("path")?),
        "absent" => format!("{} does not exist", text("path")?),
        "manual" => text("text")?.to_owned(),
        _ => return (!title.is_empty()).then(|| title.to_owned()),
    };
    Some(if title.is_empty() || title == detail {
        detail
    } else {
        format!("{title}: {detail}")
    })
}

fn constraint(entry: &Value) -> Option<String> {
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    Some(match text("kind")? {
        "forbidden_path" => format!("Do not change files matching {}", text("glob")?),
        "allowed_dependency" => format!("The dependency {} is allowed", text("name")?),
        "max_changed_files" => format!("Change at most {} files", entry["n"].as_u64()?),
        "note" => text("text")?.to_owned(),
        _ => return None,
    })
}

/// The tree at the base commit, anonymised, as the single commit of a bundle. The project's own
/// history would carry every name and secret it ever held, so none of it travels.
fn bundle_base(anon: &Anonymiser, facts: &Facts, scratch: &Path, bundle: &Path) -> Result<()> {
    let tree = scratch.join("tree");
    export_revision(facts.source, facts.base_revision, &tree)?;
    anonymise_tree(anon, &tree)?;
    git(&tree, &["init", "-q", "-b", BUNDLE_BRANCH])?;
    git(&tree, &["add", "-A", "-f"])?;
    git(
        &tree,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Base of the candidate",
        ],
    )?;
    let bundle = std::path::absolute(bundle)?;
    git(
        &tree,
        &[
            "bundle",
            "create",
            "-q",
            &bundle.display().to_string(),
            BUNDLE_BRANCH,
        ],
    )?;
    Ok(())
}

/// Anonymises every text file under `dir`; binary files and links are left as they are.
fn anonymise_tree(anon: &Anonymiser, dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let kind = std::fs::symlink_metadata(&path)?.file_type();
        if kind.is_dir() {
            anonymise_tree(anon, &path)?;
        } else if kind.is_file() {
            let bytes = std::fs::read(&path)?;
            if let Ok(text) = String::from_utf8(bytes)
                && !text.contains('\0')
                && anon.leaves_something(&text)
            {
                std::fs::write(&path, anon.text(&text))?;
            }
        }
    }
    Ok(())
}

/// A candidate that `check-solutions` was pointed at, and what it still lacks to be a task.
#[derive(Debug)]
pub struct Refusal {
    pub id: String,
    pub missing: Vec<&'static str>,
}

impl Refusal {
    pub fn message(&self) -> String {
        format!(
            "{}: a candidate, not a task; it still lacks {}. A person writes the hidden exam and \
             the reference solution (see evals/README.md)",
            self.id,
            self.missing.join(", ")
        )
    }
}

/// Every candidate under `paths` that is not a task yet: a directory with `candidate.json` and no
/// `task.yaml`.
pub fn refusals(paths: &[PathBuf]) -> Result<Vec<Refusal>> {
    let mut found = Vec::new();
    for path in paths {
        collect_refusals(path, &mut found)?;
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(found)
}

fn collect_refusals(dir: &Path, found: &mut Vec<Refusal>) -> Result<()> {
    if dir.join("task.yaml").is_file() || !dir.is_dir() {
        return Ok(());
    }
    if dir.join(CANDIDATE_FILE).is_file() {
        let filled = |name: &str| {
            std::fs::read_dir(dir.join(name)).is_ok_and(|mut entries| entries.next().is_some())
        };
        found.push(Refusal {
            id: dir
                .file_name()
                .map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into()),
            missing: ["hidden/", "solution/", "repo/"]
                .into_iter()
                .filter(|name| !filled(name))
                .chain(["task.yaml"])
                .collect(),
        });
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        collect_refusals(&entry?.path(), found)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anon() -> Anonymiser {
        Anonymiser::new(
            &["hunter2-the-secret".to_owned()],
            &["/home/ana/Sites/acme-shop".to_owned()],
            &["Acme Shop".to_owned()],
        )
    }

    #[test]
    fn a_path_inside_the_repository_keeps_its_relative_part() {
        assert_eq!(
            anon().text("see /home/ana/Sites/acme-shop/src/cart.py"),
            "see /repo/src/cart.py"
        );
    }

    #[test]
    fn any_other_home_path_goes_whole() {
        assert_eq!(anon().text("cd /home/ana/.ssh && ls"), "cd ~ && ls");
        assert_eq!(anon().text("in \"/Users/bob/x\"."), "in \"~\".");
    }

    #[test]
    fn tokens_secrets_and_keys_are_taken_out() {
        let text = "key sk-ant-abcdef123456 and ghp_abcdefgh1234, Bearer abcd1234efgh, \
                    value hunter2-the-secret\n-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n\
                    -----END OPENSSH PRIVATE KEY-----\nafter";
        let clean = anon().text(text);
        for gone in ["sk-", "ghp_", "abcd1234efgh", "hunter2", "AAAA", "BEGIN"] {
            assert!(!clean.contains(gone), "{gone} left in {clean}");
        }
        assert!(clean.contains("Bearer [REDACTED]") && clean.ends_with("after"));
    }

    #[test]
    fn a_word_that_only_ends_like_a_prefix_is_not_a_token() {
        let text = "ask-me later; task-12345 and risk-score";
        assert_eq!(anon().text(text), text);
    }

    #[test]
    fn names_go_in_every_spelling() {
        assert_eq!(
            anon().text("ACME SHOP, acme-shop, acme_shop, AcmeShop"),
            "My SaaS, my-saas, my_saas, mysaas"
        );
    }

    #[test]
    fn a_name_too_short_to_replace_is_said() {
        let anon = Anonymiser::new(&[], &[], &["ab".to_owned()]);
        assert_eq!(anon.text("about"), "about");
        assert_eq!(anon.warnings.len(), 1);
    }

    #[test]
    fn anonymising_twice_changes_nothing() {
        let once = anon().text("/home/ana/x sk-abcdefgh Acme Shop");
        assert!(!anon().leaves_something(&once));
    }
}
