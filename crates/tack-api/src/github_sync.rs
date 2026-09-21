//! GitHub status sync: push (Tack → GitHub) and inbound poll (GitHub → Tack).
//!
//! When a Tack item linked to a GitHub issue crosses the Done boundary, its
//! issue is closed (or reopened). This is best-effort and fire-and-forget: it
//! never blocks or fails the originating item update.
//!
//! The inbound poll is the reverse direction: on `github_poll_seconds`, each
//! linked repo's issues are re-fetched and a changed state moves the linked
//! item through the project's ordinary workflow
//! (`Repository::update_item_atomically`, the same validation
//! `PATCH /items/{id}` uses), never a raw status write. It never calls
//! `handlers::items::maybe_sync_github`, so an inbound move can't itself
//! trigger an outbound push back to GitHub — the two directions only ever
//! meet through the item's stored status, not through a shared code path.

/// Decide whether a status change warrants a GitHub push, and in which direction.
///
/// Returns `Some(true)` to close the issue, `Some(false)` to reopen it, or
/// `None` when nothing should be pushed (the Done-ness didn't change — e.g. a
/// title edit or a same-category move).
pub fn state_change(old_done: bool, new_done: bool) -> Option<bool> {
    if old_done == new_done {
        None
    } else {
        Some(new_done)
    }
}

/// PATCH a GitHub issue's open/closed state.
///
/// `base` is the API root (`https://api.github.com`, overridable for Enterprise
/// or tests). `repo` is `owner/name`. `closed` closes the issue; otherwise it is
/// reopened.
pub async fn push_issue_state(
    base: &str,
    token: &str,
    repo: &str,
    issue_number: i64,
    closed: bool,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        // A redirect target is remote input — never forward the token to it.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let url = format!(
        "{}/repos/{}/issues/{}",
        base.trim_end_matches('/'),
        repo,
        issue_number
    );
    let state = if closed { "closed" } else { "open" };

    let resp = client
        .patch(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "state": state }))
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("GitHub PATCH {url} returned {status}");
    }
    Ok(())
}

/// The fields the inbound poll reads off one GitHub issue. Deliberately
/// narrow — body, labels, assignee are never deserialized here.
#[derive(serde::Deserialize)]
struct GithubIssue {
    number: i64,
    state: String,
    updated_at: String,
}

/// What one [`poll_once`] iteration did, for a caller (a test, or the
/// background task) to log or assert on without a timer.
#[derive(Debug, Default)]
pub struct PollSummary {
    pub repos_polled: usize,
    pub issues_updated: usize,
}

/// Poll every linked GitHub repo once for issue-state changes. No-op
/// (`Ok(PollSummary::default())`) when no `github_token` is configured — the
/// same gate the outbound push in `handlers::items::maybe_sync_github` uses.
///
/// A per-repo failure (a bad response, a network error) is logged and
/// skipped rather than failing the whole poll; ids only are logged, never
/// the token or an issue body.
pub async fn poll_once(
    state: &crate::router::AppState,
    etags: &mut std::collections::HashMap<String, String>,
) -> anyhow::Result<PollSummary> {
    let mut summary = PollSummary::default();
    let Some(token) = state.config.github_token.clone() else {
        return Ok(summary);
    };
    let base = state.config.github_api_base.clone();

    let repos: Vec<String> = sqlx::query_scalar("SELECT DISTINCT repo FROM github_links")
        .fetch_all(state.pool())
        .await?;

    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        // A redirect target is remote input — never forward the token to it.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    for repo in repos {
        summary.repos_polled += 1;
        match poll_repo(state, &client, &base, &token, &repo, etags).await {
            Ok(updated) => summary.issues_updated += updated,
            Err(error) => {
                tracing::warn!(repo = %repo, %error, "GitHub inbound poll failed for repo");
            }
        }
    }

    Ok(summary)
}

