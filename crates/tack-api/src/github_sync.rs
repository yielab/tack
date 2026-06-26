//! Push-only GitHub status sync (Phase 21, v1).
//!
//! When a Tack item linked to a GitHub issue crosses the Done boundary, its
//! issue is closed (or reopened). This is best-effort and fire-and-forget: it
//! never blocks or fails the originating item update.

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
}
