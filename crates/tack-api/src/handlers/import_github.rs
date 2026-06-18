use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::{CreateItem, ItemType};

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct GitHubImportRequest {
    /// Repository as "owner/repo" or a full GitHub URL.
    pub repo: String,
    /// Personal access token. Optional — unauthenticated calls are allowed
    /// but are rate-limited to 60 requests/hour.
    #[serde(default)]
    pub token: Option<String>,
    /// Include closed issues (default: false — only open issues are imported).
    #[serde(default)]
    pub import_closed: bool,
    /// When non-empty, only issues that carry at least one of these labels
    /// are imported; all others are skipped.
    #[serde(default)]
    pub label_filter: Vec<String>,
}

// ─── GitHub API response shapes ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    labels: Vec<GhLabel>,
    assignee: Option<GhUser>,
    html_url: String,
    /// Present only on pull-request records — used to skip them.
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GhUser {
    login: String,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// POST /api/projects/:id/import-github
///
/// Fetches all (matching) issues from a GitHub repository and creates Tack
/// items for them.  Pull-request records are silently skipped.
///
/// Rate-limit note: unauthenticated calls are limited to 60 requests/hour by
/// GitHub; supplying a `token` raises this to 5 000 req/hour.
#[instrument(skip(state))]
pub async fn import_github(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<GitHubImportRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let (owner, repo_name) = parse_github_repo(&input.repo).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Invalid GitHub repo '{}'. Use 'owner/repo' or a full GitHub URL.",
            input.repo
        ))
    })?;

    // Determine target statuses from the project's workflow
    let open_status = project
        .workflow
        .statuses
        .iter()
        .min_by_key(|s| s.order)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "To Do".to_string());

    let done_status = project
        .workflow
        .find_first_done_status()
        .unwrap_or("Done")
        .to_string();

    let client = reqwest::Client::builder()
        .user_agent("Tack/1.0 (github.com/yielab/Tack)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("HTTP client error: {e}")))?;

    let gh_state = if input.import_closed { "all" } else { "open" };

    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut rate_limit_remaining: Option<u64> = None;
    let mut page = 1u32;

    loop {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo_name}/issues\
             ?state={gh_state}&per_page=100&page={page}"
        );

        let mut req = client
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref tok) = input.token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("GitHub request failed: {e}")))?;

        // Capture rate-limit header before consuming the response body
        if let Some(v) = resp.headers().get("x-ratelimit-remaining") {
            rate_limit_remaining = v.to_str().ok().and_then(|s| s.parse().ok());
        }

        match resp.status().as_u16() {
            200 => {}
            401 | 403 if rate_limit_remaining == Some(0) => {
                return Err(ApiError::BadRequest(
                    "GitHub rate limit exceeded. Supply a token or wait for the window to reset."
                        .into(),
                ));
            }
            401 => return Err(ApiError::BadRequest("Invalid GitHub token.".into())),
            403 => {
                return Err(ApiError::BadRequest(
                    "GitHub API access forbidden. The token may lack 'repo' scope.".into(),
                ));
            }
            404 => {
                return Err(ApiError::NotFound(format!(
                    "GitHub repository '{owner}/{repo_name}' not found (or not accessible)."
                )));
            }
            status => {
                return Err(ApiError::Internal(anyhow::anyhow!(
                    "GitHub API returned unexpected status {status}"
                )));
            }
        }

        let issues: Vec<GhIssue> = resp.json().await.map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("Failed to parse GitHub response: {e}"))
        })?;

        if issues.is_empty() {
            break;
        }

        for issue in &issues {
            // Pull requests appear in the issues endpoint — skip them
            if issue.pull_request.is_some() {
                skipped += 1;
                continue;
            }

            // Label filter: skip if the issue has none of the requested labels
            if !input.label_filter.is_empty() {
                let has_match = issue.labels.iter().any(|l| {
                    input
                        .label_filter
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case(&l.name))
                });
                if !has_match {
                    skipped += 1;
                    continue;
                }
            }

            let status = if issue.state == "closed" {
                done_status.clone()
            } else {
                open_status.clone()
            };

            let tags: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

            let data = CreateItem {
                title: format!("[#{num}] {title}", num = issue.number, title = issue.title),
                description: Some(build_description(issue)),
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: Some(tags),
                due_date: None,
                sprint_id: None,
                assignee: issue.assignee.as_ref().map(|u| u.login.clone()),
            };

            match state.repo.create_item(project_id, &status, data).await {
                Ok(_) => created += 1,
                Err(e) => {
                    tracing::warn!(
                        issue = issue.number,
                        error = %e,
                        "Skipped GitHub issue due to create_item error"
                    );
                    skipped += 1;
                }
            }
        }

        // GitHub returns fewer than 100 items on the last page
        if issues.len() < 100 {
            break;
        }
        page += 1;
    }

    tracing::info!(
        project_id = %project_id,
        repo = format!("{owner}/{repo_name}"),
        created,
        skipped,
        "GitHub import complete"
    );

    Ok(Json(serde_json::json!({
        "created": created,
        "skipped": skipped,
        "rate_limit_remaining": rate_limit_remaining,
    })))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parse "owner/repo", "https://github.com/owner/repo", or
/// "https://github.com/owner/repo.git" into (owner, repo).
fn parse_github_repo(input: &str) -> Option<(String, String)> {
    let s = input.trim().trim_end_matches('/').trim_end_matches(".git");

    let path = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"))
        .unwrap_or(s);

    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn build_description(issue: &GhIssue) -> String {
    let mut d = format!("GitHub Issue: {}", issue.html_url);
    if let Some(body) = issue.body.as_deref().filter(|b| !b.trim().is_empty()) {
        d.push_str("\n\n");
        d.push_str(body);
    }
    d
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::parse_github_repo;

    #[test]
    fn parses_owner_slash_repo() {
        let (o, r) = parse_github_repo("torvalds/linux").unwrap();
        assert_eq!(o, "torvalds");
        assert_eq!(r, "linux");
    }

    #[test]
    fn parses_full_https_url() {
        let (o, r) = parse_github_repo("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(o, "rust-lang");
        assert_eq!(r, "rust");
    }

    #[test]
    fn parses_url_with_git_suffix() {
        let (o, r) = parse_github_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(o, "owner");
        assert_eq!(r, "repo");
    }

    #[test]
    fn parses_url_with_trailing_slash() {
        let (o, r) = parse_github_repo("https://github.com/owner/repo/").unwrap();
        assert_eq!(o, "owner");
        assert_eq!(r, "repo");
    }

    #[test]
    fn rejects_missing_repo() {
        assert!(parse_github_repo("just-owner").is_none());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_github_repo("").is_none());
    }

    #[test]
    fn rejects_deep_path() {
        assert!(parse_github_repo("https://github.com/owner/repo/tree/main").is_none());
    }
}
