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
        let resp = self.request(reqwest::Method::GET, path).send()?;
        extract(resp)
    }

    pub fn post<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()?;
        extract(resp)
    }

    pub fn patch<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PATCH, path)
            .json(body)
            .send()?;
        extract(resp)
    }

    /// GET a binary response (e.g. a file download).
    pub fn get_bytes(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self
            .request(reqwest::Method::GET, path)
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
            .request(reqwest::Method::POST, path)
            .header("Content-Type", "application/octet-stream")
            .body(data)
            .send()?;
        extract(resp)
    }

    pub fn delete(&self, path: &str) -> anyhow::Result<()> {
        let resp = self.request(reqwest::Method::DELETE, path).send()?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body: serde_json::Value = resp.json().unwrap_or_default();
        anyhow::bail!("{}: {}", status, error_msg(&body))
    }

    pub fn delete_json(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::DELETE, path).send()?;
        extract(resp)
    }

    pub fn put_empty(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PUT, path)
            .header("Content-Length", "0")
            .send()?;
        extract(resp)
    }

    pub fn put_json<T: Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::PUT, path).json(body).send()?;
        extract(resp)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        let url = format!("{}/api{}", self.base_url, path);
        let mut req = self.client.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
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
             Make sure the server is running: cargo run --bin tack-api",
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
        422 => "Validation failed",
        _ => "Error",
    }
}
