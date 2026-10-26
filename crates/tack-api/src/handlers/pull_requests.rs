//! The pull request an accepted attempt opens on GitHub.

use super::runner_protocol::artifact_storage::ArtifactStorage;
use chrono::Utc;
use serde_json::Value;
use tack_db::queries::{execution_artifacts, execution_attempts, pull_requests};

/// A project's GitHub token: its own `github_token_ref` resolved through the embedded
/// runner's secret store, else the environment's `TACK_GITHUB_TOKEN`. See
/// [`tack_app::github_sync::github_token_for_project`].
pub(crate) async fn github_token_for_project(
    state: &crate::router::AppState,
    project_id: uuid::Uuid,
) -> Option<String> {
    tack_app::github_sync::github_token_for_project(
        &state.repo,
        state,
        state.config.github_token.as_deref(),
        project_id,
    )
    .await
}

/// What the inbound GitHub poll asks of the board: the embedded runner's secret store, prill's
/// hand-off for a pull request just observed, and the merge autonomy pass that follows a poll.
#[async_trait::async_trait]
impl tack_app::github_sync::Hooks for crate::router::AppState {
    async fn resolve_secret(&self, reference: &str) -> Option<Result<String, String>> {
        let local_runner = self.local_runner.as_ref()?;
        Some(
            local_runner
                .resolve_secret(reference)
                .await
                .map_err(|error| error.to_string()),
        )
    }

    async fn pull_request_observed(&self, repo: &str, number: i64) {
        crate::prill_store::observed(self, repo, number).await;
    }

    async fn after_pass(&self) {
        crate::autonomy::tick(self).await;
    }
}

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
    if let Ok(Some(existing)) = pull_requests::pull_request_of_attempt(&app.repo, attempt_id).await
    {
        return Some(existing.url);
    }
    let git = execution_attempts::actual_execution_of_attempt(&app.repo, attempt_id)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("git").cloned())?;
    if git.get("pushed").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let branch = git.get("branch").and_then(Value::as_str)?;
    let row = execution_attempts::item_of_attempt(&app.repo, attempt_id).await;
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
    let Some(token) = github_token_for_project(&app, item.project_id).await else {
        tracing::debug!(attempt_id, "pull request: no GitHub token, none opened");
        return None;
    };

    let reference =
        execution_artifacts::latest_content_reference(&app.repo, attempt_id, MRP_MEDIA_TYPE)
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
    let base_branch = match tack_app::github_sync::default_branch(api_base, &token, &repo_name)
        .await
    {
        Ok(branch) => branch,
        Err(error) => {
            tracing::warn!(attempt_id, %error, "pull request: could not read the default branch");
            return None;
        }
    };
    let title = format!("Tack: attempt {attempt_number} for #{issue_number}");
    match tack_app::github_sync::open_pull_request(
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
            if let Err(error) = app
                .repo
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
