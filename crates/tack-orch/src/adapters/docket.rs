//! `DocketAdapter` — the [`ControlPlane`] implementation for docket
//! (TODO.md §Wave 1, card A1 / task 33.3).
//!
//! # Constructor
//!
//! ```ignore
//! let adapter = DocketAdapter::new("http://127.0.0.1:7331", Some(token))?;
//! let plane: std::sync::Arc<dyn tack_orch::ControlPlane> = std::sync::Arc::new(adapter);
//! ```
//!
//! `new` takes the docket base URL (any trailing slash is normalized away)
//! and an optional Bearer token. A `None` token is a legitimate
//! configuration — every unauthenticated route (`/health`, `/status.json`,
//! `/metrics`) still works, and calling an authenticated route without one
//! degrades to whatever docket itself returns for a missing
//! `Authorization` header (a real 401, mapped to [`OrchError::Auth`]) rather
//! than being special-cased client-side, since that is both simpler and
//! exactly what a caller would observe by hand.
//!
//! # Auth split
//!
//! Per TODO.md §1.4: `/status.json`, `/metrics`, and `/health` never carry
//! a Bearer token, even if one is configured — every other route
//! (`/runs`, `/runs/{id}`, `/approvals`, `/tasks/{project}`,
//! `/traces/{project}`, plus the Wave-3 write routes) does. This is
//! enforced structurally by [`DocketAdapter::get_unauthed`] vs.
//! [`DocketAdapter::get_authed`] never sharing a code path that attaches
//! the header — a future edit can't accidentally leak the token onto an
//! unauthenticated request by adding one branch to a shared function.
//!
//! # Write methods
//!
//! **`enqueue_task` is implemented (card C1, Wave 3, 2026-08-05)** — see
//! below. [`ControlPlane::dispatch`] and [`ControlPlane::decide_approval`]
//! still return [`OrchError::Disabled`] unconditionally — **not** because
//! docket lacks the routes (`POST /dispatch/{project}` and
//! `POST /approvals/{token}` are both real, live-verified endpoints too,
//! card V1) but because C1's card scoped it to the per-item dispatch
//! primitive (`enqueue_task`) only; `dispatch` (a distinct pipeline-run
//! trigger, body = arbitrary `variables`) has no consumer in Tack yet and
//! `decide_approval` is Wave 4 / card D1's job (it also needs a second,
//! separate gate — `TACK_ORCH_APPROVAL_TOKEN` — that doesn't exist yet
//! either). Wiring either now, with no caller and no design for the
//! surrounding safety properties, would just be dead code.
//!
//! `enqueue_task`'s implementation deliberately does **not** parse
//! `status`/`approvalToken` off `POST /tasks/{project}`'s response body,
//! even though both are present on the wire (see "Verified live" below) —
//! [`ControlPlane::enqueue_task`]'s frozen signature
//! (`Result<String, OrchError>`) has nowhere to carry them out. The
//! Wave-3 dispatcher (`tack-api`'s `dispatcher` module) recovers that
//! information with one follow-up call to the already-fully-implemented
//! [`ControlPlane::list_tasks`], matching the just-created task by the id
//! this method returns. See that module's doc comment for the full
//! reasoning; this adapter only needs to get the id right.
//!
//! A `pre_input` policy **block** (HTTP 400) is mapped to
//! [`OrchError::Http`] with the message prefixed by
//! [`POLICY_BLOCK_PREFIX`] — `OrchError` has no dedicated variant for "the
//! control plane refused this on purpose" (its variant set is part of
//! TODO.md §1.1's Wave-0 freeze, a file this adapter doesn't own), so the
//! prefix is the private, documented contract `dispatcher` uses to tell a
//! deliberate policy refusal apart from every other `Http` failure.
//!
//! # Verified live (card V1, 2026-08-05)
//!
//! Every route this adapter (or a future Wave-3 write path) uses was
//! exercised against a real, isolated `docket serve` instance — not just
//! read from `serve.py`/`core/dispatch.py` source, which is as far as A1
//! and B2 could each go. Full detail lives in TODO.md §6's V1 handoff; the
//! two facts most load-bearing for whoever builds Wave 3 (card C1) are
//! recorded here because they directly contradict what TODO.md §1.4's table
//! says today:
//!
//! - **`POST /tasks/{project}`'s success response is `{"ok": true, "task":
//!   "<id>", "project": "...", "status": "pending"|"waiting_approval",
//!   "approvalToken"?: "..."}`, not `{"taskId": "..."}`.** TODO.md §1.4's
//!   summary line ("body `{description, priority, trusted}` → `{taskId}`")
//!   is wrong about the response — confirmed by a live capture, not a
//!   guess. The task id is under the key `"task"`, and a `require_approval`
//!   verdict adds `"approvalToken"` alongside `"status": "waiting_approval"`
//!   rather than a separate response shape. [`NewRemoteTask`] (the request
//!   body this adapter would send) is unaffected — only the response this
//!   adapter doesn't parse yet (because [`ControlPlane::enqueue_task`] is
//!   disabled) is wrong in the docs. Whoever wires this up for Wave 3 must
//!   not deserialize a `taskId` field — it doesn't exist on the wire.
//! - **The `pre_input` gate's three outcomes, and the `trusted` boundary,
//!   both behave exactly as TODO.md §1.4 describes** — confirmed against a
//!   real server, not assumed: a `block` verdict returns HTTP 400 with
//!   `{"ok": false, "error": "task rejected by guardrail policy '<id>' at
//!   enqueue: <message>"}`; a `require_approval` verdict returns HTTP 200
//!   with the task's real `status` (`"waiting_approval"`, never
//!   `"pending"`) and its `approvalToken`; and passing `trusted: false`
//!   explicitly in the request body genuinely flips a `prompt-injection`-id
//!   policy from silently skipped to evaluated — while omitting `trusted`
//!   entirely reproduces every existing caller's behavior (operator trust,
//!   the policy skipped) exactly as `core/dispatch.py::enqueue_task`'s
//!   docstring says. This is the prompt-injection boundary card C2 depends
//!   on; it is now confirmed real, not just read from source.
//!
//! # `list_tasks` / `traces`
//!
//! **Corrected 2026-08-05 (card B2, trace ingestion).** This section
//! originally said neither route existed in docket; that was true when A1
//! wrote it (Wave 1) but is stale — docket shipped both in its own Phase 22
//! (`GET /tasks/{project}` in P22-2, `GET /traces/{project}?since=` in
//! P22-3), verified directly against `serve.py`'s `do_GET`, not against
//! docket's own `ROADMAP.md` (which still lists them `TODO`). If either
//! route 404s against a real docket instance today, that means the plane is
//! running an older docket build, not that the endpoint is hypothetical —
//! [`OrchError::NotFound`] still surfaces exactly as before so callers
//! (B2's `reconciler::poll_traces`) can tell "this plane doesn't have the
//! capability yet" apart from a real outage ([`OrchError::Auth`]/
//! [`OrchError::Http`] are unaffected by any of this).
//!
//! **`traces`'s wire-format trap, verified against `serve.py`'s
//! `_traces_page`/`do_GET` directly (not the guessed shape
//! `crates/tack-orch/tests/fixtures/traces_list.json`'s provenance comment
//! flagged as unverified — that guess turned out wrong):** the real
//! response is `{"events": [...], "next": "<cursor>"}`, but `events` is an
//! array of **raw JSON strings**, not parsed objects — `_traces_page`
//! returns the verbatim JSONL lines `core.trace.export_lines` read off
//! disk, and `do_GET` calls `json.dumps` on that list of strings without
//! ever parsing them back into objects first. So every element of `events`
//! must be JSON-decoded a *second* time to reach the real event record
//! ([`TracesResponse`] below reflects this: `events: Vec<String>`, not
//! `Vec<RemoteEvent>`). `next` is docket's own minted resume cursor
//! (`serve.py`'s module comment above `_traces_page` documents the
//! compound `"<ts>Z:<n>"` format and why a bare last-seen timestamp isn't
//! enough) — the frozen [`ControlPlane::traces`] signature (§1.1) has
//! nowhere to carry a second return value out, so it is intentionally not
//! modeled on [`TracesResponse`] at all; `reconciler.rs` reconstructs the
//! equivalent cursor client-side from the returned events instead (see
//! that module's doc comment, "Trace cursor" section, for the mirrored
//! algorithm and why it's provably equivalent to computing `next` here and
//! throwing it away).

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tracing::warn;

