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
//!
//! A pull request an attempt opened (`open_pull_request`) is followed by the same
//! poll: the `/issues` response also lists pull requests, and a stored one has its
//! state, `merged_at` and `closed_at` kept current.

/// The single place the token-resolution order lives: a project's own
/// `github_token_ref` (resolved through the embedded runner's secret store,
/// when one is wired into this `AppState`), falling through to the
/// environment's `TACK_GITHUB_TOKEN` when the project has no reference, no
/// runner control is wired in, or the reference fails to resolve. A
/// resolution failure logs the reference *name* — never the value — at
/// `warn` and falls through rather than failing the caller.
/// `handlers::items::maybe_sync_github` and `handlers::comments::
/// maybe_sync_github` call this instead of reading `state.config.
/// github_token` directly; so does the inbound poll's `poll_repo`.
pub(crate) async fn github_token_for_project(
    state: &crate::router::AppState,
    project_id: uuid::Uuid,
) -> Option<String> {
    if let Ok(Some(project)) = state.repo.get_project(project_id).await
        && let Some(reference) = project.github_token_ref.as_deref()
        && let Some(local_runner) = &state.local_runner
    {
        match local_runner.resolve_secret(reference).await {
            Ok(value) => return Some(value),
            Err(error) => {
                tracing::warn!(
                    project_id = %project_id,
                    reference = %reference,
                    %error,
                    "GitHub token reference did not resolve"
                );
            }
        }
    }
    state.config.github_token.clone()
}

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

/// POST a new comment onto a GitHub issue, returning the id GitHub assigns
/// it — stored so the inbound poll never mirrors it back in, and so a
/// second push attempt (there isn't one yet) would know it already went out.
///
/// `base`, `token`, `repo`, `issue_number` are as [`push_issue_state`].
pub async fn push_issue_comment(
    base: &str,
    token: &str,
    repo: &str,
    issue_number: i64,
    body: &str,
) -> anyhow::Result<i64> {
    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        // A redirect target is remote input — never forward the token to it.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let url = format!(
        "{}/repos/{}/issues/{}/comments",
        base.trim_end_matches('/'),
        repo,
        issue_number
    );

    let resp = client
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "body": body }))
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("GitHub POST {url} returned {status}");
    }
    let created: CreatedComment = resp.json().await?;
    Ok(created.id)
}

/// GET a repository's default branch — the base a new pull request targets.
pub async fn default_branch(base: &str, token: &str, repo: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        // A redirect target is remote input — never forward the token to it.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let url = format!("{}/repos/{}", base.trim_end_matches('/'), repo);
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("GitHub GET {url} returned {status}");
    }
    #[derive(serde::Deserialize)]
    struct Repo {
        default_branch: String,
    }
    Ok(resp.json::<Repo>().await?.default_branch)
}

/// POST a new pull request, returning its `(number, html_url)`. Best-effort like
/// the other pushes: the caller logs a failure and moves on.
///
/// `head` is the pushed branch, `base_branch` the branch it targets; `title`
/// and `body` are used as given, so a caller never passes untrusted text as the
/// title.
pub async fn open_pull_request(
    base: &str,
    token: &str,
    repo: &str,
    head: &str,
    base_branch: &str,
    title: &str,
    body: &str,
) -> anyhow::Result<(i64, String)> {
    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/tack)")
        .timeout(std::time::Duration::from_secs(15))
        // A redirect target is remote input — never forward the token to it.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let url = format!("{}/repos/{}/pulls", base.trim_end_matches('/'), repo);
    let resp = client
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "title": title,
            "head": head,
            "base": base_branch,
            "body": body,
        }))
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("GitHub POST {url} returned {status}");
    }
    let created: CreatedPullRequest = resp.json().await?;
    Ok((created.number, created.html_url))
}

/// The number and page GitHub assigns a newly opened pull request.
#[derive(serde::Deserialize)]
struct CreatedPullRequest {
    number: i64,
    html_url: String,
}

/// The id GitHub assigns a newly created comment.
#[derive(serde::Deserialize)]
struct CreatedComment {
    id: i64,
}

/// The fields the inbound poll reads off one GitHub issue. Deliberately
/// narrow — body, labels, assignee are never deserialized here.
#[derive(serde::Deserialize)]
struct GithubIssue {
    number: i64,
    state: String,
    updated_at: String,
    /// Present only on the entries that are pull requests.
    #[serde(default)]
    pull_request: Option<GithubPullRequestRef>,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(serde::Deserialize)]
struct GithubPullRequestRef {
    #[serde(default)]
    merged_at: Option<String>,
}

