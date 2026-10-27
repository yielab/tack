//! A store with one active runner, one agent profile and one item, for the tests of the
//! runner-protocol use cases. Not every test binary uses every function.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};
use tack_db::Repository;
use tack_db::repo::execution::{NewAgentProfile, NewExecutionRequest, NewRunner};
use tack_protocol::Clock;
use tack_test_support::{
    ControllableClock, create_test_workspace, make_item, make_project, setup_test_db,
};
use uuid::Uuid;

pub const RUNNER_ID: &str = "runner-use-case";
pub const CREDENTIAL_HASH: &str = "hash-of-the-first-credential";
const PROFILE_ID: &str = "profile-use-case";
const MODEL_ID: &str = "opaque/model-use-case";

pub struct FakeClock(pub ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

pub struct World {
    pub repo: Repository,
    pub clock: FakeClock,
    pub item_id: String,
}

pub fn capabilities(reported_at: DateTime<Utc>, total: i64, available: i64) -> Value {
    json!({
        "reported_at": reported_at.to_rfc3339(),
        "labels": {"os": "linux"},
        "concurrency": {"total": total, "available": available},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": reported_at.to_rfc3339(),
            "requires_model": true,
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// An active runner (capacity 2) that declares `codex`, a profile, and an item.
pub async fn world() -> World {
    let repo = setup_test_db().await;
    let clock = FakeClock(ControllableClock::new(
        Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap(),
    ));
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item_id = make_item(&repo, &project).await.id.to_string();
    let snapshot = capabilities(clock.now(), 2, 2).to_string();
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "Use case runner",
            credential_hash: CREDENTIAL_HASH,
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: &snapshot,
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: PROFILE_ID,
            name: "Use case profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &clock,
    )
    .await
    .expect("profile");
    World {
        repo,
        clock,
        item_id,
    }
}

/// Enqueues a request that only `RUNNER_ID` may take; returns its id.
pub async fn enqueue(world: &World, key: &str) -> String {
    let request_id = format!("exec_{}", Uuid::new_v4());
    let snapshot = json!({
        "request_id": request_id,
        "item_id": world.item_id,
        "idempotency_key": key,
        "created_by": {"source": "test", "subject_id": "use-case"},
        "created_at": world.clock.now().to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": RUNNER_ID},
        "agent_profile_id": PROFILE_ID,
        "resolved_agent_profile": {"name":"Use case profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "requested_harness_kind": "codex",
        "requested_model": MODEL_ID,
        "repository": {"kind":"git","remote":"https://example.test/uc.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory": Value::Null},
        "permission_policy": {"tools":["shell"],"network": false},
        "timeout_seconds": 60,
        "budgets": {"tokens": 1000},
        "status_map_policy_id": Value::Null,
        "environment": {},
        "metadata": {},
    });
    let snapshot_string = serde_json::to_string(&snapshot).unwrap();
    let field = |name: &str| serde_json::to_string(&snapshot[name]).unwrap();
    world
        .repo
        .enqueue_execution(
            NewExecutionRequest {
                id: &request_id,
                item_id: &world.item_id,
                idempotency_scope: "test",
                idempotency_key: key,
                request_fingerprint: key,
                selector_kind: "exact_runner",
                selector_id: RUNNER_ID,
                agent_profile_id: Some(PROFILE_ID),
                agent_profile_snapshot: &field("resolved_agent_profile"),
                requested_harness_kind: Some("codex"),
                requested_model: Some(MODEL_ID),
                repository_snapshot: &field("repository"),
                permission_policy: &field("permission_policy"),
                timeout_seconds: Some(60),
                budgets: &field("budgets"),
                status_map_policy_id: None,
                environment: &field("environment"),
                metadata: &field("metadata"),
                request_snapshot: &snapshot_string,
            },
            world.clock.now(),
        )
        .await
        .expect("enqueue");
    request_id
}

/// An attempt of `RUNNER_ID` that is leased and then running, under its first fence.
pub struct Running {
    pub request_id: String,
    pub attempt_id: String,
    pub fence: i64,
}

/// Enqueues a request, claims it and puts the attempt in `running`.
pub async fn running_attempt(world: &World, key: &str) -> Running {
    let request_id = enqueue(world, key).await;
    let claimed = tack_app::runs::claim::claim(
        &world.repo,
        &world.clock,
        RUNNER_ID,
        &format!("claim-{key}"),
        chrono::Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");
    tack_test_support::seed::execution_attempt_state(
        &world.repo,
        &claimed.lease.attempt_id,
        tack_protocol::execution::ExecutionState::Running,
    )
    .await;
    Running {
        request_id,
        attempt_id: claimed.lease.attempt_id,
        fence: claimed.lease.fencing_token,
    }
}
