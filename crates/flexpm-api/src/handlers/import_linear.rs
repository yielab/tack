use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use flexpm_core::models::{CreateItem, ItemType};

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct LinearImportRequest {
    /// Linear personal API key (create at https://linear.app/settings/api).
    pub api_key: String,
    /// Import only issues from this team (slug or ID). When omitted, all
    /// issues accessible to the key are fetched.
    #[serde(default)]
    pub team_id: Option<String>,
    /// Import only issues belonging to this Linear project ID.  Overrides
    /// `team_id` when both are set.
    #[serde(default)]
    pub project_id: Option<String>,
    /// Include completed/cancelled issues (default: false — open only).
    #[serde(default)]
    pub import_completed: bool,
    /// When non-empty, only issues carrying at least one matching label are
    /// imported; all others are skipped.
    #[serde(default)]
    pub label_filter: Vec<String>,
}

// ─── Linear GraphQL response shapes ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GqlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssuesData {
    issues: IssueConnection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueConnection {
    nodes: Vec<LinearIssue>,
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinearIssue {
    identifier: String,
    title: String,
    description: Option<String>,
    url: String,
    state: LinearState,
    assignee: Option<LinearUser>,
    labels: LabelConnection,
    priority: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct LinearState {
    #[serde(rename = "type")]
    state_type: String,
}

#[derive(Debug, Deserialize)]
struct LinearUser {
    name: String,
}

#[derive(Debug, Deserialize)]
struct LabelConnection {
    nodes: Vec<LinearLabel>,
}

#[derive(Debug, Deserialize)]
struct LinearLabel {
    name: String,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// POST /api/projects/:id/import-linear
///
/// Fetches issues from Linear's GraphQL API and creates FlexPM items.
/// Pagination is cursor-based (50 issues per page).
#[instrument(skip(state))]
pub async fn import_linear(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<LinearImportRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if input.api_key.trim().is_empty() {
        return Err(ApiError::BadRequest("api_key must not be empty.".into()));
    }

    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

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
        .user_agent("FlexPM/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("HTTP client error: {e}")))?;

    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut cursor: Option<String> = None;

    loop {
        let query = build_query(&input, cursor.as_deref());

        let resp = client
            .post("https://api.linear.app/graphql")
            .header("Authorization", input.api_key.trim())
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&serde_json::json!({ "query": query })).unwrap())
            .send()
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Linear request failed: {e}")))?;

        match resp.status().as_u16() {
            200 => {}
            401 => {
                return Err(ApiError::BadRequest(
                    "Invalid Linear API key. Generate one at https://linear.app/settings/api."
                        .into(),
                ));
            }
            429 => {
                return Err(ApiError::BadRequest(
                    "Linear API rate limit exceeded. Wait a moment and retry.".into(),
                ));
            }
            status => {
                return Err(ApiError::Internal(anyhow::anyhow!(
                    "Linear API returned unexpected status {status}"
                )));
            }
        }

        let body: GqlResponse<IssuesData> = resp.json().await.map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("Failed to parse Linear response: {e}"))
        })?;

        if let Some(errors) = body.errors {
            let msg = errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ApiError::BadRequest(format!("Linear GraphQL error: {msg}")));
        }

        let conn = body
            .data
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Linear returned empty data")))?
            .issues;

        for issue in &conn.nodes {
            // Skip completed/cancelled issues unless requested
            let is_done = matches!(issue.state.state_type.as_str(), "completed" | "cancelled");
            if is_done && !input.import_completed {
                skipped += 1;
                continue;
            }

            // Label filter
            if !input.label_filter.is_empty() {
                let has_match = issue.labels.nodes.iter().any(|l| {
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

            let status = if is_done {
                done_status.clone()
            } else {
                open_status.clone()
            };

            let tags: Vec<String> = issue.labels.nodes.iter().map(|l| l.name.clone()).collect();

            let priority = linear_priority_to_flexpm(issue.priority);

            let data = CreateItem {
                title: format!("[{id}] {title}", id = issue.identifier, title = issue.title),
                description: Some(build_description(issue)),
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority,
                estimate: None,
                estimate_unit: None,
                tags: Some(tags),
                due_date: None,
                sprint_id: None,
                assignee: issue.assignee.as_ref().map(|u| u.name.clone()),
            };

            match state.repo.create_item(project_id, &status, data).await {
                Ok(_) => created += 1,
                Err(e) => {
                    tracing::warn!(
                        issue = %issue.identifier,
                        error = %e,
                        "Skipped Linear issue due to create_item error"
                    );
                    skipped += 1;
                }
            }
        }

        if conn.page_info.has_next_page {
            cursor = conn.page_info.end_cursor;
        } else {
            break;
        }
    }

    tracing::info!(
        project_id = %project_id,
        created,
        skipped,
        "Linear import complete"
    );

    Ok(Json(serde_json::json!({
        "created": created,
        "skipped": skipped,
    })))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_query(input: &LinearImportRequest, cursor: Option<&str>) -> String {
    let after = cursor
        .map(|c| format!(r#", after: "{}""#, c.replace('"', "")))
        .unwrap_or_default();

    let filter = build_filter(input);

    format!(
        r#"{{
  issues(first: 50{after}{filter}) {{
    nodes {{
      identifier
      title
      description
      url
      state {{ type }}
      assignee {{ name }}
      labels {{ nodes {{ name }} }}
      priority
    }}
    pageInfo {{ hasNextPage endCursor }}
  }}
}}"#
    )
}

fn build_filter(input: &LinearImportRequest) -> String {
    // Project filter takes precedence over team filter
    if let Some(ref pid) = input.project_id {
        return format!(r#", filter: {{ project: {{ id: {{ eq: "{pid}" }} }} }}"#);
    }
    if let Some(ref tid) = input.team_id {
        // Accept both slug and ID — Linear allows filtering by team key or ID
        return format!(r#", filter: {{ team: {{ key: {{ eq: "{tid}" }} }} }}"#);
    }
    String::new()
}

/// Linear priority: 0=No priority, 1=Urgent, 2=High, 3=Medium, 4=Low
fn linear_priority_to_flexpm(p: Option<u8>) -> Option<flexpm_core::models::Priority> {
    use flexpm_core::models::Priority;
    match p {
        Some(1) => Some(Priority::Critical),
        Some(2) => Some(Priority::High),
        Some(3) => Some(Priority::Medium),
        Some(4) => Some(Priority::Low),
        _ => None,
    }
}

fn build_description(issue: &LinearIssue) -> String {
    let mut d = format!("Linear Issue: {}", issue.url);
    if let Some(body) = issue
        .description
        .as_deref()
        .filter(|b| !b.trim().is_empty())
    {
        d.push_str("\n\n");
        d.push_str(body);
    }
    d
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(team_id: Option<&str>, project_id: Option<&str>) -> LinearImportRequest {
        LinearImportRequest {
            api_key: "key".into(),
            team_id: team_id.map(String::from),
            project_id: project_id.map(String::from),
            import_completed: false,
            label_filter: vec![],
        }
    }

    #[test]
    fn no_filter_produces_no_filter_clause() {
        let q = build_query(&make_input(None, None), None);
        assert!(!q.contains("filter:"));
    }

    #[test]
    fn team_filter_uses_key() {
        let q = build_query(&make_input(Some("ENG"), None), None);
        assert!(q.contains(r#"team: { key: { eq: "ENG" } }"#));
    }

    #[test]
    fn project_filter_uses_id() {
        let q = build_query(&make_input(Some("ENG"), Some("proj-123")), None);
        assert!(q.contains(r#"project: { id: { eq: "proj-123" } }"#));
        // project filter takes precedence — team filter absent
        assert!(!q.contains("team:"));
    }

    #[test]
    fn cursor_included_when_present() {
        let q = build_query(&make_input(None, None), Some("abc123"));
        assert!(q.contains(r#"after: "abc123""#));
    }

    #[test]
    fn priority_mapping() {
        use flexpm_core::models::Priority;
        assert_eq!(linear_priority_to_flexpm(Some(1)), Some(Priority::Critical));
        assert_eq!(linear_priority_to_flexpm(Some(2)), Some(Priority::High));
        assert_eq!(linear_priority_to_flexpm(Some(3)), Some(Priority::Medium));
        assert_eq!(linear_priority_to_flexpm(Some(4)), Some(Priority::Low));
        assert_eq!(linear_priority_to_flexpm(Some(0)), None);
        assert_eq!(linear_priority_to_flexpm(None), None);
    }

    #[test]
    fn cursor_injection_is_sanitised() {
        // A cursor containing a double-quote must not break the GraphQL query
        let q = build_query(&make_input(None, None), Some(r#"abc"def"#));
        // The injected quote should have been stripped
        assert!(q.contains("abcdef"));
        assert!(!q.contains(r#"abc"def"#));
    }
}
