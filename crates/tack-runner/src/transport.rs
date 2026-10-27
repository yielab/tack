//! The real HTTP transport for runner protocol v1.
//!
//! Without this module, [`crate::UnavailableProtocolClient`] is the only production
//! [`RunnerProtocolClient`] in the tree and `reqwest` is not a dependency of this
//! crate — so a packaged `tack-runner` binary could not enroll, claim, heartbeat or
//! report against a live server.
//!
//! [`HttpPullProtocol`] implements [`PullProtocol`] (the eight engine-facing
//! operations) and [`AttemptDataProtocol`] (events, decisions, decision polling,
//! artifact manifests, artifact content) — together the fourteen `/api/runner/v1`
//! routes. [`HttpRunnerClient`] implements [`RunnerProtocolClient`]: the daemon loop
//! that enrolls (or resumes a persisted session), replays unresolved journal records,
//! then claims and heartbeats until shutdown. `docs/contracts/runner-v1/` is the
//! authority for every payload here; where a response body and a fixture disagree,
//! this module follows the fixture.
//!
//! Two conditions must both hold before anything is resent: the failure is retryable
//! ([`ProtocolClientError::is_retryable`], derived from `StableErrorCode::retryable`),
//! and the operation is replayable by construction — its payload carries an
//! idempotency key or it is a pure read. [`Idempotency::SingleUse`] marks the one
//! operation that fails both tests:
//! enrollment. Its token is redeemed exactly once server-side, so a response lost in
//! transit leaves an ambiguous state a resend cannot recover — reported as a typed
//! transport failure instead of retried.
//!
//! The enrollment token travels only in the enrollment request body; the runner
//! credential only in an `Authorization: Bearer` header. Neither is ever logged, put
//! in an error, or rendered via `Debug` — [`RunnerCredential`] and
//! [`crate::EnrollmentCredential`] redact structurally, and this module never calls
//! `expose()` outside the exact place the byte is written onto the wire.

use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode, header};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tack_protocol::execution as wire;
use tack_protocol::execution::{
    ClaimGranted, ClaimResponse, ExecutionState, ProtocolErrorEnvelope, ProtocolVersion,
    RecoveryObservationRequest, RecoveryObservationResponse, RunnerCapabilities, StableErrorCode,
};
use tokio::task::JoinSet;

use crate::{
    Clock, EnrollmentCredential, RunnerConfig, RunnerError, Shutdown,
    client::engine::{EngineError, LEASE_RENEWAL_INTERVAL, RunCycle, RunnerEngine},
    client::{
        AttemptId, AttemptLease, AttemptState, CancellationReport, CancellationResponse,
        ClaimRequest, ClaimRequestId, ClaimResult, ClaimedWork, CompletionRequest,
        CompletionResponse, EnrollmentRequest, EnrollmentResponse, EventBatchRequest,
        EventBatchResponse, FencingToken, HeartbeatRequest, HeartbeatResponse, ProtocolClientError,
        PullProtocol, RefreshRequest, RefreshResponse, RunnerCredential, RunnerId,
        RunnerProtocolClient, RunnerSession, StartPhase, StartReport, Timestamp,
    },
};

/// `protocol.json`'s `base_path`. Fixed by the contract, so the client owns
/// it rather than requiring every operator to spell it into `--api-url`.
const PROTOCOL_BASE_PATH: &str = "/api/runner/v1";

/// How long an idle runner waits before it claims again: the server's
/// `retry_after_ms`, less what the claim itself already took, held between a
/// floor that keeps a server answering `0` from turning the loop hot and the
/// longest wait the contract allows.
fn idle_pause(retry_after: Duration, claim_took: Duration) -> Duration {
    const FLOOR: Duration = Duration::from_millis(250);
    retry_after
        .min(Duration::from_millis(CLAIM_WAIT_MS_MAX))
        .saturating_sub(claim_took)
        .max(FLOOR)
}

/// `limits.json`'s `claim_wait_ms_max`. A larger `wait` is rejected by the
/// server with `payload_too_large`, so the client clamps rather than sends a
/// request it already knows is invalid.
const CLAIM_WAIT_MS_MAX: u64 = 30_000;

/// Header the server reads for the fencing token on artifact content uploads,
/// where the body is raw bytes and the token cannot travel in JSON. Not part
/// of any frozen fixture — mirrored from
/// `crates/tack-api/src/handlers/runner_protocol.rs`'s own constant, which
/// documents it as that route's addition.
const ARTIFACT_FENCING_TOKEN_HEADER: &str = "x-tack-fencing-token";

/// Filename under `state_dir` holding the enrolled session. Owner-only.
const SESSION_FILE: &str = "session.json";

/// How much longer than the server-side long-poll window a claim request may
/// take before the client gives up on it.
const CLAIM_TIMEOUT_MARGIN: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------

/// Bounded retry. `max_attempts` counts total sends, so `1` disables retry
/// entirely; there is no unbounded mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    fn backoff_for(&self, already_sent: u32) -> Duration {
        let factor = 1_u32 << already_sent.saturating_sub(1).min(16);
        self.initial_backoff
            .saturating_mul(factor)
            .min(self.max_backoff)
    }
}

