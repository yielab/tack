use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde::Serialize;

use crate::config::Config;

pub struct TackClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl TackClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            token: config.token.clone(),
        })
    }

    pub fn get(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::GET, path, &[]).send()?;
        extract(resp)
    }

    /// GET returning the parsed body plus the response's `ETag` header, when
    /// the server sent one. Plain `get` throws headers away entirely, which
    /// is fine for read-only callers — but the MCP write path (`mcp.rs`)
    /// needs the ETag from a fresh read to send back as `If-Match` on the
    /// write that follows. Losing it here would silently reopen the exact
    /// read-then-write race `If-Match` exists to close, one layer up from
    /// where it looks closed. Returns `None` for the ETag (never an error)
    /// when the server doesn't send one — today's server, and any provider
    /// route this client hits before optimistic concurrency lands there —
    /// so a caller threading it into a later write degrades to sending no
    /// `If-Match`, i.e. today's unconditional-write behavior, exactly.
    pub fn get_with_etag(&self, path: &str) -> anyhow::Result<(serde_json::Value, Option<String>)> {
        let resp = self.request(reqwest::Method::GET, path, &[]).send()?;
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let value = extract(resp)?;
        Ok((value, etag))
    }

    pub fn post<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    pub fn patch<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PATCH, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    /// PATCH with an optional `If-Match` precondition. `if_match` is
    /// normally the ETag `get_with_etag` returned for the same resource a
    /// moment earlier; passing `None` sends no header at all and behaves
    /// exactly like plain `patch` — an absent precondition must preserve
    /// today's unconditional-write behavior, not fail closed, so an older
    /// server (or any route that doesn't version its rows yet) keeps
    /// working unchanged.
    ///
    /// A `412 Precondition Failed` is reported as a distinct, actionable
    /// error rather than falling through to the generic `{status}:
    /// {message}` shape `extract` produces for every other status: the
    /// caller raced a concurrent write to the same row (a human in the UI,
    /// another agent, or the reconciler) and needs to be told to re-read,
    /// not handed a message it can't distinguish from "the server broke."
    /// An MCP tool that can't tell those apart retries blindly and clobbers
    /// whatever won the race.
    pub fn patch_if_match<T: Serialize>(
        &self,
        path: &str,
        body: &T,
        if_match: Option<&str>,
    ) -> anyhow::Result<serde_json::Value> {
        let headers: [(&str, &str); 1] = match if_match {
            Some(tag) => [("If-Match", tag)],
            None => return self.patch(path, body),
        };
        let resp = self
            .request(reqwest::Method::PATCH, path, &headers)
            .json(body)
            .send()?;
        if resp.status() == StatusCode::PRECONDITION_FAILED {
            anyhow::bail!(
                "412 Precondition Failed: this item changed since it was last read — \
                 re-read it (get_item) and retry with the current data instead of \
                 resending the same change"
            );
        }
        extract(resp)
    }

    /// GET a binary response (e.g. a file download).
    pub fn get_bytes(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self
            .request(reqwest::Method::GET, path, &[])
            .timeout(std::time::Duration::from_secs(120))
            .send()?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp.bytes()?.to_vec())
        } else {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            anyhow::bail!("{}: {}", status, error_msg(&body))
        }
    }

    /// POST raw bytes and return the JSON response.
    pub fn post_bytes(&self, path: &str, data: Vec<u8>) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path, &[])
            .header("Content-Type", "application/octet-stream")
            .body(data)
            .send()?;
        extract(resp)
    }

    pub fn delete(&self, path: &str) -> anyhow::Result<()> {
        let resp = self.request(reqwest::Method::DELETE, path, &[]).send()?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body: serde_json::Value = resp.json().unwrap_or_default();
        anyhow::bail!("{}: {}", status, error_msg(&body))
    }

    pub fn delete_json(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::DELETE, path, &[]).send()?;
        extract(resp)
    }

    pub fn put_empty(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PUT, path, &[])
            .header("Content-Length", "0")
            .send()?;
        extract(resp)
    }

    pub fn put_json<T: Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PUT, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    /// Build a request with the auth header (if configured) plus any extra
    /// headers the caller supplies. Before this card there was no way to
    /// attach a header at all here, so every MCP write went out with no
    /// `If-Match` and was unconditionally last-write-wins — precisely the
    /// agent-versus-human race card G3 added `If-Match` to catch, on
    /// exactly the path that couldn't send it.
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        extra_headers: &[(&str, &str)],
    ) -> reqwest::blocking::RequestBuilder {
        let url = format!("{}/api{}", self.base_url, path);
        let mut req = self.client.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        for (name, value) in extra_headers {
            req = req.header(*name, *value);
        }
        req
    }
}

