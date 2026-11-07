//! The operator's side of Tack's HTTP API, as JSON in and JSON out.
//!
//! The harness knows nothing of Tack's inside: it reads what the server answers, as a person
//! with a token would. That is also why the answers stay `Value`s: a field the harness does not
//! read cannot break it.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::world::{API_TOKEN, DECISION_TOKEN};

pub struct Api {
    base: String,
    client: reqwest::blocking::Client,
}

impl Api {
    pub fn new(base: &str) -> Result<Self> {
        Self::with_token(base, Some(API_TOKEN))
    }

    /// An operator of a server that is not a throwaway world: its own token, or none for a
    /// loopback server that asks for none.
    pub fn with_token(base: &str, token: Option<&str>) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = token {
            let mut value: reqwest::header::HeaderValue = format!("Bearer {token}").parse()?;
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            client,
        })
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let response = self.client.get(self.url(path)).send()?;
        read(path, response)
    }

    /// Like [`Api::get`], but a 404 is `None`: the thing is not there, which is an answer.
    pub fn get_optional(&self, path: &str) -> Result<Option<Value>> {
        let response = self.client.get(self.url(path)).send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        read(path, response).map(Some)
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self.client.post(self.url(path)).json(body).send()?;
        read(path, response)
    }

    pub fn put(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self.client.put(self.url(path)).json(body).send()?;
        read(path, response)
    }

    /// A POST that carries the decision token: only the operator answers a decision.
    pub fn post_as_decider(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(self.url(path))
            .header("x-tack-decision-token", DECISION_TOKEN)
            .json(body)
            .send()?;
        read(path, response)
    }

    /// The raw bytes behind `path` (an artifact's content).
    pub fn bytes(&self, path: &str) -> Result<Vec<u8>> {
        let response = self.client.get(self.url(path)).send()?;
        if !response.status().is_success() {
            bail!("GET {path}: {}", response.status());
        }
        Ok(response.bytes()?.to_vec())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

fn read(path: &str, response: reqwest::blocking::Response) -> Result<Value> {
    let status = response.status();
    let text = response.text().with_context(|| format!("read {path}"))?;
    if !status.is_success() {
        bail!(
            "{path}: {status} {}",
            text.chars().take(300).collect::<String>()
        );
    }
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).with_context(|| format!("{path} did not answer JSON"))
}