use crate::adapters::prometheus;
use crate::{
    ControlPlane, FleetStatus, Health, MetricSample, NewRemoteTask, OrchError, RemoteApproval,
    RemoteEvent, RemoteRun, RemoteTask,
};

/// Every request this adapter makes gets this timeout — docket runs on
/// loopback in every deployment TODO.md describes, so 5s is generous for a
/// live plane and still fails fast against a hung/unreachable one rather
/// than blocking a reconciler poll tick indefinitely.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of a non-2xx response body to fold into an [`OrchError`]
/// message — enough to be useful in a log line, not enough to dump an
/// arbitrarily large or (in principle) sensitive body into `tracing` output.
const ERROR_BODY_SNIPPET_LEN: usize = 500;

/// Prefix on the [`OrchError::Http`] message [`DocketAdapter::enqueue_task`]
/// returns when `POST /tasks/{project}` refuses the request with a
/// `pre_input` guardrail-policy **block** (HTTP 400) — see the module doc's
/// "Write methods" section for why this exists instead of a dedicated
/// `OrchError` variant. The remainder of the message is docket's own
/// `error` text verbatim, which names the policy id
/// (`"task rejected by guardrail policy '<id>' at enqueue: <message>"`).
pub const POLICY_BLOCK_PREFIX: &str = "dispatch blocked by guardrail policy: ";