fn extract(resp: Response) -> anyhow::Result<serde_json::Value> {
    let status = resp.status();
    // Parse body regardless of status so we can show the server's error message
    let body: serde_json::Value = resp.json().unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        anyhow::bail!("{}: {}", status, error_msg(&body))
    }
}

fn error_msg(body: &serde_json::Value) -> String {
    body.get("error")
        .or_else(|| body.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("server error")
        .to_string()
}

// ── Connection check ──────────────────────────────────────────────────────────

pub fn check_connection(config: &Config) -> anyhow::Result<()> {
    let client = TackClient::new(config)?;
    client.get("/health").map_err(|_| {
        anyhow::anyhow!(
            "Cannot reach Tack API at {}\n\
             Make sure the server is running: cargo run -p tack-cli -- serve",
            config.base_url
        )
    })?;
    Ok(())
}

// ── Response helpers ──────────────────────────────────────────────────────────

/// Format a status code for user-facing output
pub fn status_label(code: StatusCode) -> &'static str {
    match code.as_u16() {
        400 => "Bad request",
        401 => "Unauthorized (check TACK_API_TOKEN)",
        403 => "Forbidden",
        404 => "Not found",
        409 => "Conflict",
        412 => "Precondition failed (item changed — re-read and retry)",
        422 => "Validation failed",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::Config;

    fn client_for(base_url: &str) -> TackClient {
        TackClient::new(&Config {
            base_url: base_url.to_string(),
            token: None,
        })
        .unwrap()
    }

    // Run a blocking closure in a thread that is allowed to block — the
    // client is a blocking `reqwest` client, and `wiremock`'s server runs on
    // the same tokio runtime the test does, so calling it directly from the
    // `#[tokio::test]` body would block that runtime's own worker thread.
    async fn run_blocking<F, T>(f: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(f)
            .await
            .expect("blocking task panicked")
    }

    #[tokio::test]
    async fn get_with_etag_reads_the_response_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/items/x"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"3\"")
                    .set_body_json(serde_json::json!({ "id": "x" })),
            )
            .mount(&server)
            .await;

        let uri = server.uri();
        let (value, etag) = run_blocking(move || client_for(&uri).get_with_etag("/items/x"))
            .await
            .unwrap();

        assert_eq!(value["id"], "x");
        assert_eq!(etag.as_deref(), Some("\"3\""));
    }

    #[tokio::test]
    async fn get_with_etag_tolerates_a_response_with_no_etag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/items/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let uri = server.uri();
        let (_, etag) = run_blocking(move || client_for(&uri).get_with_etag("/items/x"))
            .await
            .unwrap();

        assert_eq!(etag, None);
    }

    #[tokio::test]
    async fn patch_if_match_sends_the_if_match_header() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/items/x"))
            .and(header("If-Match", "\"7\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let uri = server.uri();
        let result = run_blocking(move || {
            client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), Some("\"7\""))
        })
        .await;

        assert!(result.is_ok(), "{result:?}");
    }

    /// `None` must round-trip to plain `patch` — an absent precondition
    /// preserves today's unconditional-write behavior exactly (D4). The
    /// mock rejects any request that *does* carry `If-Match`, so this fails
    /// loudly (500, no matching mock) rather than passing vacuously if a
    /// future edit starts sending an empty-string header instead of
    /// omitting it.
    #[tokio::test]
    async fn patch_if_match_sends_no_header_when_none_is_given() {
        struct NoIfMatch;
        impl wiremock::Match for NoIfMatch {
            fn matches(&self, request: &wiremock::Request) -> bool {
                !request.headers.contains_key("if-match")
            }
        }

        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/items/x"))
            .and(NoIfMatch)
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let uri = server.uri();
        let result = run_blocking(move || {
            client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), None)
        })
        .await;

        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn patch_if_match_maps_412_to_a_conflict_specific_message() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/items/x"))
            .respond_with(ResponseTemplate::new(412).set_body_json(serde_json::json!({
                "error": { "status": 412, "message": "version mismatch" }
            })))
            .mount(&server)
            .await;

        let uri = server.uri();
        let result = run_blocking(move || {
            client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), Some("\"1\""))
        })
        .await;

        let err = result
            .expect_err("412 must surface as an error")
            .to_string();
        assert!(err.contains("412"), "unexpected message: {err}");
        assert!(
            err.to_lowercase().contains("re-read"),
            "message must tell the caller to re-read, not just fail: {err}"
        );
    }

    #[test]
    fn status_label_covers_412() {
        assert_eq!(
            status_label(StatusCode::PRECONDITION_FAILED),
            "Precondition failed (item changed — re-read and retry)"
        );
    }
}