/// Poll one repo and return how many linked items it moved.
async fn poll_repo(
    state: &crate::router::AppState,
    client: &reqwest::Client,
    base: &str,
    token: &str,
    repo: &str,
    etags: &mut std::collections::HashMap<String, String>,
) -> anyhow::Result<usize> {
    let links = tack_db::repo::github_links::list_links_for_repo(state.pool(), repo).await?;
    if links.is_empty() {
        return Ok(0);
    }
    let since = links
        .iter()
        .filter_map(|(_, _, synced_at)| synced_at.clone())
        .max();

    let url = format!("{}/repos/{}/issues", base.trim_end_matches('/'), repo);
    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("state", "all")]);
    if let Some(since) = &since {
        request = request.query(&[("since", since.as_str())]);
    }
    if let Some(etag) = etags.get(repo) {
        request = request.header("If-None-Match", etag.clone());
    }

    let resp = request.send().await?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(0);
    }
    if !status.is_success() {
        anyhow::bail!("GitHub GET {url} returned {status}");
    }
    if let Some(etag) = resp.headers().get(reqwest::header::ETAG)
        && let Ok(etag) = etag.to_str()
    {
        etags.insert(repo.to_string(), etag.to_string());
    }

    let issues: Vec<GithubIssue> = resp.json().await?;
    let issues_by_number: std::collections::HashMap<i64, &GithubIssue> =
        issues.iter().map(|issue| (issue.number, issue)).collect();

    let mut updated = 0;
    for (item_id, issue_number, _) in &links {
        let Some(issue) = issues_by_number.get(issue_number) else {
            continue;
        };
        let closed = issue.state == "closed";

        let Ok(Some(item)) = state.repo.get_item(*item_id).await else {
            continue;
        };
        let Ok(Some(project)) = state.repo.get_project(item.project_id).await else {
            continue;
        };

        // Compare categories, never names: an item already in any Done status
        // stays where it is when the issue is closed, and an item in progress
        // stays in progress while the issue is open. Only a crossing of the Done
        // boundary moves it, so the outbound push's own close/reopen never
        // echoes back as a second move.
        let item_done = project.workflow.is_done_status(&item.status);
        let target = match (closed, item_done) {
            (true, false) => project.workflow.find_first_done_status(),
            (false, true) => project
                .workflow
                .statuses
                .iter()
                .filter(|s| s.category == tack_core::workflow::StatusCategory::Todo)
                .min_by_key(|s| s.order)
                .map(|s| s.name.as_str()),
            _ => None,
        };

        if let Some(target) = target {
            let input = tack_core::models::UpdateItem {
                status: Some(target.to_string()),
                ..Default::default()
            };
            match state
                .repo
                .update_item_atomically(*item_id, input, &project.workflow, None)
                .await
            {
                Ok(tack_db::repo::items::AtomicItemUpdateOutcome::Updated { .. }) => {
                    updated += 1;
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(item_id = %item_id, %error, "GitHub inbound status move failed");
                    continue;
                }
            }
        }

        sqlx::query("UPDATE github_links SET synced_at = ? WHERE item_id = ?")
            .bind(&issue.updated_at)
            .bind(item_id.to_string())
            .execute(state.pool())
            .await?;
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_change_only_fires_on_boundary_cross() {
        assert_eq!(state_change(false, true), Some(true)); // moved into Done → close
        assert_eq!(state_change(true, false), Some(false)); // moved out of Done → reopen
        assert_eq!(state_change(false, false), None); // still not done → no-op
        assert_eq!(state_change(true, true), None); // still done → no-op
    }

    #[tokio::test]
    async fn push_closes_issue_with_correct_request() {
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/acme/widgets/issues/42"))
            .and(header("authorization", "Bearer tok-123"))
            .and(body_json(serde_json::json!({ "state": "closed" })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "number": 42 })),
            )
            .expect(1)
            .mount(&server)
            .await;

        push_issue_state(&server.uri(), "tok-123", "acme/widgets", 42, true)
            .await
            .expect("push should succeed");
        // `.expect(1)` is verified on server drop.
    }

    #[tokio::test]
    async fn push_reopens_issue() {
        use wiremock::matchers::{body_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/acme/widgets/issues/7"))
            .and(body_json(serde_json::json!({ "state": "open" })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        push_issue_state(&server.uri(), "tok", "acme/widgets", 7, false)
            .await
            .expect("reopen should succeed");
    }

    #[tokio::test]
    async fn push_errors_on_non_success_status() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = push_issue_state(&server.uri(), "tok", "acme/widgets", 1, true)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("404"), "got: {err}");
    }

    #[tokio::test]
    async fn push_issue_redirect_never_forwards_token() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let origin = MockServer::start().await;
        let private_destination = MockServer::start().await;
        let redirect_target = format!("{}/instance-metadata", private_destination.uri());

        Mock::given(method("PATCH"))
            .and(path("/repos/acme/widgets/issues/42"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", redirect_target))
            .mount(&origin)
            .await;

        let err = push_issue_state(&origin.uri(), "tok-123", "acme/widgets", 42, true)
            .await
            .expect_err("a redirect is not a successful GitHub update");
        assert!(err.to_string().contains("302"), "got: {err}");
        assert!(
            private_destination
                .received_requests()
                .await
                .expect("inspect private destination")
                .is_empty(),
            "a redirect must never forward the GitHub authorization token"
        );
    }
}