/// [`ControlPlane`] for a single docket instance. See the module doc for
/// the constructor, the auth split, and why the write methods and
/// `list_tasks`/`traces` behave the way they do.
pub struct DocketAdapter {
    client: Client,
    /// Always ends with exactly one trailing `/` (normalized in [`Self::new`])
    /// so [`Url::join`] resolves every route as a same-origin relative path.
    base_url: String,
    token: Option<String>,
}

impl DocketAdapter {
    /// Build an adapter for the docket instance at `base_url`
    /// (e.g. `"http://127.0.0.1:7331"` — a trailing slash is fine either
    /// way). `token` is docket's `/approvals` + `/runs` + `/dispatch` Bearer
    /// token (`DOCKET_SERVE_TOKEN`, or the value `docket serve` prints /
    /// writes to `--token-file` at startup) — `None` disables every
    /// authenticated route (see the module doc).
    ///
    /// Returns `Err` only if `reqwest::Client::builder().build()` itself
    /// fails (e.g. no usable TLS backend at runtime) — building a plain
    /// HTTP(S) client with just a timeout and a User-Agent essentially never
    /// fails in practice, but the constructor propagates it rather than
    /// panicking so a misconfigured host can never crash the process that
    /// registers a control plane.
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, OrchError> {
        let mut base_url = base_url.into();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(format!("tack-orch/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| OrchError::Http(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            client,
            base_url,
            token,
        })
    }

    /// Resolve `path` (relative, no leading `/`) against `self.base_url`.
    fn url(&self, path: &str) -> Result<Url, OrchError> {
        let base = Url::parse(&self.base_url)
            .map_err(|e| OrchError::Http(format!("invalid control-plane base URL: {e}")))?;
        base.join(path)
            .map_err(|e| OrchError::Http(format!("invalid route {path:?}: {e}")))
    }

    /// GET an unauthenticated route (`/health`, `/status.json`, `/metrics`).
    /// Never attaches the Bearer token — see the module doc's "Auth split".
    async fn get_unauthed(&self, path: &str) -> Result<reqwest::Response, OrchError> {
        let url = self.url(path)?;
        self.send(self.client.get(url)).await
    }

    /// GET an authenticated route, attaching `Authorization: Bearer <token>`
    /// when one is configured. With no token configured, the request still
    /// goes out (without the header) so docket's own 401 — mapped to
    /// [`OrchError::Auth`] by [`Self::send`] — is what the caller sees,
    /// rather than a client-side short-circuit that could drift from
    /// docket's actual auth behavior.
    async fn get_authed(&self, path: &str) -> Result<reqwest::Response, OrchError> {
        let url = self.url(path)?;
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        self.send(req).await
    }

