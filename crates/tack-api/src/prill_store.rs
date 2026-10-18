//! The board's prill store: one database per repository under
//! `<storage_dir>/prill/<owner>__<repo>.db`, fed with what Tack knows (the
//! pack a run produced, and what became of its pull request) and read back
//! by `prill metrics`. Each record is made once: `prill_records` holds
//! `(attempt, kind)` after prill took it. A failure is logged and left
//! unrecorded, so the next tick tries again.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::prill::{Scratch, Unavailable, run, write_private};
use crate::router::AppState;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
const ATTESTATION_MEDIA_TYPE: &str = "application/vnd.in-toto+json";
const TIMEOUT: Duration = Duration::from_secs(30);

/// `<storage_dir>/prill/<owner>__<repo>.db` for `owner/repo`; `None` when the
/// name is not a plain GitHub `owner/repo`.
pub fn store_path(storage_dir: &str, repo: &str) -> Option<PathBuf> {
    let (owner, name) = repo.split_once('/')?;
    let plain = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (plain(owner) && plain(name)).then(|| {
        PathBuf::from(storage_dir)
            .join("prill")
            .join(format!("{owner}__{name}.db"))
    })
}

async fn recorded(app: &AppState, attempt_id: &str, kind: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM prill_records WHERE attempt_id = ? AND kind = ?",
    )
    .bind(attempt_id)
    .bind(kind)
    .fetch_one(app.pool())
    .await
    .unwrap_or(0)
        > 0
}

async fn mark_recorded(app: &AppState, attempt_id: &str, kind: &str) {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO prill_records (attempt_id, kind, recorded_at) VALUES (?, ?, ?)",
    )
    .bind(attempt_id)
    .bind(kind)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(app.pool())
    .await;
    if let Err(error) = result {
        tracing::warn!(attempt_id, kind, %error, "prill record: could not mark it recorded");
    }
}

/// Runs `prill args…`; true when prill took it. A failure is logged.
async fn record(app: &AppState, attempt_id: &str, kind: &str, args: Vec<String>) -> bool {
    match run(&app.prill_program().await, &args, TIMEOUT).await {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let said = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(attempt_id, kind, status = %output.status, said = %said.trim(), "prill refused the record; it is tried again");
            false
        }
        Err(Unavailable) => {
            tracing::debug!(attempt_id, kind, "prill is not installed; the record waits");
            false
        }
    }
}