/// Whether an operation's payload makes a resend safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Idempotency {
    /// Carries an idempotency key, a fencing token or is a pure read.
    Replayable,
    /// Cannot be resent without risking an ambiguous outcome. Today this is
    /// enrollment alone (single-use token).
    SingleUse,
}

// ---------------------------------------------------------------------
// The client
// ---------------------------------------------------------------------

/// A real `/api/runner/v1` client.
pub struct HttpPullProtocol {
    http: Client,
    base_url: String,
    retry: RetryPolicy,
}

impl HttpPullProtocol {
    /// `api_base_url` is the protocol base (`.../api/runner/v1`); a trailing
    /// slash is tolerated. `request_timeout` bounds every non-claim call.
    pub fn new(
        api_base_url: &str,
        request_timeout: Duration,
        retry: RetryPolicy,
    ) -> Result<Self, RunnerError> {
        let http = Client::builder()
            .timeout(request_timeout)
            .connect_timeout(request_timeout.min(Duration::from_secs(10)))
            .user_agent(concat!("tack-runner/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| RunnerError::ProtocolTransport)?;
        Ok(Self {
            http,
            base_url: normalize_base_url(api_base_url),
            retry,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// One JSON exchange, with the two-condition retry discipline documented
    /// at module level.
    async fn send_json(
        &self,
        method: Method,
        path: &str,
        credential: Option<&RunnerCredential>,
        body: &(impl Serialize + ?Sized),
        idempotency: Idempotency,
        timeout: Option<Duration>,
    ) -> Result<Value, ProtocolClientError> {
        let url = self.url(path);
        let mut sent = 0_u32;
        loop {
            sent += 1;
            let mut request = self.http.request(method.clone(), &url).json(body);
            if let Some(credential) = credential {
                // The only place a runner credential is written onto the
                // wire. It is never formatted anywhere else.
                request = request.bearer_auth(credential.expose());
            }
            if let Some(timeout) = timeout {
                request = request.timeout(timeout);
            }
            let outcome = match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let bytes = response.bytes().await.unwrap_or_default();
                    if status.is_success() {
                        return serde_json::from_slice::<Value>(&bytes)
                            .map_err(|_| ProtocolClientError::Rejected);
                    }
                    map_error_body(status, &bytes)
                }
                // `reqwest`'s error carries the URL and can carry a source
                // chain; it is deliberately dropped rather than wrapped, so
                // nothing about the request can leak into a log line.
                Err(_) => ProtocolClientError::Transport,
            };

            let may_retry = matches!(idempotency, Idempotency::Replayable)
                && outcome.is_retryable()
                && sent < self.retry.max_attempts;
            if !may_retry {
                tracing::debug!(
                    operation = %path,
                    attempts = sent,
                    error = %outcome,
                    "runner protocol call failed"
                );
                return Err(outcome);
            }
            tokio::time::sleep(self.retry.backoff_for(sent)).await;
        }
    }
}

/// Resolves a configured URL to the protocol base.
///
/// `protocol.json` fixes `base_path: "/api/runner/v1"`, so the path is the
/// *client's* knowledge, not the operator's: `--api-url http://host:3210` and
/// `--api-url http://host:3210/api/runner/v1` must both work, and an operator
/// who only knows where the server listens must not have to memorise a
/// contract detail. Appending only when the suffix is absent keeps the
/// existing default (which spells it out) exactly as it was.
fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim_end_matches('/');
    if trimmed.ends_with(PROTOCOL_BASE_PATH) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}{PROTOCOL_BASE_PATH}")
    }
}

/// Maps an HTTP error response onto the typed enum.
///
/// The **body's** stable code wins over the status line: two different codes
/// share a status (`conflict` and `idempotency_conflict` are both 409;
/// `stale_lease` is 409 too), so branching on status alone would misreport
/// `stale_lease` as a generic conflict.
/// A body that is not a v1 envelope yields [`ProtocolClientError::Rejected`],
/// which claims no stable code at all rather than guessing one.
fn map_error_body(status: StatusCode, body: &[u8]) -> ProtocolClientError {
    match serde_json::from_slice::<ProtocolErrorEnvelope>(body) {
        Ok(envelope) => ProtocolClientError::from_stable_code(envelope.error.code),
        Err(_) => {
            if status.is_server_error() {
                // A 5xx with an unparseable body is still a server fault and
                // still retryable; typing it as `internal_error` would claim
                // the server said something it did not.
                ProtocolClientError::Transport
            } else {
                ProtocolClientError::Rejected
            }
        }
    }
}

/// An instant a harness adapter reported as text, as the wire carries it; one that is not an
/// RFC 3339 timestamp is refused here, the way the board would refuse it.
fn instant(timestamp: &Timestamp) -> Result<chrono::DateTime<chrono::Utc>, ProtocolClientError> {
    chrono::DateTime::parse_from_rfc3339(timestamp.as_str())
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
        .map_err(|_| ProtocolClientError::Protocol {
            code: StableErrorCode::InvalidRequest,
        })
}

