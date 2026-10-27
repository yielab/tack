//! The pull request an accepted attempt opens on GitHub.

use chrono::Utc;
use serde_json::Value;
use tack_db::Repository;
use tack_db::queries::{execution_artifacts, execution_attempts, pull_requests};

use crate::github_sync;
use crate::ports::Ports;

const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

/// When an accepted attempt pushed its branch and its item is linked to a
/// GitHub issue, opens a pull request from that branch to the repository's
/// default branch, records it, and returns its URL. An attempt that already
/// has one returns that URL without calling GitHub. The body is the attempt's
/// merge-readiness pack rendered as Markdown, else a plain note; the title
/// carries only numbers, never item or agent text. Every refusal is logged and
/// returns `None`, never failing the review.
pub async fn open_for_attempt(
    repo: &Repository,
    ports: &Ports,
    attempt_id: &str,
) -> Option<String> {
    if let Ok(Some(existing)) = pull_requests::pull_request_of_attempt(repo, attempt_id).await {
        return Some(existing.url);
    }
    let git = execution_attempts::actual_execution_of_attempt(repo, attempt_id)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("git").cloned())?;
    if git.get("pushed").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let branch = git.get("branch").and_then(Value::as_str)?;
    let row = execution_attempts::item_of_attempt(repo, attempt_id).await;
    let Ok(Some(execution_attempts::AttemptItem {
        item_id,
        attempt_number,
    })) = row
    else {
        tracing::warn!(
            attempt_id,
            "pull request: could not read the attempt's item"
        );
        return None;
    };
    let Ok(item_uuid) = item_id.parse::<uuid::Uuid>() else {
        return None;
    };
    let link = match repo.get_github_link(item_uuid).await {
        Ok(Some(link)) => link,
        Ok(None) => {
            tracing::debug!(
                attempt_id,
                "pull request: the item has no GitHub link, none opened"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(attempt_id, %error, "pull request: could not read the GitHub link");
            return None;
        }
    };
    let (repo_name, issue_number) = link;
    let Ok(Some(item)) = repo.get_item(item_uuid).await else {
        return None;
    };
    let Some(token) = ports
        .github
        .token_for_project(repo, ports.runner.as_deref(), item.project_id)
        .await
    else {
        tracing::debug!(attempt_id, "pull request: no GitHub token, none opened");
        return None;
    };

    let reference = execution_artifacts::latest_content_reference(repo, attempt_id, MRP_MEDIA_TYPE)
        .await
        .ok()
        .flatten();
    let pack_body = match reference {
        Some(reference) => ports
            .artifacts
            .read(&reference)
            .await
            .and_then(|bytes| {
                serde_json::from_slice::<tack_protocol::mrp::MergeReadinessPack>(&bytes).ok()
            })
            .map(|pack| tack_protocol::mrp::render_markdown(&pack)),
        None => None,
    };
    let body = pack_body.unwrap_or_else(|| {
        format!(
            "No merge-readiness pack was reported for this attempt.\n\nBranch `{branch}` at `{}`.",
            git.get("head_commit")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )
    });

    let api_base = ports.github.api_base.as_str();
    let base_branch = match github_sync::default_branch(api_base, &token, &repo_name).await {
        Ok(branch) => branch,
        Err(error) => {
            tracing::warn!(attempt_id, %error, "pull request: could not read the default branch");
            return None;
        }
    };
    let title = format!("Tack: attempt {attempt_number} for #{issue_number}");
    match github_sync::open_pull_request(
        api_base,
        &token,
        &repo_name,
        branch,
        &base_branch,
        &title,
        &body,
    )
    .await
    {
        Ok((number, url)) => {
            let opened_at = Utc::now().to_rfc3339();
            if let Err(error) = repo
                .record_pull_request(attempt_id, &repo_name, number, &url, &opened_at)
                .await
            {
                tracing::warn!(attempt_id, %error, "pull request: opened but not recorded");
            }
            Some(url)
        }
        Err(error) => {
            tracing::warn!(attempt_id, %error, "pull request: GitHub refused it");
            None
        }
    }
}