/// The repository (`owner/repo`) of the GitHub issue an attempt's item is linked to.
async fn repository_of(app: &AppState, attempt_id: &str) -> Option<String> {
    let item: String = sqlx::query_scalar(
        "SELECT r.item_id FROM execution_attempts a JOIN execution_requests r ON r.id = a.request_id WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(app.pool())
    .await
    .ok()??;
    let (repo, _) = app.repo.get_github_link(item.parse().ok()?).await.ok()??;
    Some(repo)
}

async fn artifact_bytes(app: &AppState, attempt_id: &str, media_type: &str) -> Option<Vec<u8>> {
    let reference: String = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts \
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(media_type)
    .fetch_optional(app.pool())
    .await
    .ok()??;
    let storage = ArtifactStorage::new(format!("{}/execution-artifacts", app.config.storage_dir));
    let mut file = storage.open_for_read(&reference).await.ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    Some(bytes)
}

fn ensure_store_dir(db: &std::path::Path) -> bool {
    db.parent()
        .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
}

/// Hands the attempt's pack (and its attestation, when it came with one) to
/// `prill record mrp`, once. An attempt with no pack, or whose item is linked
/// to no repository, has nothing to record.
pub async fn record_mrp(app: &AppState, attempt_id: &str) {
    if recorded(app, attempt_id, "mrp").await {
        return;
    }
    let Some(db) = repository_of(app, attempt_id)
        .await
        .and_then(|repo| store_path(&app.config.storage_dir, &repo))
    else {
        return;
    };
    let Some(pack) = artifact_bytes(app, attempt_id, MRP_MEDIA_TYPE).await else {
        return;
    };
    let attestation = artifact_bytes(app, attempt_id, ATTESTATION_MEDIA_TYPE).await;
    let Some(scratch) = Scratch::new() else {
        return;
    };
    let pack_path = scratch.0.join("mrp.json");
    let mut args = vec![
        "record".to_owned(),
        "mrp".to_owned(),
        "--db".to_owned(),
        db.display().to_string(),
        pack_path.display().to_string(),
    ];
    if write_private(&pack_path, &pack).is_err() || !ensure_store_dir(&db) {
        return;
    }
    if let Some(envelope) = attestation {
        let path = scratch.0.join("mrp.intoto.json");
        if write_private(&path, &envelope).is_err() {
            return;
        }
        args.push("--attestation".to_owned());
        args.push(path.display().to_string());
    }
    if record(app, attempt_id, "mrp", args).await {
        mark_recorded(app, attempt_id, "mrp").await;
    }
}

/// Tells prill what became of the pull request `repo#number`: the pack first
/// (a retry of [`record_mrp`]), then each outcome Tack has seen (merged,
/// closed, reverted, red CI) that prill was not told yet.
pub async fn observed(app: &AppState, repo: &str, number: i64) {
    let Ok(Some(row)) = sqlx::query_as::<_, tack_db::repo::pull_requests::PullRequestRow>(
        "SELECT * FROM pull_requests WHERE repo = ? AND number = ?",
    )
    .bind(repo)
    .bind(number)
    .fetch_optional(app.pool())
    .await
    else {
        return;
    };
    record_mrp(app, &row.attempt_id).await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut due: Vec<(&str, String)> = Vec::new();
    if matches!(row.state.as_str(), "merged" | "reverted") {
        due.push((
            "merged",
            row.merged_at.clone().unwrap_or_else(|| now.clone()),
        ));
    }
    if row.state == "closed" {
        due.push((
            "closed",
            row.closed_at.clone().unwrap_or_else(|| now.clone()),
        ));
    }
    if row.state == "reverted" {
        due.push(("reverted", now.clone()));
    }
    if let Some(at) = &row.red_ci_at {
        due.push(("red_ci", at.clone()));
    }
    let mut pending = Vec::new();
    for (kind, at) in due {
        if !recorded(app, &row.attempt_id, kind).await {
            pending.push((kind, at));
        }
    }
    if pending.is_empty() {
        return;
    }
    let Some(db) = store_path(&app.config.storage_dir, repo) else {
        return;
    };
    let Some(head) = head_commit(app, &row.attempt_id).await else {
        tracing::warn!(attempt_id = %row.attempt_id, "prill outcome: the attempt names no head commit");
        return;
    };
    let Some(scratch) = Scratch::new() else {
        return;
    };
    for (kind, at) in pending {
        let path = scratch.0.join(format!("outcome-{kind}.json"));
        let file = json!({
            "contract": "outcome/v1",
            "headCommit": head,
            "kind": kind,
            "observedAt": at,
            "source": "tack",
            "reference": row.url,
        });
        if write_private(&path, file.to_string().as_bytes()).is_err() || !ensure_store_dir(&db) {
            return;
        }
        let args = vec![
            "record".to_owned(),
            "outcome".to_owned(),
            "--db".to_owned(),
            db.display().to_string(),
            path.display().to_string(),
        ];
        if record(app, &row.attempt_id, kind, args).await {
            mark_recorded(app, &row.attempt_id, kind).await;
        }
    }
}

async fn head_commit(app: &AppState, attempt_id: &str) -> Option<String> {
    let raw: String =
        sqlx::query_scalar("SELECT actual_execution FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(app.pool())
            .await
            .ok()??;
    serde_json::from_str::<Value>(&raw)
        .ok()?
        .pointer("/git/head_commit")?
        .as_str()
        .map(str::to_owned)
}

/// prill's version, from `prill --version` (`prill 0.1.0`).
pub async fn version(program: &str) -> Result<String, String> {
    match run(program, &["--version".to_owned()], TIMEOUT).await {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .last()
            .map(str::to_owned)
            .ok_or_else(|| "prill did not say its version".to_owned()),
        Ok(_) => Err("prill did not say its version".to_owned()),
        Err(Unavailable) => Err(crate::prill::NOT_INSTALLED.to_owned()),
    }
}

/// `prill metrics --db <db> --json`, parsed.
pub async fn metrics(program: &str, db: &std::path::Path) -> Result<Value, String> {
    let args = [
        "metrics".to_owned(),
        "--db".to_owned(),
        db.display().to_string(),
        "--json".to_owned(),
    ];
    match run(program, &args, TIMEOUT).await {
        Ok(output) if output.status.success() => serde_json::from_slice(&output.stdout)
            .map_err(|_| "prill answered with something that is not JSON".to_owned()),
        Ok(output) => Err(format!("prill metrics failed ({})", output.status)),
        Err(Unavailable) => Err(crate::prill::NOT_INSTALLED.to_owned()),
    }
}
