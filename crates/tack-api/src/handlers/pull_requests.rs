//! The pull request an accepted attempt opens on GitHub.

use super::runner_protocol::artifact_storage::ArtifactStorage;
use chrono::Utc;
use serde_json::Value;

/// When an accepted attempt pushed its branch and its item is linked to a
/// GitHub issue, opens a pull request from that branch to the repository's
/// default branch, records it, and returns its URL. An attempt that already
/// has one returns that URL without calling GitHub. The body is the attempt's
/// merge-readiness pack rendered as Markdown, else a plain note; the title
/// carries only numbers, never item or agent text. Every refusal is logged and
/// returns `None`, never failing the review.
pub(crate) async fn open_pull_request_for_attempt(
    app: crate::router::AppState,
    storage: &ArtifactStorage,
    attempt_id: &str,
) -> Option<String> {
    const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";
    let pool = app.repo.pool();
    if let Ok(Some(existing)) =
        tack_db::repo::pull_requests::get_for_attempt(pool, attempt_id).await
    {
        return Some(existing.url);
    }
    let actual: Result<Option<Option<String>>, sqlx::Error> =
        sqlx::query_scalar("SELECT actual_execution FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(pool)
            .await;
    let git = actual
        .ok()
        .flatten()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("git").cloned())?;
    if git.get("pushed").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let branch = git.get("branch").and_then(Value::as_str)?;
    let row: Result<Option<(String, i64)>, sqlx::Error> = sqlx::query_as(
        "SELECT r.item_id, a.attempt_number FROM execution_attempts a \
         JOIN execution_requests r ON r.id = a.request_id WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await;
    let Ok(Some((item_id, attempt_number))) = row else {
        tracing::warn!(
            attempt_id,
            "pull request: could not read the attempt's item"
        );
        return None;
    };
    let Ok(item_uuid) = item_id.parse::<uuid::Uuid>() else {
        return None;
    };
    let link = match app.repo.get_github_link(item_uuid).await {
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
    let Ok(Some(item)) = app.repo.get_item(item_uuid).await else {
        return None;
    };
    let Some(token) = crate::github_sync::github_token_for_project(&app, item.project_id).await
    else {
        tracing::debug!(attempt_id, "pull request: no GitHub token, none opened");
        return None;
    };

    let reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts \
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(MRP_MEDIA_TYPE)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let mut pack_body = None;
    if let Some(reference) = reference
        && let Ok(mut file) = storage.open_for_read(&reference).await
    {
        let mut bytes = Vec::new();
        if tokio::io::AsyncReadExt::read_to_end(&mut file, &mut bytes)
            .await
            .is_ok()
            && let Ok(pack) =
                serde_json::from_slice::<tack_protocol::mrp::MergeReadinessPack>(&bytes)
        {
            pack_body = Some(tack_protocol::mrp::render_markdown(&pack));
        }
    }
    let body = pack_body.unwrap_or_else(|| {
        format!(
            "No merge-readiness pack was reported for this attempt.\n\nBranch `{branch}` at `{}`.",
            git.get("head_commit")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )
    });

    let api_base = app.config.github_api_base.as_str();
    let base_branch = match crate::github_sync::default_branch(api_base, &token, &repo_name).await {
        Ok(branch) => branch,
        Err(error) => {
            tracing::warn!(attempt_id, %error, "pull request: could not read the default branch");
            return None;
        }
    };
    let title = format!("Tack: attempt {attempt_number} for #{issue_number}");
    match crate::github_sync::open_pull_request(
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
            if let Err(error) = tack_db::repo::pull_requests::insert(
                pool, attempt_id, &repo_name, number, &url, &opened_at,
            )
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
