//! Proves `tack_orch::model_policy::wiring::resolve_request_model_policy`
//! against a real `tack_db::Repository`, and — the card's own load-bearing
//! safety claim — that once a resolved model is persisted as an
//! `execution_requests` row's `requested_model_provider`/`requested_model_id`,
//! a runner that does not declare that model can never lease it: the
//! existing, unmodified claim path (`tack_orch::scheduler::wiring`, card
//! III-E6, and `Repository::claim_execution_idempotent_with_snapshot`, card
//! B2) rejects it before any `execution_attempts` row, fencing token, or
//! capacity change is ever committed.
//!
//! This is a genuine integration proof, not a unit test dressed up as one:
//! every step below goes through the same repository methods the real API
//! handlers use, and the "never leases" assertions check the database
//! directly (row counts, request state, runner capacity) rather than only a
//! function's return value — the discipline `CLAUDE.md` and this card's own
//! brief both name explicitly ("assert the absence directly").

use chrono::{Duration as ChronoDuration, Utc};
use tack_core::models::{CreateItem, ItemType, Priority as ItemPriority, ProjectType};
use tack_core::vocabulary;
use tack_db::repo::execution::{NewAgentProfile, NewExecutionRequest, NewRunner, RequestSelection};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::model_policy::wiring::resolve_request_model_policy;
use tack_orch::model_policy::{ModelPolicyTier, ResolvedModelPolicy};
use tack_orch::scheduler::types::ModelSelector;
use tack_orch::scheduler::wiring::choose_request_for_runner;
use tack_orch::scheduler::{SchedulingPolicy, choose_request_for_runner as choose};
use uuid::Uuid;

struct FixedClock(chrono::DateTime<Utc>);
impl tack_db::repo::execution::ExecutionClock for FixedClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0
    }
}

async fn setup_repo() -> (Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace_id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'F3', ?)")
        .bind(workspace_id.to_string())
        .bind(vocab)
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "F3".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let status = project.workflow.initial_status().unwrap().to_string();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Model policy proof".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: Some(ItemPriority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item");
    let clock = FixedClock(Utc::now());
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-1",
            name: "profile-1",
            instructions: "work safely",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("agent profile");
    (repo, item.id.to_string())
}

/// A runner that has declared exactly one harness/model combination:
/// `codex` + `openai/opaque/model-alpha`. Anything else is unavailable to
/// it.
fn codex_capability_snapshot(now: chrono::DateTime<Utc>) -> String {
    serde_json::json!({
        "reported_at": now.to_rfc3339(),
        "labels": {},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now.to_rfc3339(),
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-alpha"],
                "discovery": "reported"
            }]
        }],
        "features": {
            "cancel": {"support": "supported", "reason": null},
            "resume": {"support": "unsupported", "reason": "n/a"},
            "decisions": {"support": "supported", "reason": null},
            "artifacts": {"support": "supported", "reason": null},
            "usage": {"support": "advisory", "reason": "n/a"}
        },
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 1048576}
    })
    .to_string()
}

async fn register_active_runner(
    repo: &Repository,
    runner_id: &str,
    capability_snapshot: &str,
    now: chrono::DateTime<Utc>,
) {
    repo.register_runner(
        NewRunner {
            id: runner_id,
            name: runner_id,
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot,
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(runner_id)
        .execute(repo.pool())
        .await
        .expect("set heartbeat");
}

async fn create_fleet_with_default_model(
    repo: &Repository,
    fleet_id: &str,
    default_model_provider: &str,
    default_model_id: &str,
    now: chrono::DateTime<Utc>,
) {
    let default_policy = serde_json::json!({
        "default_model": {"provider": default_model_provider, "model_id": default_model_id}
    })
    .to_string();
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, ?, NULL, ?, ?, ?)",
    )
    .bind(fleet_id)
    .bind(format!("fleet-{fleet_id}"))
    .bind(default_policy)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert fleet");
}

async fn add_fleet_member(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
    now: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(fleet_id)
    .bind(runner_id)
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("fleet membership");
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_via_fleet(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    idempotency_key: &str,
    fleet_id: &str,
    harness: &str,
    model_provider: Option<&str>,
    model_id: Option<&str>,
    created_at: chrono::DateTime<Utc>,
) {
    let selector = serde_json::json!({"kind":"fleet","fleet_id":fleet_id});
    let snapshot = serde_json::json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": idempotency_key,
        "created_by": {"source": "test", "subject_id": "model-policy-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": "profile-1",
        "resolved_agent_profile": {
            "name": "profile", "instructions": "work safely",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "requested_harness_kind": harness,
        "requested_model_provider": model_provider,
        "requested_model_id": model_id,
        "repository": {
            "kind": "git", "remote": "https://example.test/f3.git",
            "base_revision": "abc123def456abc123def456abc123def456abc", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": {}
    })
    .to_string();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: request_id,
            item_id,
            idempotency_scope: "model-policy-test",
            idempotency_key,
            request_fingerprint: request_id,
            selector_kind: "fleet",
            selector_id: fleet_id,
            agent_profile_id: Some("profile-1"),
            agent_profile_snapshot: r#"{"name":"profile","instructions":"work safely","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
            requested_harness_kind: Some(harness),
            requested_model_provider: model_provider,
            requested_model_id: model_id,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/f3.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata: "{}",
            request_snapshot: &snapshot,
        },
        &FixedClock(created_at),
    )
    .await
    .expect("enqueue");
}

