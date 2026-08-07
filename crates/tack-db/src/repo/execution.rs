//! Durable neutral execution queue.  This module intentionally uses opaque text
//! identifiers: runner-protocol ids are not UUIDs and callers must not parse
//! their example prefixes.  All runner-owned writes are fenced in SQL.

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tracing::instrument;

use super::Repository;

pub trait ExecutionClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemExecutionClock;

impl ExecutionClock for SystemExecutionClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct NewRunner<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub credential_hash: &'a str,
    pub labels: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: &'a str,
    pub protocol_version: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgentProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub instructions: &'a str,
    pub tool_policy: &'a str,
    pub limits: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewExecutionRequest<'a> {
    pub id: &'a str,
    pub item_id: &'a str,
    pub idempotency_scope: &'a str,
    pub idempotency_key: &'a str,
    /// A stable canonical representation of immutable request fields. Equal
    /// keys with different fingerprints are conflicts, never silent merges.
    pub request_fingerprint: &'a str,
    pub selector_kind: &'a str,
    pub selector_id: &'a str,
    pub agent_profile_id: Option<&'a str>,
    pub agent_profile_snapshot: &'a str,
    pub requested_harness_kind: Option<&'a str>,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    pub repository_snapshot: &'a str,
    pub permission_policy: &'a str,
    pub timeout_seconds: Option<i64>,
    pub budgets: &'a str,
    pub status_map_policy_id: Option<&'a str>,
    pub environment: &'a str,
    pub metadata: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Replayed(String),
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub attempt_id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub id: &'a str,
    pub event_id: &'a str,
    pub sequence: i64,
    pub source: &'a str,
    pub kind: &'a str,
    pub payload: &'a str,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EventBatch<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub previous_checkpoint: Option<&'a str>,
    pub checkpoint: &'a str,
}

#[derive(Debug, Clone)]
pub struct Completion<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub completion_id: &'a str,
    pub terminal_state: &'a str,
    pub terminal_reason: &'a str,
    pub actual_execution: &'a str,
    pub usage: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub artifact_id: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub media_type: Option<&'a str>,
    pub size_bytes: i64,
    pub sha256: &'a str,
    pub content_disposition: Option<&'a str>,
    pub content_reference: Option<&'a str>,
    pub metadata: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewDecision<'a> {
    pub id: &'a str,
    pub decision_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub options: &'a str,
    pub metadata: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct EnrollmentToken<'a> {
    pub id: &'a str,
    pub runner_id: &'a str,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedeemEnrollmentResult {
    Redeemed(String),
    InvalidOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimReplayResult {
    Lease(Lease),
    NoWork,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    pub lease: Lease,
    pub request_snapshot: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct HeartbeatLease<'a> {
    pub attempt_id: &'a str,
    pub fencing_token: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatBatchResult {
    Accepted(Vec<(String, i64, DateTime<Utc>, bool)>),
    Replayed(String),
    StaleLease(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBatchResult {
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: String,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryClassification {
    SafePreSpawnRequeue,
    NeedsOperator,
}

fn stamp(clock: &dyn ExecutionClock) -> String {
    clock.now().to_rfc3339()
}

fn terminal(state: &str) -> bool {
    matches!(state, "succeeded" | "failed" | "cancelled")
}

fn lease_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Lease, sqlx::Error> {
    let issued: String = row.get("lease_issued_at");
    let expires: String = row.get("lease_expires_at");
    let parse = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))
    };
    Ok(Lease {
        attempt_id: row.get("id"),
        request_id: row.get("request_id"),
        attempt_number: row.get("attempt_number"),
        runner_id: row.get("runner_id"),
        fencing_token: row.get("fencing_token"),
        issued_at: parse(issued)?,
        expires_at: parse(expires)?,
    })
}

fn snapshot(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    serde_json::json!({
        "request_id": row.get::<String,_>("id"), "item_id": row.get::<String,_>("item_id"),
        "idempotency_key": row.get::<String,_>("idempotency_key"),
        "agent_profile_snapshot": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("agent_profile_snapshot")).unwrap_or(serde_json::Value::Null),
        "repository": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("repository_snapshot")).unwrap_or(serde_json::Value::Null),
        "permission_policy": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("permission_policy")).unwrap_or(serde_json::Value::Null),
        "budgets": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("budgets")).unwrap_or(serde_json::Value::Null),
        "environment": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("environment")).unwrap_or(serde_json::Value::Null),
        "metadata": serde_json::from_str::<serde_json::Value>(&row.get::<String,_>("metadata")).unwrap_or(serde_json::Value::Null),
        "requested_harness_kind": row.get::<Option<String>,_>("requested_harness_kind"), "requested_model_provider": row.get::<Option<String>,_>("requested_model_provider"), "requested_model_id": row.get::<Option<String>,_>("requested_model_id"), "timeout_seconds": row.get::<Option<i64>,_>("timeout_seconds")
    })
}

