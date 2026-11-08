use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde::Serialize;

use crate::config::Config;

pub struct TackClient {
    client: Client,
    base_url: String,
    token: Option<String>,
    /// Sends `X-Tack-Client: mcp` on every call, so the board knows an agent made it.
    mcp: bool,
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
            mcp: false,
        })
    }

    /// Whether the token is an agent credential rather than the operator's. `tack mcp` uses it
    /// to send an agent's writes as change sets, the only way an agent changes the board.
    pub fn acts_as_agent(&self) -> bool {
        self.token
            .as_deref()
            .is_some_and(|token| token.starts_with(tack_app::runs::agent_credential::SECRET_PREFIX))
    }

    /// The same client, marking every call as made through `tack mcp`.
    pub fn marked_mcp(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            token: self.token.clone(),
            mcp: true,
        }
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
    /// when the server doesn't send one — a route without optimistic
    /// concurrency, or a server older than this client — so a caller
    /// threading it into a later write degrades to sending no `If-Match`,
    /// i.e. an unconditional write, exactly.
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

    /// PATCH with an optional `If-Match` precondition. `if_match` is normally
    /// the ETag `get_with_etag` returned for the same resource a moment
    /// earlier; passing `None` sends no header and behaves exactly like plain
    /// `patch` — an absent precondition preserves today's unconditional-write
    /// behavior rather than failing closed, so an older server keeps working
    /// unchanged.
    ///
    /// A `412 Precondition Failed` is reported as a distinct, actionable error
    /// rather than the generic `{status}: {message}` shape `extract` produces
    /// for every other status: the caller raced a concurrent write to the same
    /// row and needs to be told to re-read, not handed a message it can't
    /// distinguish from "the server broke" — an MCP tool that can't tell those
    /// apart retries blindly and clobbers whatever won the race.
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

    /// POST a text body of the given content type, with query parameters,
    /// and return the JSON response.
    pub fn post_text(
        &self,
        path: &str,
        query: &[(&str, &str)],
        content_type: &str,
        body: String,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(
                reqwest::Method::POST,
                path,
                &[("Content-Type", content_type)],
            )
            .query(query)
            .body(body)
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
    /// headers the caller supplies. Without `extra_headers`, every MCP write
    /// would go out with no `If-Match` and be unconditionally
    /// last-write-wins — exactly the agent-versus-human race `If-Match`
    /// exists to catch, on exactly the path that couldn't send it.
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
        if self.mcp {
            req = req.header("X-Tack-Client", "mcp");
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

/// Every route answers an error with the envelope `{"error": {"code": "...", "message": "...",
/// "details": {...}}}`: the line is `code: message`, so `idempotency_conflict`,
/// `invalid_transition`, `stale_lease` and `conflict` read as distinct outcomes a script can grep
/// for.
fn error_msg(body: &serde_json::Value) -> String {
    let error = &body["error"];
    match (error["code"].as_str(), error["message"].as_str()) {
        (Some(code), Some(message)) => format!("{code}: {message}"),
        (None, Some(message)) => message.to_string(),
        _ => "server error".to_string(),
    }
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