/// An instant of the wire as the runner's text timestamp.
fn timestamp(instant: chrono::DateTime<chrono::Utc>) -> Timestamp {
    Timestamp::new(instant.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
}

fn typed<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, ProtocolClientError> {
    serde_json::from_value(value.clone()).map_err(|_| ProtocolClientError::Rejected)
}

/// Rejects a response whose `protocol_version` is not v1 rather than parsing
/// a payload whose semantics this client cannot vouch for. `protocol.json`
/// fixes `semantic_changes: require_new_protocol_version`, so a different
/// version is a genuinely different contract.
fn require_v1(value: &Value) -> Result<(), ProtocolClientError> {
    match value.get("protocol_version").and_then(Value::as_u64) {
        Some(1) => Ok(()),
        Some(_) => Err(ProtocolClientError::Protocol {
            code: StableErrorCode::UnsupportedProtocol,
        }),
        None => Err(ProtocolClientError::Rejected),
    }
}

/// Serializes a [`RunnerCapabilities`] into the *embedded* snapshot shape
/// `enrollment.request.json`/`refresh.request.json` carry under
/// `capabilities`. `runner_version` and `protocol_version` are siblings of
/// `capabilities` in those envelopes, never nested inside it (see
/// `EmbeddedCapabilitySnapshot`'s own doc comment in tack-orch), so they are
/// dropped here rather than emitted as extra keys the server would have to
/// absorb into its additive-field bucket.
fn embedded_capabilities(
    capabilities: &RunnerCapabilities,
) -> Result<wire::EmbeddedCapabilitySnapshot, ProtocolClientError> {
    let mut value =
        serde_json::to_value(capabilities).map_err(|_| ProtocolClientError::Rejected)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("runner_version");
        object.remove("protocol_version");
    }
    typed(&value)
}

fn attempt_state(state: ExecutionState) -> Result<AttemptState, ProtocolClientError> {
    AttemptState::from_execution(state).ok_or(ProtocolClientError::Rejected)
}