impl Repository {
    pub async fn heartbeat_batch(
        &self,
        runner_id: &str,
        heartbeat_id: &str,
        available_capacity: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<HeartbeatBatchResult, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let mut tx = self.pool().begin().await?;
        if let Some(response) = sqlx::query_scalar::<_, String>(
            "SELECT response FROM execution_heartbeat_replays WHERE runner_id=? AND heartbeat_id=?",
        )
        .bind(runner_id)
        .bind(heartbeat_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::Replayed(response));
        }
        let capacity:Option<i64>=sqlx::query_scalar("SELECT total_capacity FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL").bind(runner_id).fetch_optional(&mut *tx).await?;
        let Some(capacity) = capacity else {
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::StaleLease(runner_id.into()));
        };
        if available_capacity < 0 || available_capacity > capacity {
            tx.rollback().await?;
            return Ok(HeartbeatBatchResult::StaleLease(runner_id.into()));
        }
        let mut result = Vec::with_capacity(leases.len());
        for lease in leases {
            let row=sqlx::query("SELECT r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND a.state IN ('leased','preparing','running','waiting_decision') AND a.lease_expires_at>=?").bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).bind(&now_s).fetch_optional(&mut *tx).await?;
            let Some(row) = row else {
                tx.rollback().await?;
                return Ok(HeartbeatBatchResult::StaleLease(lease.attempt_id.into()));
            };
            let cancellation: Option<String> = row.get("cancellation_requested_at");
            sqlx::query("UPDATE execution_attempts SET last_heartbeat_at=?,lease_expires_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=?").bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).execute(&mut *tx).await?;
            result.push((
                lease.attempt_id.into(),
                lease.fencing_token,
                expires,
                cancellation.is_some(),
            ));
        }
        sqlx::query("UPDATE agent_runners SET available_capacity=?,last_heartbeat_at=?,updated_at=? WHERE id=?").bind(available_capacity).bind(&now_s).bind(&now_s).bind(runner_id).execute(&mut *tx).await?;
        let stored=serde_json::to_string(&result.iter().map(|(id,f,_,c)| serde_json::json!({"attempt_id":id,"fencing_token":f,"cancellation_requested":c})).collect::<Vec<_>>()).map_err(|e|sqlx::Error::Protocol(e.to_string()))?;
        sqlx::query("INSERT INTO execution_heartbeat_replays(runner_id,heartbeat_id,response,created_at) VALUES(?,?,?,?)").bind(runner_id).bind(heartbeat_id).bind(&stored).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(HeartbeatBatchResult::Accepted(result))
    }

    pub async fn recover_attempt(
        &self,
        attempt_id: &str,
        recovery_key: &str,
        classification: RecoveryClassification,
        details: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?)").bind(attempt_id).bind(recovery_key).fetch_one(&mut *tx).await? { tx.commit().await?; return Ok(true); }
        let row=sqlx::query("SELECT request_id,runner_id,state,started_at FROM execution_attempts WHERE id=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at<?").bind(attempt_id).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let started: Option<String> = row.get("started_at");
        let (attempt_state, request_state, kind) = match classification {
            RecoveryClassification::SafePreSpawnRequeue if started.is_none() => {
                ("lost", "queued", "safe_pre_spawn_requeue")
            }
            RecoveryClassification::NeedsOperator => {
                ("needs_operator", "needs_operator", "needs_operator")
            }
            _ => {
                tx.rollback().await?;
                return Ok(false);
            }
        };
        sqlx::query("UPDATE execution_attempts SET state=?,updated_at=? WHERE id=?")
            .bind(attempt_state)
            .bind(&now)
            .bind(attempt_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE execution_requests SET state=?,updated_at=? WHERE id=?")
            .bind(request_state)
            .bind(&now)
            .bind(&request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=available_capacity+1,updated_at=? WHERE id=?").bind(&now).bind(&runner).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,created_at) VALUES(?,?,?,?,?)").bind(attempt_id).bind(recovery_key).bind(kind).bind(details).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
    /// Store only a token hash. Tokens are tied to a pre-created runner so a
    /// redemption can atomically consume the token and activate that identity.
    pub async fn issue_enrollment_token(
        &self,
        token: EnrollmentToken<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO agent_enrollment_tokens (id, runner_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(stamp(clock))
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn revoke_enrollment_token(
        &self,
        token_hash: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE token_hash = ? AND consumed_at IS NULL")
            .bind(stamp(clock)).bind(token_hash).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn redeem_enrollment_token(
        &self,
        token_hash: &str,
        credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        runner_version: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<RedeemEnrollmentResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let token = sqlx::query("SELECT runner_id FROM agent_enrollment_tokens WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(token_hash).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(token) = token else {
            tx.commit().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        };
        let runner_id: String = token.get("runner_id");
        let used = sqlx::query("UPDATE agent_enrollment_tokens SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL")
            .bind(&now).bind(token_hash).execute(&mut *tx).await?;
        if used.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        let runner = sqlx::query("UPDATE agent_runners SET credential_hash = ?, credential_expires_at = ?, credential_rotated_at = ?, runner_version = ?, state = 'active', revoked_at = NULL, updated_at = ? WHERE id = ?")
            .bind(credential_hash).bind(credential_expires_at.to_rfc3339()).bind(&now).bind(runner_version).bind(&now).bind(&runner_id).execute(&mut *tx).await?;
        if runner.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        tx.commit().await?;
        Ok(RedeemEnrollmentResult::Redeemed(runner_id))
    }

    /// Strong claim: replay lookup, capacity reservation, attempt and replay
    /// record are committed together, so a crash cannot lose the replay key.
    pub async fn claim_execution_idempotent_with_snapshot(
        &self,
        runner_id: &str,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<Option<ClaimedExecution>, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let mut tx = self.pool().begin().await?;
        let replay=sqlx::query("SELECT a.id,a.request_id,a.attempt_number,a.runner_id,a.fencing_token,a.lease_issued_at,a.lease_expires_at,r.item_id,r.idempotency_key,r.agent_profile_snapshot,r.repository_snapshot,r.permission_policy,r.budgets,r.environment,r.metadata,r.requested_harness_kind,r.requested_model_provider,r.requested_model_id,r.timeout_seconds FROM execution_claim_replays c JOIN execution_attempts a ON a.id=c.attempt_id JOIN execution_requests r ON r.id=a.request_id WHERE c.runner_id=? AND c.claim_request_id=?").bind(runner_id).bind(claim_request_id).fetch_optional(&mut *tx).await?;
        if let Some(row) = replay {
            let lease = lease_from_row(&row)?;
            let snapshot = snapshot(&row);
            tx.commit().await?;
            return Ok(Some(ClaimedExecution {
                lease,
                request_snapshot: snapshot,
            }));
        }
        if sqlx::query("UPDATE agent_runners SET available_capacity=available_capacity-1,updated_at=? WHERE id=? AND state='active' AND revoked_at IS NULL AND available_capacity>0").bind(&now_s).bind(runner_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.commit().await?; return Ok(None); }
        let request=sqlx::query("SELECT id,item_id,idempotency_key,agent_profile_snapshot,repository_snapshot,permission_policy,budgets,environment,metadata,requested_harness_kind,requested_model_provider,requested_model_id,timeout_seconds FROM execution_requests WHERE state='queued' AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at LIMIT 1").bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?;
        let Some(request) = request else {
            tx.rollback().await?;
            return Ok(None);
        };
        let request_id: String = request.get("id");
        if sqlx::query("UPDATE execution_requests SET state='leased',updated_at=? WHERE id=? AND state='queued'").bind(&now_s).bind(&request_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.rollback().await?; return Ok(None); }
        let n: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_number),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        let f: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(fencing_token),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO execution_attempts(id,request_id,attempt_number,runner_id,fencing_token,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(attempt_id).bind(&request_id).bind(n).bind(runner_id).bind(f).bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_claim_replays(runner_id,claim_request_id,attempt_id,created_at) VALUES(?,?,?,?)").bind(runner_id).bind(claim_request_id).bind(attempt_id).bind(&now_s).execute(&mut *tx).await?;
        let lease = Lease {
            attempt_id: attempt_id.into(),
            request_id,
            attempt_number: n,
            runner_id: runner_id.into(),
            fencing_token: f,
            issued_at: now,
            expires_at: expires,
        };
        let snapshot = snapshot(&request);
        tx.commit().await?;
        Ok(Some(ClaimedExecution {
            lease,
            request_snapshot: snapshot,
        }))
    }

    /// Compatibility form for callers that only need lease facts.
    pub async fn claim_execution_idempotent(
        &self,
        runner_id: &str,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<ClaimReplayResult, sqlx::Error> {
        Ok(
            match self
                .claim_execution_idempotent_with_snapshot(
                    runner_id,
                    claim_request_id,
                    attempt_id,
                    lease_duration,
                    clock,
                )
                .await?
            {
                Some(claim) => ClaimReplayResult::Lease(claim.lease),
                None => ClaimReplayResult::NoWork,
            },
        )
    }
    #[instrument(skip(self, input, clock))]
    pub async fn register_runner(
        &self,
        input: NewRunner<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_runners (id, name, credential_hash, labels, total_capacity, \
             available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id)
        .bind(input.name)
        .bind(input.credential_hash)
        .bind(input.labels)
        .bind(input.total_capacity)
        .bind(input.available_capacity)
        .bind(input.capability_snapshot)
        .bind(input.protocol_version)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn create_agent_profile(
        &self,
        input: NewAgentProfile<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.name).bind(input.instructions)
        .bind(input.tool_policy).bind(input.limits).bind(&now).bind(&now)
        .execute(self.pool()).await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn enqueue_execution(
        &self,
        input: NewExecutionRequest<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<EnqueueResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id, request_fingerprint FROM execution_requests \
             WHERE idempotency_scope = ? AND idempotency_key = ?",
        )
        .bind(input.idempotency_scope)
        .bind(input.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, fingerprint)) = existing {
            tx.commit().await?;
            return Ok(if fingerprint == input.request_fingerprint {
                EnqueueResult::Replayed(id)
            } else {
                EnqueueResult::Conflict
            });
        }
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_id, agent_profile_snapshot, \
             requested_harness_kind, requested_model_provider, requested_model_id, repository_snapshot, \
             permission_policy, timeout_seconds, budgets, status_map_policy_id, environment, metadata, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.item_id).bind(input.idempotency_scope).bind(input.idempotency_key)
        .bind(input.request_fingerprint).bind(input.selector_kind).bind(input.selector_id)
        .bind(input.agent_profile_id).bind(input.agent_profile_snapshot).bind(input.requested_harness_kind)
        .bind(input.requested_model_provider).bind(input.requested_model_id).bind(input.repository_snapshot)
        .bind(input.permission_policy).bind(input.timeout_seconds).bind(input.budgets)
        .bind(input.status_map_policy_id).bind(input.environment).bind(input.metadata).bind(&now).bind(&now)
        .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EnqueueResult::Created(input.id.to_string()))
    }

    /// Atomically reserves one eligible queued request.  The request state,
    /// attempt number/fence, and runner capacity are one transaction, so two
    /// claimers cannot obtain valid leases for the same request.
    #[instrument(skip(self, clock))]
    pub async fn claim_execution(
        &self,
        runner_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<Option<Lease>, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let mut tx = self.pool().begin().await?;
        let capacity = sqlx::query(
            "UPDATE agent_runners SET available_capacity = available_capacity - 1, updated_at = ? \
             WHERE id = ? AND state = 'active' AND revoked_at IS NULL AND available_capacity > 0",
        )
        .bind(&now_s)
        .bind(runner_id)
        .execute(&mut *tx)
        .await?;
        if capacity.rows_affected() != 1 {
            tx.commit().await?;
            return Ok(None);
        }
        let request: Option<String> = sqlx::query_scalar(
            "SELECT r.id FROM execution_requests r WHERE r.state = 'queued' AND \
             ( (r.selector_kind = 'exact_runner' AND r.selector_id = ?) OR \
               (r.selector_kind = 'fleet' AND EXISTS (SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id = r.selector_id AND m.runner_id = ?)) ) \
             ORDER BY r.created_at LIMIT 1",
        ).bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?;
        let Some(request_id) = request else {
            sqlx::query("UPDATE agent_runners SET available_capacity = available_capacity + 1, updated_at = ? WHERE id = ?")
                .bind(&now_s).bind(runner_id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(None);
        };
        let claimed = sqlx::query("UPDATE execution_requests SET state = 'leased', updated_at = ? WHERE id = ? AND state = 'queued'")
            .bind(&now_s).bind(&request_id).execute(&mut *tx).await?;
        if claimed.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(None);
        }
        let attempt_number: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id).fetch_one(&mut *tx).await?;
        let fence: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(fencing_token), 0) + 1 FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, lease_issued_at, lease_expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(attempt_id).bind(&request_id).bind(attempt_number).bind(runner_id).bind(fence)
            .bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(Lease {
            attempt_id: attempt_id.into(),
            request_id,
            attempt_number,
            runner_id: runner_id.into(),
            fencing_token: fence,
            issued_at: now,
            expires_at: expires,
        }))
    }

    #[instrument(skip(self, clock))]
    pub async fn heartbeat_execution(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = clock.now();
        let result = sqlx::query("UPDATE execution_attempts SET last_heartbeat_at = ?, lease_expires_at = ?, updated_at = ? WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at >= ?")
            .bind(now.to_rfc3339()).bind((now + lease_duration).to_rfc3339()).bind(now.to_rfc3339())
            .bind(attempt_id).bind(runner_id).bind(fence).bind(now.to_rfc3339()).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, events, clock))]
    pub async fn append_execution_events(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT event_checkpoint FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at >= ?")
            .bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let current: Option<String> = row.get("event_checkpoint");
        if current.as_deref() == Some(batch.checkpoint) {
            tx.commit().await?;
            return Ok(true);
        }
        if current.as_deref() != batch.previous_checkpoint {
            tx.commit().await?;
            return Ok(false);
        }
        for event in events {
            sqlx::query("INSERT INTO execution_events (id, attempt_id, event_id, sequence, source, kind, payload, occurred_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, event_id) DO NOTHING")
                .bind(event.id).bind(batch.attempt_id).bind(event.event_id).bind(event.sequence).bind(event.source).bind(event.kind).bind(event.payload).bind(event.occurred_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
        }
        let updated = sqlx::query("UPDATE execution_attempts SET event_checkpoint = ?, updated_at = ? WHERE id = ? AND runner_id = ? AND fencing_token = ? AND event_checkpoint IS ?")
            .bind(batch.checkpoint).bind(&now).bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(batch.previous_checkpoint).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    #[instrument(skip(self, clock))]
    pub async fn complete_execution(
        &self,
        completion: Completion<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        if !terminal(completion.terminal_state) {
            return Ok(false);
        }
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT request_id, runner_id, state, completion_id FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND lease_expires_at >= ?")
            .bind(completion.attempt_id).bind(completion.runner_id).bind(completion.fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let request_id: String = row.get("request_id");
        let owner_runner_id: String = row.get("runner_id");
        let state: String = row.get("state");
        let existing: Option<String> = row.get("completion_id");
        if terminal(&state) {
            tx.commit().await?;
            return Ok(existing.as_deref() == Some(completion.completion_id));
        }
        let result = sqlx::query("UPDATE execution_attempts SET state = ?, completion_id = ?, terminal_reason = ?, actual_execution = ?, usage = ?, ended_at = ?, updated_at = ? WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ('succeeded','failed','cancelled')")
            .bind(completion.terminal_state).bind(completion.completion_id).bind(completion.terminal_reason).bind(completion.actual_execution).bind(completion.usage).bind(&now).bind(&now).bind(completion.attempt_id).bind(completion.runner_id).bind(completion.fencing_token).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE execution_requests SET state = ?, updated_at = ? WHERE id = ?")
            .bind(completion.terminal_state)
            .bind(&now)
            .bind(request_id)
            .execute(&mut *tx)
            .await?;
        // This is in the same transition transaction and runs only after the
        // terminal CAS succeeded; replay returns above, so capacity restores once.
        sqlx::query("UPDATE agent_runners SET available_capacity = available_capacity + 1, updated_at = ? WHERE id = ?")
            .bind(&now).bind(owner_runner_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    #[instrument(skip(self, clock))]
    pub async fn request_execution_cancellation(
        &self,
        request_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let result = sqlx::query("UPDATE execution_requests SET cancellation_requested_at = COALESCE(cancellation_requested_at, ?), updated_at = ? WHERE id = ? AND state NOT IN ('succeeded','failed','cancelled')")
            .bind(&now).bind(&now).bind(request_id).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, clock))]
    pub async fn classify_expired_attempt(
        &self,
        attempt_id: &str,
        classification: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        if !matches!(classification, "lost" | "needs_operator") {
            return Ok(false);
        }
        let now = stamp(clock);
        let result = sqlx::query("UPDATE execution_attempts SET state = ?, updated_at = ? WHERE id = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at < ?")
            .bind(classification).bind(&now).bind(attempt_id).bind(&now).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, artifact, clock))]
    pub async fn record_execution_artifact(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        artifact: NewArtifact<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ('succeeded','failed','cancelled') AND lease_expires_at >= ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(stamp(clock)).fetch_one(self.pool()).await?;
        if !valid {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, content_disposition, content_reference, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, artifact_id) DO NOTHING")
            .bind(artifact.id).bind(attempt_id).bind(artifact.artifact_id).bind(artifact.kind).bind(artifact.name).bind(artifact.media_type).bind(artifact.size_bytes).bind(artifact.sha256).bind(artifact.content_disposition).bind(artifact.content_reference).bind(artifact.metadata).bind(now).execute(self.pool()).await?;
        Ok(true)
    }

    #[instrument(skip(self, decision, clock))]
    pub async fn create_execution_decision(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        decision: NewDecision<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('running','waiting_decision') AND lease_expires_at >= ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(stamp(clock)).fetch_one(self.pool()).await?;
        if !valid {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, options, metadata, expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, decision_id) DO NOTHING")
        .bind(decision.id).bind(attempt_id).bind(decision.decision_id).bind(decision.kind).bind(decision.prompt).bind(decision.options).bind(decision.metadata).bind(decision.expires_at.map(|v| v.to_rfc3339())).bind(&now).bind(&now).execute(self.pool()).await?;
        Ok(true)
    }
}