async fn attempt_count_for_request(repo: &Repository, request_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
        .bind(request_id)
        .fetch_one(repo.pool())
        .await
        .expect("count attempts")
}

async fn request_state(repo: &Repository, request_id: &str) -> String {
    sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
        .bind(request_id)
        .fetch_one(repo.pool())
        .await
        .expect("request state")
}

async fn runner_available_capacity(repo: &Repository, runner_id: &str) -> i64 {
    sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = ?")
        .bind(runner_id)
        .fetch_one(repo.pool())
        .await
        .expect("runner capacity")
}

// ---------------------------------------------------------------------
// Precedence resolution against real `agent_profiles`/`agent_fleets` rows
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_fleet_default_model_is_read_from_the_real_default_policy_column() {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    create_fleet_with_default_model(&repo, "fleet-a", "openai", "opaque/model-alpha", now).await;

    let resolved = resolve_request_model_policy(&repo, None, Some("fleet-a"), None)
        .await
        .expect("no db error");
    assert_eq!(
        resolved,
        ResolvedModelPolicy {
            selector: ModelSelector::Explicit {
                provider: tack_orch::execution::RequestedModelProvider::new("openai"),
                model_id: tack_orch::execution::RequestedModelId::new("opaque/model-alpha"),
            },
            source: Some(ModelPolicyTier::Fleet),
        }
    );
}

#[tokio::test]
async fn an_agent_profile_default_beats_a_fleet_default() {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    // The agent profile from `setup_repo` (`profile-1`) starts with `limits:
    // "{}"` (no opinion) — overwrite it directly with a real default so this
    // test exercises the actual `agent_profiles.limits` column, matching how
    // an operator would configure it via `POST /api/agent-profiles`.
    sqlx::query("UPDATE agent_profiles SET limits = ? WHERE id = 'profile-1'")
        .bind(r#"{"default_model":{"provider":"anthropic","model_id":"opaque/sonnet"}}"#)
        .execute(repo.pool())
        .await
        .expect("set profile default");
    create_fleet_with_default_model(&repo, "fleet-a", "openai", "opaque/model-alpha", now).await;

    let resolved = resolve_request_model_policy(&repo, Some("profile-1"), Some("fleet-a"), None)
        .await
        .expect("no db error");
    assert_eq!(resolved.source, Some(ModelPolicyTier::AgentProfile));
    assert_eq!(
        resolved.selector,
        ModelSelector::Explicit {
            provider: tack_orch::execution::RequestedModelProvider::new("anthropic"),
            model_id: tack_orch::execution::RequestedModelId::new("opaque/sonnet"),
        }
    );
}

#[tokio::test]
async fn a_request_override_beats_every_other_tier() {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    sqlx::query("UPDATE agent_profiles SET limits = ? WHERE id = 'profile-1'")
        .bind(r#"{"default_model":{"provider":"anthropic","model_id":"opaque/sonnet"}}"#)
        .execute(repo.pool())
        .await
        .expect("set profile default");
    create_fleet_with_default_model(&repo, "fleet-a", "openai", "opaque/model-alpha", now).await;

    let override_selector = ModelSelector::Explicit {
        provider: tack_orch::execution::RequestedModelProvider::new("openai"),
        model_id: tack_orch::execution::RequestedModelId::new("opaque/override-model"),
    };
    let resolved = resolve_request_model_policy(
        &repo,
        Some("profile-1"),
        Some("fleet-a"),
        Some(override_selector.clone()),
    )
    .await
    .expect("no db error");
    assert_eq!(resolved.source, Some(ModelPolicyTier::RequestOverride));
    assert_eq!(resolved.selector, override_selector);
}

#[tokio::test]
async fn no_tier_configured_resolves_to_auto_select() {
    let (repo, _item_id) = setup_repo().await;
    let resolved = resolve_request_model_policy(&repo, Some("profile-1"), None, None)
        .await
        .expect("no db error");
    assert_eq!(resolved.source, None);
    assert_eq!(resolved.selector, ModelSelector::AutoSelect);
}

// ---------------------------------------------------------------------
// The load-bearing safety claim: unavailable choice never leases
// ---------------------------------------------------------------------

/// Full pipeline: resolve a fleet's default model (this card), persist it
/// as the queued request's `requested_model_provider`/`requested_model_id`
/// (exactly what a wired `POST /executions` handler would do), then run it
/// through the real, unmodified claim path. The runner only declares
/// `openai/opaque/model-alpha`; the fleet's configured default is a
/// different, undeclared model. The request must never be leased.
#[tokio::test]
async fn a_fleet_default_model_the_runner_does_not_declare_never_leases() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();

    register_active_runner(&repo, "runner-a", &codex_capability_snapshot(now), now).await;
    create_fleet_with_default_model(&repo, "fleet-a", "openai", "opaque/UNAVAILABLE-model", now)
        .await;
    add_fleet_member(&repo, "fleet-a", "runner-a", now).await;

    let resolved = resolve_request_model_policy(&repo, None, Some("fleet-a"), None)
        .await
        .expect("resolve model policy");
    let (provider, model_id) = match &resolved.selector {
        ModelSelector::Explicit { provider, model_id } => {
            (provider.as_str().to_string(), model_id.as_str().to_string())
        }
        ModelSelector::AutoSelect => panic!("expected the fleet default to resolve explicitly"),
    };
    assert_eq!(model_id, "opaque/UNAVAILABLE-model");

    let request_id = "req-unavailable";
    enqueue_via_fleet(
        &repo,
        &item_id,
        request_id,
        "key-unavailable",
        "fleet-a",
        "codex",
        Some(&provider),
        Some(&model_id),
        now,
    )
    .await;

    // Step 1: the pure-scheduler wiring (card III-E6, untouched by this
    // card) must find no eligible pick.
    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen, None,
        "an undeclared fleet-default model must never be chosen for a claim attempt"
    );
    // The re-exported entry point must agree — not a second, drifting path.
    let chosen_via_reexport = choose(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen_via_reexport, None);

    // Step 2: the actual fenced claim transaction (card B2, untouched) must
    // also refuse, honoring `Scheduled(None)` rather than falling back to a
    // naive match that could still lease it.
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-unavailable",
            "attempt-unavailable",
            ChronoDuration::seconds(60),
            &FixedClock(now),
            RequestSelection::Scheduled(chosen.as_deref()),
        )
        .await
        .expect("no db error");
    assert!(
        claimed.is_none(),
        "claim must return no lease for an unavailable choice"
    );

    // Step 3: assert the absence directly against the database, not just
    // the function's return value (CLAUDE.md: "assert the absence
    // directly — row counts, an untouched checkpoint, empty bookkeeping").
    assert_eq!(
        attempt_count_for_request(&repo, request_id).await,
        0,
        "no execution_attempts row may exist for an unavailable choice"
    );
    assert_eq!(
        request_state(&repo, request_id).await,
        "queued",
        "the request must remain queued, never transition to leased"
    );
    assert_eq!(
        runner_available_capacity(&repo, "runner-a").await,
        1,
        "the runner's capacity reservation must be rolled back, not partially consumed"
    );
}

