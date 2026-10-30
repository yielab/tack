//! The calls this harness makes to docket's operator API (version 6), one
//! function per route. Each returns the status and the JSON body as docket
//! sent them; the caller reads the body through [`Reply::read`], into the
//! shapes of `wire.rs`. The token is read from its
//! file on every call, so a daemon restarted with a new one needs no restart
//! here.

use std::{path::PathBuf, time::Duration};

use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::wire::ErrorBody;
use crate::config::DocketConfig;

/// How long one request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// What docket answered.
#[derive(Debug, Clone)]
pub(super) struct Reply {
    pub status: u16,
    pub body: Value,
}

impl Reply {
    /// docket's `{"error": "<sentence>"}`, or the status when it sent none.
    pub fn error(&self) -> String {
        self.read::<ErrorBody>()
            .ok()
            .and_then(|body| body.error)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| format!("docket answered {}", self.status))
    }

    /// The body as the shape the caller expects of this route, or the sentence saying it is not.
    pub fn read<T: DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.body.clone())
            .map_err(|why| format!("docket's answer is not what its API describes: {why}"))
    }
}

pub(super) struct Client {
    http: reqwest::Client,
    base: String,
    token_file: Option<PathBuf>,
}

impl Client {
    pub fn new(config: &DocketConfig) -> Self {
        Self {
            // docket is on this machine: a proxy in the environment has no
            // business between the two.
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .no_proxy()
                .build()
                .unwrap_or_default(),
            base: config.url(),
            token_file: config.token_file.clone(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn token(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.token_file.as_ref()?).ok()?;
        Some(text.trim().to_owned()).filter(|token| !token.is_empty())
    }

    async fn send(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Reply, String> {
        let mut url = Url::parse(&self.base)
            .map_err(|error| format!("the docket URL is not valid ({error})"))?;
        url.path_segments_mut()
            .map_err(|()| "the docket URL cannot carry a path".to_owned())?
            .extend(segments);
        let mut request = self.http.request(method, url);
        if !query.is_empty() {
            request = request.query(query);
        }
        if let Some(token) = self.token() {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("could not reach docket: {}", error.without_url()))?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(|error| {
            format!("docket's answer could not be read: {}", error.without_url())
        })?;
        Ok(Reply {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        })
    }

    /// `GET /status.json`, the only route that names the API version.
    pub async fn status(&self) -> Result<Reply, String> {
        self.send(Method::GET, &["status.json"], &[], None).await
    }

    /// `GET /pods`.
    pub async fn pods(&self) -> Result<Reply, String> {
        self.send(Method::GET, &["pods"], &[], None).await
    }

    /// `GET /pods/<pod>`.
    pub async fn pod(&self, pod: &str) -> Result<Reply, String> {
        self.send(Method::GET, &["pods", pod], &[], None).await
    }

    /// `POST /pods` with `ensure`: `200` when the pod exists, `201` when made.
    pub async fn ensure_pod(&self, pod: &str, location: &str) -> Result<Reply, String> {
        // No shape is named: docket's own default is the code team, whatever it calls it.
        let body = json!({"name": pod, "location": location, "ensure": true});
        self.send(Method::POST, &["pods"], &[], Some(body)).await
    }

    /// `GET /pods/<pod>/records`: the record sets the team declares.
    pub async fn records(&self, pod: &str) -> Result<Reply, String> {
        self.send(Method::GET, &["pods", pod, "records"], &[], None)
            .await
    }

    /// `GET /pods/<pod>/records/<set>?limit=&sql=`: one read-only `SELECT` over the set.
    pub async fn record_rows(
        &self,
        pod: &str,
        set: &str,
        limit: u32,
        sql: &str,
    ) -> Result<Reply, String> {
        let limit = limit.to_string();
        self.send(
            Method::GET,
            &["pods", pod, "records", set],
            &[("limit", limit.as_str()), ("sql", sql)],
            None,
        )
        .await
    }

    /// `POST /tasks/<pod>`.
    pub async fn enqueue_task(&self, pod: &str, body: Value) -> Result<Reply, String> {
        self.send(Method::POST, &["tasks", pod], &[], Some(body))
            .await
    }

    /// `GET /tasks/<pod>/<id>`.
    pub async fn task(&self, pod: &str, id: &str) -> Result<Reply, String> {
        self.send(Method::GET, &["tasks", pod, id], &[], None).await
    }

    /// `POST /tasks/<pod>/<id>/cancel`.
    pub async fn cancel_task(&self, pod: &str, id: &str, reason: &str) -> Result<Reply, String> {
        let body = json!({"reason": reason});
        self.send(Method::POST, &["tasks", pod, id, "cancel"], &[], Some(body))
            .await
    }

    /// `GET /tasks/<pod>/<id>/diff`.
    pub async fn task_diff(&self, pod: &str, id: &str) -> Result<Reply, String> {
        self.send(Method::GET, &["tasks", pod, id, "diff"], &[], None)
            .await
    }

    /// `GET /traces/<pod>?since=<cursor>`; an empty cursor reads from the start.
    pub async fn traces(&self, pod: &str, since: &str) -> Result<Reply, String> {
        let query: &[(&str, &str)] = if since.is_empty() {
            &[]
        } else {
            &[("since", since)]
        };
        self.send(Method::GET, &["traces", pod], query, None).await
    }

    /// `GET /escalations?since=<seq>`, narrowed to one task when `task` names it: every
    /// record past `since`, whatever its state, and `next`, the highest `seq` returned.
    pub async fn escalations(&self, since: u64, task: Option<&str>) -> Result<Reply, String> {
        let since = since.to_string();
        let mut query = vec![("since", since.as_str())];
        if let Some(task) = task {
            query.push(("task", task));
        }
        self.send(Method::GET, &["escalations"], &query, None).await
    }

    /// `GET /escalations/<id>`: the one record, which for a pending approval of a call that
    /// leaves carries the call itself. The list never does.
    pub async fn escalation(&self, id: &str) -> Result<Reply, String> {
        self.send(Method::GET, &["escalations", id], &[], None)
            .await
    }

    /// `POST /escalations/<id>`: the one route that settles a pause.
    pub async fn resolve(&self, id: &str, body: Value) -> Result<Reply, String> {
        self.send(Method::POST, &["escalations", id], &[], Some(body))
            .await
    }
}