    /// Send a request and classify the response: network failure →
    /// [`OrchError::Http`]; 401/403 → [`OrchError::Auth`]; 404 →
    /// [`OrchError::NotFound`] (message extracted from a `{"error": "..."}`
    /// JSON body when present, else the raw response text — docket's
    /// generic "route doesn't exist" 404 is plain text, not JSON, see the
    /// module doc); any other non-2xx → [`OrchError::Http`] with a
    /// truncated body snippet. A 2xx response is returned as-is for the
    /// caller to decode.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, OrchError> {
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::NotFound(message));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }
        Ok(resp)
    }

    /// Read the full response body and decode it as JSON, mapping both a
    /// body-read failure and a JSON decode failure to [`OrchError::Decode`]
    /// — from the caller's perspective both mean "docket sent something
    /// this adapter can't turn into the DTO it asked for".
    async fn decode_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, OrchError> {
        let text = resp
            .text()
            .await
            .map_err(|e| OrchError::Decode(format!("failed to read response body: {e}")))?;
        serde_json::from_str(&text).map_err(|e| {
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            OrchError::Decode(format!("{e} (body: {snippet})"))
        })
    }
}

/// docket's generic JSON error body: `{"ok": false, "error": "..."}`
/// (`serve.py`'s `_send_json_error`). `ok` is intentionally not modeled —
/// only `error` is ever read.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
}

/// `GET /runs` wraps the list in `{"runs": [...]}` (`serve.py`'s `do_GET`).
#[derive(Debug, Deserialize)]
struct RunsResponse {
    runs: Vec<RemoteRun>,
}

/// `GET /approvals` wraps the list in `{"pending": [...]}`.
#[derive(Debug, Deserialize)]
struct ApprovalsResponse {
    pending: Vec<RemoteApproval>,
}

/// `GET /tasks/{project}` — real wire shape, confirmed by a live HTTP
/// capture against a running `docket serve` (card V1, 2026-08-05; see the
/// module doc's "Verified live" section). Wrapper key really is
/// `{"tasks": [...]}`, matching `/runs`/`/approvals`'s own wrapping
/// convention exactly as A1 guessed — `tests/fixtures/tasks_list.json` is
/// now a genuine capture, not a derived projection.
/// `POST /tasks/{project}`'s success response — real wire shape, confirmed
/// by a live HTTP capture (card V1, 2026-08-05; see the module doc's
/// "Write methods" section). Only `task` is modeled: `ok`/`project` are
/// never read, and `status`/`approvalToken` — real fields, but this
/// method's frozen return type has nowhere to carry them — are recovered by
/// the caller via a follow-up [`ControlPlane::list_tasks`] instead (see the
/// module doc). An unknown/unmodeled JSON key costs nothing; `serde_json`
/// ignores it.
#[derive(Debug, Deserialize)]
struct EnqueueTaskResponse {
    task: String,
}

#[derive(Debug, Deserialize)]
struct TasksResponse {
    tasks: Vec<RemoteTask>,
}

/// `GET /traces/{project}` — real wire shape, verified against `serve.py`
/// (see the module doc's "list_tasks / traces" section for the full
/// wire-format trap this struct exists to route around). `events` is
/// **not** `Vec<RemoteEvent>`: each element is itself a raw JSON string
/// that must be decoded a second time — see [`DocketAdapter::traces`].
/// `next` (docket's own minted resume cursor) exists on the wire but is
/// deliberately not modeled here; nothing reads it because the frozen
/// `ControlPlane::traces` signature has nowhere to return it — an unknown
/// JSON key is simply ignored by `serde_json`, so omitting the field costs
/// nothing.
#[derive(Debug, Deserialize)]
struct TracesResponse {
    events: Vec<String>,
}