/// The positive control for the test above, proving its assertions are
/// load-bearing rather than vacuously true (e.g. because claiming never
/// works in this harness for an unrelated reason). Identical setup, except
/// the fleet's configured default *is* the model the runner declares — the
/// request must be leased successfully.
#[tokio::test]
async fn a_fleet_default_model_the_runner_does_declare_leases_successfully() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();

    register_active_runner(&repo, "runner-a", &codex_capability_snapshot(now), now).await;
    create_fleet_with_default_model(&repo, "fleet-a", "openai", "opaque/model-alpha", now).await;
    add_fleet_member(&repo, "fleet-a", "runner-a", now).await;

    let resolved = resolve_request_model_policy(&repo, None, Some("fleet-a"), None)
        .await
        .expect("resolve model policy");
    let (provider, model_id) = match &resolved.selector {
        ModelSelector::Explicit { provider, model_id } => {
            (provider.as_str().to_string(), model_id.as_str().to_string())
        }
        ModelSelector::AutoSelect => panic!("expected the fleet default to resolve explicitly"),
    };
    assert_eq!(model_id, "opaque/model-alpha");

    let request_id = "req-available";
    enqueue_via_fleet(
        &repo,
        &item_id,
        request_id,
        "key-available",
        "fleet-a",
        "codex",
        Some(&provider),
        Some(&model_id),
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        chosen.as_deref(),
        Some(request_id),
        "a declared, available model must be chosen"
    );

    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-available",
            "attempt-available",
            ChronoDuration::seconds(60),
            &FixedClock(now),
            RequestSelection::Scheduled(chosen.as_deref()),
        )
        .await
        .expect("no db error")
        .expect("an available choice must actually lease");

    assert_eq!(claimed.lease.request_id, request_id);
    assert_eq!(
        attempt_count_for_request(&repo, request_id).await,
        1,
        "exactly one execution_attempts row must exist for a successful lease"
    );
    assert_eq!(request_state(&repo, request_id).await, "leased");
    assert_eq!(
        runner_available_capacity(&repo, "runner-a").await,
        0,
        "the runner's one slot must now be consumed"
    );
}