#[async_trait]
impl PullProtocol for HttpPullProtocol {
    async fn enroll(
        &self,
        enrollment_credential: &EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError> {
        let body = wire::EnrollmentRequest {
            protocol_version: ProtocolVersion::v1(),
            // The only place the enrollment token is written onto the wire.
            enrollment_token: enrollment_credential.expose().to_owned(),
            runner_name: request.runner_name,
            runner_version: request.runner_version,
            capabilities: embedded_capabilities(&request.capabilities)?,
        };
        let value = self
            .send_json(
                Method::POST,
                "/enroll",
                None,
                &body,
                Idempotency::SingleUse,
                None,
            )
            .await?;
        require_v1(&value)?;
        let granted: wire::EnrollmentResponse = typed(&value)?;
        Ok(EnrollmentResponse {
            session: RunnerSession::new(
                granted.runner_id,
                RunnerCredential::new(granted.runner_credential),
                timestamp(granted.credential_expires_at),
            ),
            heartbeat_interval: Duration::from_secs(granted.heartbeat_interval_seconds),
            lease_duration: Duration::from_secs(granted.lease_duration_seconds),
            server_time: timestamp(granted.server_time),
        })
    }

    async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, ProtocolClientError> {
        let body = wire::RefreshRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            runner_name: request.runner_name,
            runner_version: request.runner_version,
            rotate_credential: request.rotate_credential,
            capabilities: embedded_capabilities(&request.capabilities)?,
        };
        let value = self
            .send_json(
                Method::POST,
                "/refresh",
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let accepted: wire::RefreshResponse = typed(&value)?;
        // `refresh.response.json` carries `runner_credential: null` when no
        // rotation was requested. Retaining the current credential in that
        // case is the fixture's own documented meaning — not a fallback.
        let credential = match accepted.runner_credential {
            Some(rotated) => RunnerCredential::new(rotated),
            None => session.credential().clone(),
        };
        Ok(RefreshResponse {
            session: RunnerSession::new(
                accepted.runner_id,
                credential,
                timestamp(accepted.credential_expires_at),
            ),
            accepted_at: timestamp(accepted.accepted_at),
        })
    }

    async fn claim(
        &self,
        session: &RunnerSession,
        request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError> {
        let wait_ms = u64::try_from(request.wait.as_millis())
            .unwrap_or(CLAIM_WAIT_MS_MAX)
            .min(CLAIM_WAIT_MS_MAX);
        let body = wire::ClaimRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            claim_request_id: request.claim_request_id,
            available_capacity: request.available_capacity,
            wait_ms,
        };
        let value = self
            .send_json(
                Method::POST,
                "/claim",
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                Some(Duration::from_millis(wait_ms) + CLAIM_TIMEOUT_MARGIN),
            )
            .await?;
        require_v1(&value)?;
        match typed::<ClaimResponse>(&value)? {
            ClaimResponse::Idle(idle) => Ok(ClaimResult::NoWork {
                retry_after: Duration::from_millis(idle.retry_after_ms),
                reason: idle.reason,
            }),
            ClaimResponse::Work(granted) => {
                let ClaimGranted {
                    claim_request_id,
                    lease,
                    request,
                    attempt,
                    ..
                } = *granted;
                // `claim.response.json`'s `lease` carries neither `attempt_number`
                // nor `state`; both live on the attempt snapshot beside it. Building
                // the engine-facing `AttemptLease` from both halves is what makes
                // `ClaimedWork::workspace_repository`'s redundancy check meaningful
                // — it re-validates the two against each other before any local
                // side effect.
                let lease = AttemptLease {
                    attempt_id: lease.attempt_id,
                    runner_id: lease.runner_id,
                    fencing_token: lease.fencing_token,
                    attempt_number: attempt.attempt_number,
                    state: attempt_state(attempt.state)?,
                    issued_at: Timestamp::new(lease.issued_at.to_rfc3339()),
                    expires_at: Timestamp::new(lease.expires_at.to_rfc3339()),
                };
                Ok(ClaimResult::Work(Box::new(ClaimedWork {
                    claim_request_id,
                    lease,
                    request,
                    attempt,
                })))
            }
        }
    }

    async fn heartbeat(
        &self,
        session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError> {
        let value = self
            .send_json(
                Method::POST,
                "/heartbeat",
                Some(session.credential()),
                &request,
                Idempotency::Replayable,
                None,
            )
            .await?;
        typed(&value)
    }

    async fn report_start(
        &self,
        session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        // Both phases are required by the server to carry `workspace_id` and
        // `base_revision`. `StartReport` models them as optional because the
        // type predates that requirement; an absent value is reported as
        // a typed invalid_request instead of being sent as `null` for the
        // server to reject with a less specific message.
        let (workspace_id, base_revision) = match (&report.workspace_id, &report.base_revision) {
            (Some(workspace_id), Some(base_revision)) => (workspace_id, base_revision),
            _ => {
                return Err(ProtocolClientError::Protocol {
                    code: StableErrorCode::InvalidRequest,
                });
            }
        };
        let mut body = wire::AttemptTransitionRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            attempt_id: report.attempt_id.clone(),
            fencing_token: report.fencing_token,
            workspace_id: workspace_id.clone(),
            base_revision: base_revision.clone(),
            process_id: None,
            environment_names: Vec::new(),
            sandbox: None,
        };
        let path = match report.phase {
            StartPhase::Preparing => {
                format!("/attempts/{}/accept", report.attempt_id)
            }
            StartPhase::ProcessObservedRunning => {
                let Some(process_id) = report.process_id.as_deref() else {
                    return Err(ProtocolClientError::Protocol {
                        code: StableErrorCode::InvalidRequest,
                    });
                };
                body.process_id = Some(process_id.to_owned());
                // Empty is not sent.
                body.environment_names = report.environment_names.clone();
                body.sandbox = report.sandbox.clone();
                format!("/attempts/{}/start", report.attempt_id)
            }
        };
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        Ok(())
    }

    async fn report_completion(
        &self,
        session: &RunnerSession,
        report: CompletionRequest,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        let path = format!("/attempts/{}/completion", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &report,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        typed(&value)
    }

    async fn report_cancellation(
        &self,
        session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        let body = wire::CancellationObservationRequest {
            protocol_version: report.protocol_version,
            runner_id: report.runner_id.clone(),
            attempt_id: report.attempt_id.clone(),
            fencing_token: report.fencing_token,
            cancellation_request_id: report.cancellation_request_id.clone(),
            observation: report.observation,
            observed_at: instant(&report.observed_at)?,
            details: report.details.clone(),
        };
        let path = format!("/attempts/{}/cancellation-observation", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let observed: wire::CancellationObservationResponse = typed(&value)?;
        Ok(CancellationResponse {
            protocol_version: observed.protocol_version,
            attempt_id: observed.attempt_id,
            cancellation_request_id: observed.cancellation_request_id,
            state: attempt_state(observed.state)?,
            replayed: observed.replayed,
            committed_at: timestamp(observed.committed_at),
        })
    }

    async fn observe_recovery(
        &self,
        session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let body = serde_json::to_value(&report).map_err(|_| ProtocolClientError::Rejected)?;
        let path = format!("/attempts/{}/recovery-observation", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        typed(&value)
    }
}

// ---------------------------------------------------------------------
// Events, decisions and artifacts
// ---------------------------------------------------------------------

/// The five attempt-scoped operations [`PullProtocol`] does not model.
///
/// A **separate trait**, not new methods on `PullProtocol`: widening
/// `PullProtocol` would force edits to every test fake for methods some of
/// them never call. `engine.rs` calls `submit_events` and
/// `submit_terminal_evidence` (artifacts) through this trait;
/// `create_decision`/`poll_decisions` are implemented but still have no
/// caller — no harness adapter in this tree ever asks a question a decision
/// could answer.
#[async_trait]
pub trait AttemptDataProtocol: Send + Sync {
    async fn submit_events(
        &self,
        session: &RunnerSession,
        report: EventBatchRequest,
    ) -> Result<EventBatchResponse, ProtocolClientError>;

    async fn create_decision(
        &self,
        session: &RunnerSession,
        report: DecisionCreateReport,
    ) -> Result<DecisionCreateResponse, ProtocolClientError>;

    async fn poll_decisions(
        &self,
        session: &RunnerSession,
        report: DecisionPollReport,
    ) -> Result<DecisionPollResponse, ProtocolClientError>;

    async fn submit_artifact_manifest(
        &self,
        session: &RunnerSession,
        report: ArtifactManifestReport,
    ) -> Result<Vec<ArtifactUploadGrant>, ProtocolClientError>;

    /// Uploads the bytes for a manifest-accepted artifact, following the
    /// `method`/`path` the server returned in [`ArtifactUploadGrant`].
    async fn put_artifact_content(
        &self,
        session: &RunnerSession,
        fencing_token: FencingToken,
        grant: &ArtifactUploadGrant,
        media_type: Option<&str>,
        content: Vec<u8>,
    ) -> Result<(), ProtocolClientError>;
}

pub use wire::{DecisionAnswer, DecisionOption, DecisionRecommendation as Recommendation};

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCreateReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub decision_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
    pub recommendation: Option<Recommendation>,
    pub expires_at: Timestamp,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionCreateResponse {
    pub decision_id: String,
    pub state: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionPollReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    /// Required, as in `decision.poll.request.json`: the server refuses a
    /// poll with no cursor. "From the beginning" is an early timestamp.
    pub after: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedDecision {
    pub decision_id: String,
    pub state: String,
    #[serde(default)]
    pub answer: Option<DecisionAnswer>,
    #[serde(default)]
    pub resolved_at: Option<Timestamp>,
    #[serde(default)]
    pub resolved_by: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionPollResponse {
    pub decisions: Vec<ResolvedDecision>,
    #[serde(default)]
    pub next_after: Option<Timestamp>,
}

pub use wire::ArtifactManifestEntry as ArtifactManifestItem;

#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactManifestReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub artifacts: Vec<ArtifactManifestItem>,
}

/// The server's own upload grant. `path` and `method` are followed verbatim
/// rather than reconstructed — `artifact.response.json` records the grant as
/// data, and the server is the authority on its own route layout.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactUploadGrant {
    pub artifact_id: String,
    pub state: String,
    pub method: String,
    pub path: String,
    pub expires_at: Option<Timestamp>,
}

#[async_trait]
impl AttemptDataProtocol for HttpPullProtocol {
    async fn submit_events(
        &self,
        session: &RunnerSession,
        report: EventBatchRequest,
    ) -> Result<EventBatchResponse, ProtocolClientError> {
        let path = format!("/attempts/{}/events", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &report,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        typed(&value)
    }

    async fn create_decision(
        &self,
        session: &RunnerSession,
        report: DecisionCreateReport,
    ) -> Result<DecisionCreateResponse, ProtocolClientError> {
        let body = wire::DecisionCreateRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            attempt_id: report.attempt_id.clone(),
            fencing_token: report.fencing_token,
            decision_id: report.decision_id.clone(),
            kind: report.kind.clone(),
            prompt: report.prompt.clone(),
            options: report.options.clone(),
            recommendation: report.recommendation.clone(),
            expires_at: Some(instant(&report.expires_at)?),
            metadata: report.metadata.clone(),
        };
        let path = format!("/attempts/{}/decisions", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let created: wire::DecisionCreateResponse = typed(&value)?;
        Ok(DecisionCreateResponse {
            decision_id: created.decision_id,
            state: created.state,
            created_at: timestamp(created.created_at),
        })
    }

    async fn poll_decisions(
        &self,
        session: &RunnerSession,
        report: DecisionPollReport,
    ) -> Result<DecisionPollResponse, ProtocolClientError> {
        let body = wire::DecisionPollRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            attempt_id: report.attempt_id.clone(),
            fencing_token: report.fencing_token,
            after: instant(&report.after)?,
        };
        let path = format!("/attempts/{}/decisions/poll", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let polled: wire::DecisionPollResponse = typed(&value)?;
        Ok(DecisionPollResponse {
            decisions: polled
                .decisions
                .into_iter()
                .map(|update| ResolvedDecision {
                    decision_id: update.decision_id,
                    state: update.state,
                    answer: update.answer,
                    resolved_at: update.resolved_at.map(timestamp),
                    resolved_by: update.resolved_by,
                })
                .collect(),
            next_after: Some(timestamp(polled.next_after)),
        })
    }

    async fn submit_artifact_manifest(
        &self,
        session: &RunnerSession,
        report: ArtifactManifestReport,
    ) -> Result<Vec<ArtifactUploadGrant>, ProtocolClientError> {
        let body = wire::ArtifactManifestRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            attempt_id: report.attempt_id.clone(),
            fencing_token: report.fencing_token,
            artifacts: report.artifacts.clone(),
        };
        let path = format!("/attempts/{}/artifacts", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let accepted: wire::ArtifactManifestResponse = typed(&value)?;
        Ok(accepted
            .artifacts
            .into_iter()
            .map(|grant| ArtifactUploadGrant {
                artifact_id: grant.artifact_id,
                state: grant.state,
                method: grant.upload.method,
                path: grant.upload.path,
                expires_at: grant.upload.expires_at.map(timestamp),
            })
            .collect())
    }

    async fn put_artifact_content(
        &self,
        session: &RunnerSession,
        fencing_token: FencingToken,
        grant: &ArtifactUploadGrant,
        media_type: Option<&str>,
        content: Vec<u8>,
    ) -> Result<(), ProtocolClientError> {
        let method = Method::from_bytes(grant.method.as_bytes())
            .map_err(|_| ProtocolClientError::Rejected)?;
        // The grant's `path` is server-absolute (`/api/runner/v1/...`), so it
        // is joined to the origin, not to the protocol base.
        let url = origin_of(&self.base_url)? + &grant.path;
        let mut request = self
            .http
            .request(method, &url)
            .bearer_auth(session.credential().expose())
            .header(ARTIFACT_FENCING_TOKEN_HEADER, fencing_token.0.to_string())
            .body(content);
        if let Some(media_type) = media_type {
            request = request.header(header::CONTENT_TYPE, media_type);
        }
        // Deliberately no retry: the server refuses a second PUT for an
        // artifact whose content is already recorded (content is immutable
        // once verified), so a resend after an ambiguous first send would
        // report `conflict` and mask a success. The manifest exchange above
        // is the replayable half; the byte upload is not.
        match request.send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let status = response.status();
                let bytes = response.bytes().await.unwrap_or_default();
                Err(map_error_body(status, &bytes))
            }
            Err(_) => Err(ProtocolClientError::Transport),
        }
    }
}

/// Extracts `scheme://host[:port]` from the configured base URL, without a
/// URL-parsing dependency: the origin ends at the first `/` after `://`.
fn origin_of(base_url: &str) -> Result<String, ProtocolClientError> {
    let scheme_end = base_url.find("://").ok_or(ProtocolClientError::Rejected)? + 3;
    let rest = &base_url[scheme_end..];
    let authority_len = rest.find('/').unwrap_or(rest.len());
    Ok(base_url[..scheme_end + authority_len].to_owned())
}

// ---------------------------------------------------------------------
// Persisted session
// ---------------------------------------------------------------------

/// The enrolled identity, persisted so a restarted runner resumes instead of
/// burning a second single-use enrollment token (which an operator would have
/// to issue by hand). Written owner-only; the file holds a live credential.
#[derive(Serialize, Deserialize)]
struct PersistedSession {
    runner_id: String,
    runner_credential: String,
    credential_expires_at: String,
}

fn session_path(state_dir: &Path) -> PathBuf {
    state_dir.join(SESSION_FILE)
}

fn load_session(state_dir: &Path) -> Option<RunnerSession> {
    let raw = fs::read_to_string(session_path(state_dir)).ok()?;
    let persisted: PersistedSession = serde_json::from_str(&raw).ok()?;
    Some(RunnerSession::new(
        RunnerId::new(persisted.runner_id),
        RunnerCredential::new(persisted.runner_credential),
        Timestamp::new(persisted.credential_expires_at),
    ))
}

/// The runner id a persisted session under `state_dir` claims, without
/// validating whether the server this process talks to still recognises
/// it — only `establish_session`'s own `refresh` call settles that.
/// `None` when no session is persisted there, or its contents cannot be
/// parsed; a caller with no persisted session to compare has nothing to
/// distinguish, and a caller of this function decides what "no answer"
/// means for its own case rather than this reporting a guess.
pub fn persisted_session_runner_id(state_dir: &Path) -> Option<String> {
    load_session(state_dir).map(|session| session.runner_id.as_str().to_owned())
}

fn store_session(state_dir: &Path, session: &RunnerSession) -> Result<(), RunnerError> {
    let persisted = PersistedSession {
        runner_id: session.runner_id.as_str().to_owned(),
        runner_credential: session.credential().expose().to_owned(),
        credential_expires_at: session.credential_expires_at().as_str().to_owned(),
    };
    let encoded = serde_json::to_string(&persisted).map_err(|_| RunnerError::Filesystem)?;
    let path = session_path(state_dir);
    let temporary = path.with_extension("json.tmp");
    write_owner_only(&temporary, encoded.as_bytes())?;
    fs::rename(&temporary, &path).map_err(|_| RunnerError::Filesystem)?;
    Ok(())
}

#[cfg(unix)]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::os::unix::fs::OpenOptionsExt;

    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| RunnerError::Filesystem)?;
    file.write_all(bytes).map_err(|_| RunnerError::Filesystem)?;
    file.sync_all().map_err(|_| RunnerError::Filesystem)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    fs::write(path, bytes).map_err(|_| RunnerError::Filesystem)
}

// ---------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------

/// How often an idle runner asks its harnesses again what they report, so a team
/// or an agent added after it started reaches the board without a restart.
pub const CAPABILITY_REPROBE_INTERVAL: Duration = Duration::from_secs(300);

/// The production [`RunnerProtocolClient`]: enroll (or resume), replay
/// unresolved journal records, then claim and heartbeat until shutdown.
pub struct HttpRunnerClient<A, W, C> {
    protocol: Arc<HttpPullProtocol>,
    engine: Arc<RunnerEngine<Arc<HttpPullProtocol>, A, W, C>>,
    config: RunnerConfig,
    clock: C,
    capabilities: RunnerCapabilities,
    sequence: AtomicU64,
    /// How often the one ticker heartbeats every lease held.
    renewal_interval: Duration,
    /// How often, and how, the harnesses are probed again; `None` reports once.
    #[allow(clippy::type_complexity)] // an interval and one boxed async probe
    reprobe: Option<(
        Duration,
        Box<dyn Fn() -> Pin<Box<dyn Future<Output = RunnerCapabilities> + Send>> + Send + Sync>,
    )>,
}

impl<A, W, C> HttpRunnerClient<A, W, C>
where
    A: crate::client::HarnessAdapter,
    W: crate::client::WorktreeProvisioner,
    C: Clock,
{
    pub fn new(
        protocol: Arc<HttpPullProtocol>,
        engine: RunnerEngine<Arc<HttpPullProtocol>, A, W, C>,
        config: RunnerConfig,
        clock: C,
        capabilities: RunnerCapabilities,
    ) -> Self {
        Self {
            protocol,
            engine: Arc::new(engine),
            config,
            clock,
            capabilities,
            sequence: AtomicU64::new(0),
            renewal_interval: LEASE_RENEWAL_INTERVAL,
            reprobe: None,
        }
    }

    /// Probes the harnesses again every `interval` and after each attempt, and
    /// sends a refresh when the report differs from the last one sent.
    #[must_use]
    pub fn with_reprobe<F, Fut>(mut self, interval: Duration, probe: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RunnerCapabilities> + Send + 'static,
    {
        self.reprobe = Some((interval, Box::new(move || Box::pin(probe()))));
        self
    }

    /// A shorter heartbeat period, so a test need not wait out the real one.
    #[cfg(test)]
    fn with_renewal_interval(mut self, interval: Duration) -> Self {
        self.renewal_interval = interval;
        self
    }

    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::<chrono::Utc>::from(self.clock.now())
    }

    fn now_rfc3339(&self) -> String {
        self.now()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// Opaque, per-process-unique identity for one idempotent operation. The
    /// counter (not the clock alone) is what makes two operations issued in
    /// the same second distinct.
    fn next_id(&self, prefix: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        format!(
            "{prefix}_{}_{sequence}",
            self.now_rfc3339().replace([':', '-', '.'], "")
        )
    }

    fn capability_report(&self) -> RunnerCapabilities {
        self.capabilities.clone()
    }

    /// Probes again and, when the report differs from `last` (the time of the
    /// probe aside), sends it as a refresh and keeps it as `last`.
    async fn refresh_capabilities(&self, session: &RunnerSession, last: &mut RunnerCapabilities) {
        let Some((_, probe)) = &self.reprobe else {
            return;
        };
        let fresh = probe().await;
        let mut compared = fresh.clone();
        compared.reported_at = last.reported_at;
        for (harness, sent) in compared.harnesses.iter_mut().zip(&last.harnesses) {
            harness.probed_at = sent.probed_at;
        }
        if serde_json::to_value(&compared).ok() == serde_json::to_value(&*last).ok() {
            return;
        }
        let request = RefreshRequest {
            runner_name: self.config.runner_id.clone(),
            runner_version: env!("CARGO_PKG_VERSION").to_owned(),
            rotate_credential: false,
            capabilities: fresh.clone(),
        };
        match self.engine.refresh(session, request).await {
            Ok(_) => *last = fresh,
            Err(error) => tracing::warn!(%error, "the new capability report could not be sent"),
        }
    }

    /// Resumes a persisted session, or enrolls. A persisted session is
    /// validated with a non-rotating `refresh` before it is trusted; if the
    /// server no longer recognises it, and only then, the enrollment token is
    /// spent.
    async fn establish_session(&self) -> Result<RunnerSession, RunnerError> {
        let name = self.config.runner_id.clone();
        let version = env!("CARGO_PKG_VERSION").to_owned();

        let mut session_refused = false;
        if let Some(persisted) = load_session(&self.config.state_dir) {
            match self
                .engine
                .refresh(
                    &persisted,
                    RefreshRequest {
                        runner_name: name.clone(),
                        runner_version: version.clone(),
                        rotate_credential: false,
                        capabilities: self.capability_report(),
                    },
                )
                .await
            {
                Ok(refreshed) => {
                    tracing::info!(runner_id = %refreshed.session.runner_id, "runner session resumed");
                    store_session(&self.config.state_dir, &refreshed.session)?;
                    return Ok(refreshed.session);
                }
                // Only a credential the server refused spends the enrollment
                // token; any other failure (a timeout, a 500, a unique-column
                // conflict) is returned and the retry loop tries again.
                Err(EngineError::Protocol(
                    ProtocolClientError::RunnerRevoked
                    | ProtocolClientError::Protocol {
                        code: StableErrorCode::Unauthorized,
                    },
                )) => {
                    tracing::warn!(
                        "persisted runner session was refused; falling back to enrollment"
                    );
                    session_refused = true;
                }
                Err(error) => {
                    tracing::warn!(%error, "persisted runner session could not be refreshed");
                    return Err(RunnerError::ProtocolTransport);
                }
            }
        }

        let credential = match self.config.require_enrollment_credential() {
            Ok(credential) => credential,
            Err(_) if session_refused => return Err(RunnerError::SessionRefused),
            Err(error) => return Err(error),
        };
        let enrolled = self
            .engine
            .enroll(
                credential,
                EnrollmentRequest {
                    runner_name: name,
                    runner_version: version,
                    capabilities: self.capability_report(),
                },
            )
            .await
            .map_err(|_| RunnerError::ProtocolTransport)?;
        tracing::info!(runner_id = %enrolled.session.runner_id, "runner enrolled");
        store_session(&self.config.state_dir, &enrolled.session)?;
        Ok(enrolled.session)
    }

    /// The runner's one heartbeat: every lease it holds, and as much
    /// capacity as it has slots free. It also keeps
    /// `agent_runners.last_heartbeat_at` fresh while no attempt runs, and
    /// carries each cancellation the answer names to the attempt it names.
    async fn heartbeat_all(&self, session: &RunnerSession) -> Result<(), ProtocolClientError> {
        let active = self.engine.active();
        let request = HeartbeatRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            heartbeat_id: self.next_id("hb"),
            sent_at: self.now(),
            // The capacity the server holds against this runner: its total less the attempts it
            // leased, which a slot still asking for work is not.
            available_capacity: active.free(),
            active_attempts: active.snapshot(),
        };
        let response = self.protocol.heartbeat(session, request).await?;
        active.apply(&response.lease_results);
        Ok(())
    }
}

#[async_trait]
impl<A, W, C> RunnerProtocolClient for HttpRunnerClient<A, W, C>
where
    A: crate::client::HarnessAdapter + 'static,
    W: crate::client::WorktreeProvisioner + 'static,
    C: Clock + 'static,
{
    async fn serve(&self, mut shutdown: Shutdown) -> Result<(), RunnerError> {
        let session = self.establish_session().await?;

        let claim_wait = Duration::from_secs(10);
        let total = self.engine.active().total().max(1) as usize;
        // One slot per attempt the runner may work at once. A slot is taken
        // from the moment it asks for work, so a claim is only ever sent
        // while one is free.
        let mut slots: JoinSet<(Result<RunCycle, EngineError>, tokio::time::Instant)> =
            JoinSet::new();
        let mut ticker = tokio::time::interval(self.renewal_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut claim_not_before = tokio::time::Instant::now();
        // Replay before claiming anything new: an unresolved journal record
        // is a crash-recovery obligation, and taking fresh work first would
        // let a second attempt race the observation of the first.
        // An agent that outlived the previous runner is followed in a slot of
        // its own, so the one ticker heartbeats its lease with every other and
        // the journals behind it are recovered without waiting for it.
        match self.engine.recover(&session).await {
            Ok(outcomes) => {
                let mut replayed = 0;
                for outcome in outcomes {
                    if let RunCycle::Resumed(resumed) = outcome {
                        let engine = Arc::clone(&self.engine);
                        let session = session.clone();
                        slots.spawn(async move {
                            let result = engine.follow_resumed(&session, *resumed).await;
                            (result, tokio::time::Instant::now())
                        });
                    } else {
                        replayed += 1;
                    }
                }
                if replayed > 0 || !slots.is_empty() {
                    tracing::info!(
                        replayed,
                        resumed = slots.len(),
                        "replayed unresolved attempts"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(%error, "recovery pass failed; not claiming new work this cycle");
            }
        }
        let mut reprobe_ticker = self.reprobe.as_ref().map(|(interval, _)| {
            let mut ticker =
                tokio::time::interval_at(tokio::time::Instant::now() + *interval, *interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker
        });
        let mut last_sent = self.capability_report();
        loop {
            if shutdown.is_requested() {
                return Ok(());
            }
            let taken = slots.len();
            let now = tokio::time::Instant::now();
            if taken < total && now >= claim_not_before {
                let engine = Arc::clone(&self.engine);
                let session = session.clone();
                let claim = ClaimRequest {
                    claim_request_id: ClaimRequestId::new(self.next_id("claim")),
                    available_capacity: u32::try_from(total - taken).unwrap_or(u32::MAX),
                    wait: claim_wait,
                };
                slots.spawn(async move {
                    let asked_at = tokio::time::Instant::now();
                    (engine.run_once(&session, claim).await, asked_at)
                });
                continue;
            }
            tokio::select! {
                biased;
                () = shutdown.requested() => return Ok(()),
                _ = ticker.tick() => {
                    if let Err(error) = self.heartbeat_all(&session).await {
                        tracing::warn!(%error, "heartbeat failed");
                    }
                }
                () = async {
                    match reprobe_ticker.as_mut() {
                        Some(ticker) => { ticker.tick().await; }
                        None => std::future::pending().await,
                    }
                } => self.refresh_capabilities(&session, &mut last_sent).await,
                Some(joined) = slots.join_next(), if !slots.is_empty() => match joined {
                    Ok((Ok(RunCycle::NoWork { retry_after }), asked_at)) => {
                        // The server says when to ask again. `wait` only asks
                        // it to hold the claim open, which it is free not to
                        // do, so without this pause an idle runner claims as
                        // fast as the server can answer. Time a held claim
                        // already took counts.
                        claim_not_before = tokio::time::Instant::now()
                            + idle_pause(retry_after, asked_at.elapsed());
                    }
                    Ok((Ok(outcome), _)) => {
                        tracing::info!(?outcome, "run cycle finished");
                        self.refresh_capabilities(&session, &mut last_sent).await;
                    }
                    Ok((Err(error), _)) => {
                        // A failed cycle is logged and paced, never retried
                        // immediately: the engine has already journaled
                        // whatever durable state the attempt reached, and a
                        // tight retry loop against a failing server is
                        // itself a fault.
                        tracing::warn!(%error, "run cycle failed");
                        claim_not_before =
                            tokio::time::Instant::now() + self.protocol.retry.max_backoff;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "a run slot ended abnormally");
                        claim_not_before =
                            tokio::time::Instant::now() + self.protocol.retry.max_backoff;
                    }
                },
                () = tokio::time::sleep_until(claim_not_before),
                    if taken < total && now < claim_not_before => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "transport/tests.rs"]
mod tests;