#[async_trait]
impl ControlPlane for DocketAdapter {
    fn kind(&self) -> &'static str {
        "docket"
    }

    async fn health(&self) -> Result<Health, OrchError> {
        let resp = self.get_unauthed("health").await?;
        Self::decode_json(resp).await
    }

    async fn status(&self) -> Result<FleetStatus, OrchError> {
        let resp = self.get_unauthed("status.json").await?;
        Self::decode_json(resp).await
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError> {
        let resp = self.get_unauthed("metrics").await?;
        // Text, not JSON — a malformed/truncated body degrades to whatever
        // the parser could salvage (never an error and never a panic; see
        // `adapters::prometheus`'s module doc), so only a transport-level
        // failure reaches this point as `Err`.
        let text = resp
            .text()
            .await
            .map_err(|e| OrchError::Decode(format!("failed to read metrics body: {e}")))?;
        Ok(prometheus::parse(&text))
    }

    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        let mut url = self.url("runs")?;
        if let Some(project) = project {
            url.query_pairs_mut().append_pair("project", project);
        }
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = self.send(req).await?;
        let wrapper: RunsResponse = Self::decode_json(resp).await?;
        Ok(wrapper.runs)
    }

    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError> {
        let path = format!("runs/{run_id}");
        let resp = self.get_authed(&path).await?;
        Self::decode_json(resp).await
    }

    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        let resp = self.get_authed("approvals").await?;
        let wrapper: ApprovalsResponse = Self::decode_json(resp).await?;
        Ok(wrapper.pending)
    }

    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        // Route confirmed live (card V1, 2026-08-05) — see the module doc's
        // "list_tasks / traces" section. A 404 still surfaces as
        // `OrchError::NotFound` via `Self::send`, same as any other 404 (a
        // real docket build old enough to lack this route, not a routing
        // bug on our side).
        let path = format!("tasks/{project}");
        let resp = self.get_authed(&path).await?;
        let wrapper: TasksResponse = Self::decode_json(resp).await?;
        Ok(wrapper.tasks)
    }

    async fn traces(
        &self,
        project: &str,
        since: Option<&str>,
    ) -> Result<Vec<RemoteEvent>, OrchError> {
        let mut url = self.url(&format!("traces/{project}"))?;
        if let Some(since) = since {
            url.query_pairs_mut().append_pair("since", since);
        }
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = self.send(req).await?;
        let wrapper: TracesResponse = Self::decode_json(resp).await?;

        // Each element of `wrapper.events` is a raw JSON string (see
        // TracesResponse's doc comment) — decode it a second time. A single
        // corrupt/unparseable line is dropped with a `warn!`, not treated as
        // a whole-page failure: docket's own trace store is append-only
        // JSONL written by many independent processes, so one bad line
        // should never take down every other event in the same poll.
        let events = wrapper
            .events
            .iter()
            .filter_map(|raw| match serde_json::from_str::<RemoteEvent>(raw) {
                Ok(event) => Some(event),
                Err(e) => {
                    warn!(
                        error = %e,
                        "dropping an unparseable trace event line from docket"
                    );
                    None
                }
            })
            .collect();
        Ok(events)
    }

    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError> {
        // POST /tasks/{project}, Bearer-authed (TODO.md §1.4). Built by hand
        // rather than through `get_authed`/`send` — those only ever GET, and
        // the `pre_input` policy **block** (HTTP 400) needs distinct
        // handling from `send`'s generic non-2xx branch (see the module doc
        // and `POLICY_BLOCK_PREFIX`).
        let url = self.url(&format!("tasks/{project}"))?;
        let mut req = self.client.post(url).json(&task);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::BAD_REQUEST {
            // A `pre_input` policy block — never a transport failure. Extract
            // docket's own `error` text (which names the policy id) the same
            // way `send`'s 404 branch does, and prefix it so `dispatcher` can
            // tell this apart from every other `Http` error.
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::Http(format!("{POLICY_BLOCK_PREFIX}{message}")));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }

        let parsed: EnqueueTaskResponse = Self::decode_json(resp).await?;
        Ok(parsed.task)
    }

    async fn dispatch(
        &self,
        _project: &str,
        _vars: serde_json::Value,
    ) -> Result<String, OrchError> {
        // Gated behind TACK_ORCH_ENABLE and owned by Wave 3 — see the
        // module doc's "Write methods" section. Unlike `enqueue_task`,
        // `POST /dispatch/{project}` is a real, working docket route today;
        // it stays disabled here purely for the gate, not for lack of a
        // server-side endpoint.
        Err(OrchError::Disabled)
    }

    async fn decide_approval(&self, _token: &str, _grant: bool) -> Result<(), OrchError> {
        // Gated behind TACK_ORCH_ENABLE and owned by Wave 3 (card D1 wires
        // the actual approve/deny UI + a separate TACK_ORCH_APPROVAL_TOKEN
        // gate on top of this) — see the module doc's "Write methods"
        // section. `POST /approvals/{token}` is a real, working docket
        // route today; same "gated, not missing" note as `dispatch`.
        Err(OrchError::Disabled)
    }
}