/// The fields the inbound poll reads off one GitHub issue comment.
#[derive(serde::Deserialize)]
struct GithubComment {
    id: i64,
    body: String,
    user: GithubCommentAuthor,
}

#[derive(serde::Deserialize)]
struct GithubCommentAuthor {
    login: String,
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
/// poll only *starts* on the environment token; it does not gate on any
/// project's `github_token_ref` (a project reference can only ever narrow
/// which token a given repo's requests use once the poll is already
/// running — see `poll_repo`).
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

    // Links for one repo come from one import, so one project: the first
    // link's item's project decides which token this repo's requests use.
    // Falls back to the env token already passed in when there is no
    // project-level reference, no runner control wired in, or it fails to
    // resolve — see `github_token_for_project`.
    let project_token = match state.repo.get_item(links[0].0).await {
        Ok(Some(item)) => github_token_for_project(state, item.project_id).await,
        _ => None,
    };
    let token: &str = project_token.as_deref().unwrap_or(token);

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

    for issue in issues.iter().filter(|issue| issue.pull_request.is_some()) {
        if let Err(error) = follow_pull_request(state, repo, issue).await {
            tracing::warn!(pull_request = issue.number, %error, "GitHub pull request follow failed");
        }
    }

    let mut updated = 0;
    for link in &links {
        let (item_id, issue_number, _synced_at) = link;

        // Only an issue the `since` list returned has changed: a new comment
        // bumps the issue's `updated_at`, so an issue absent here has
        // neither a state change nor a comment to read.
        let Some(issue) = issues_by_number.get(issue_number) else {
            continue;
        };

        // Inbound comments: mirror any GitHub comment on this issue not yet
        // stored on the item, whether or not its state changed.
        if let Err(error) = sync_inbound_comments(state, client, base, token, repo, link).await {
            tracing::warn!(item_id = %item_id, %error, "GitHub inbound comment sync failed");
        }
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

/// Record what GitHub says about one pull-request entry of the issues list:
/// its state, and, when it is a `Revert` PR whose body cites `#<n>`, that the
/// stored merged PR `n` was reverted by it.
async fn follow_pull_request(
    state: &crate::router::AppState,
    repo: &str,
    pr: &GithubIssue,
) -> anyhow::Result<()> {
    let Some(pull_request) = &pr.pull_request else {
        return Ok(());
    };
    let merged_at = pull_request.merged_at.as_deref();
    let pr_state = if merged_at.is_some() {
        "merged"
    } else if pr.state == "closed" {
        "closed"
    } else {
        "open"
    };
    tack_db::repo::pull_requests::observe(
        state.pool(),
        repo,
        pr.number,
        pr_state,
        merged_at,
        pr.closed_at.as_deref(),
    )
    .await?;

    if pr.title.as_deref().is_some_and(|t| t.starts_with("Revert")) {
        for cited in cited_numbers(pr.body.as_deref().unwrap_or_default()) {
            tack_db::repo::pull_requests::mark_reverted(state.pool(), repo, cited, pr.number)
                .await?;
        }
    }
    Ok(())
}

/// Every `#<n>` in `text`.
fn cited_numbers(text: &str) -> Vec<i64> {
    text.split('#')
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect()
}

/// Mirror any GitHub comment on `link`'s issue not yet stored on its item,
/// attributed to the GitHub login in the body's first line. Reads the db
/// repo directly (never `handlers::comments::create_comment`), so a comment
/// created here is never itself pushed back out to GitHub.
async fn sync_inbound_comments(
    state: &crate::router::AppState,
    client: &reqwest::Client,
    base: &str,
    token: &str,
    repo: &str,
    link: &(uuid::Uuid, i64, Option<String>),
) -> anyhow::Result<()> {
    let (item_id, issue_number, since) = link;

    let url = format!(
        "{}/repos/{}/issues/{}/comments",
        base.trim_end_matches('/'),
        repo,
        issue_number
    );
    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"));
    if let Some(since) = since {
        request = request.query(&[("since", since.as_str())]);
    }

    let resp = request.send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("GitHub GET {url} returned {status}");
    }

    let comments: Vec<GithubComment> = resp.json().await?;
    if comments.is_empty() {
        return Ok(());
    }

    let stored = state.repo.list_github_comment_ids(*item_id).await?;
    for comment in comments {
        if stored.contains(&comment.id) {
            continue;
        }
        let created = state
            .repo
            .create_comment(
                *item_id,
                tack_core::models::CreateComment {
                    content: format!("@{} on GitHub:\n\n{}", comment.user.login, comment.body),
                    author: Some(comment.user.login.clone()),
                },
            )
            .await?;
        state
            .repo
            .set_comment_github_id(created.id, comment.id)
            .await?;
    }
    Ok(())
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
