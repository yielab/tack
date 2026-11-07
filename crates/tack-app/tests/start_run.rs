//! The rules behind `POST /api/executions`: what a run is given when the request leaves it out
//! (profile, agent, machine, repository, limits), what it is refused for, and how a retry replays.

use crate::support;
use tack_protocol::Clock;

use serde_json::{Value, json};
use support::{
    OPERATOR, add_profile, add_runner, bare_request, board, code, full_request, set_project,
    snapshot, start,
};
use tack_app::runs::start_run::start_run;
use tack_core::models::{CodeOrigin, CreateRole, UpdateProject, UpdateRole, WorkspaceMode};
use tack_protocol::SystemClock;
use tack_protocol::execution::{AutonomyLevel, ExecutionState, StableErrorCode};
use tack_test_support::seed::{self, AttemptSeed};
use tack_test_support::{create_test_workspace, make_item, make_project};

async fn add_role(board: &support::Board, name: &str, profile: &str) {
    let role = board
        .repo
        .create_role(
            board.project_id,
            CreateRole {
                name: name.into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();
    board
        .repo
        .update_role(
            role.id,
            UpdateRole {
                name: None,
                color: None,
                icon: None,
                agent_profile_id: Some(Some(profile.into())),
            },
        )
        .await
        .unwrap();
    board
        .repo
        .assign_role_to_item(board.item_id, role.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_repeated_key_replays_and_another_operator_starts_its_own_run() {
    let board = board().await;
    let first = start(&board, full_request(board.item_id)).await.unwrap();
    let second = start(&board, full_request(board.item_id)).await.unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(first.request_id, second.request_id);

    let other = start_run(
        &board.repo,
        SystemClock.now(),
        "operator-2",
        full_request(board.item_id),
    )
    .await
    .unwrap();
    assert!(!other.replayed);
    assert_ne!(first.request_id, other.request_id);
}

#[tokio::test]
async fn a_key_reused_with_a_different_request_is_an_idempotency_conflict() {
    let board = board().await;
    start(&board, full_request(board.item_id)).await.unwrap();
    let mut changed = full_request(board.item_id);
    changed.timeout_seconds = Some(61);
    let refusal = start(&board, changed).await.unwrap_err();
    assert_eq!(refusal.status(), 409);
    assert_eq!(refusal.code, StableErrorCode::IdempotencyConflict);
    assert_eq!(refusal.details, json!({"idempotency_key": "same-key"}));
}

#[tokio::test]
async fn a_revoked_runner_still_replays_but_takes_no_new_run() {
    let board = board().await;
    let first = start(&board, full_request(board.item_id)).await.unwrap();
    board
        .repo
        .revoke_runner("runner-active", SystemClock.now())
        .await
        .unwrap();
    let replay = start(&board, full_request(board.item_id)).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.request_id, first.request_id);

    let mut fresh = full_request(board.item_id);
    fresh.idempotency_key = "new-key".into();
    let refusal = start(&board, fresh).await.unwrap_err();
    assert_eq!(refusal.status(), 409);
    assert_eq!(refusal.code, StableErrorCode::MachineRevoked);
}

#[tokio::test]
async fn a_retry_after_the_task_changed_its_type_or_priority_replays_the_request() {
    let board = board().await;
    let first = start(&board, full_request(board.item_id)).await.unwrap();
    tack_test_support::update_item(
        &board.repo,
        board.item_id,
        tack_core::models::UpdateItem {
            item_type: Some("bug".into()),
            priority: Some(tack_core::models::Priority::High),
            ..Default::default()
        },
    )
    .await;
    let retry = start(&board, full_request(board.item_id)).await.unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.request_id, first.request_id);
}

#[tokio::test]
async fn a_retry_after_the_title_was_edited_replays_the_request() {
    let board = board().await;
    let first = start(&board, full_request(board.item_id)).await.unwrap();
    tack_test_support::update_item(
        &board.repo,
        board.item_id,
        tack_core::models::UpdateItem {
            title: Some("A title edited between the two tries".into()),
            ..Default::default()
        },
    )
    .await;
    let retry = start(&board, full_request(board.item_id)).await.unwrap();
    assert!(
        retry.replayed,
        "the composed instructions are not the request"
    );
    assert_eq!(retry.request_id, first.request_id);
}

#[tokio::test]
async fn a_run_without_a_profile_resolves_it_from_the_role_then_the_project_default() {
    let board = board().await;
    add_profile(&board.repo, "profile-content", "{}", "{}").await;
    add_profile(&board.repo, "profile-qa", "{}", "{}").await;
    set_project(
        &board.repo,
        board.project_id,
        UpdateProject {
            default_profile_id: Some(Some("profile-c1".into())),
            ..Default::default()
        },
    )
    .await;

    let started = start(&board, bare_request(board.item_id, "k-default"))
        .await
        .unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["agent_profile_id"], "profile-c1");
    assert_eq!(stored["metadata"]["profile_source"], "project_default");
    assert_eq!(stored["selector"]["runner_id"], "runner-active");

    add_role(&board, "Content", "profile-content").await;
    let started = start(&board, bare_request(board.item_id, "k-role"))
        .await
        .unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["agent_profile_id"], "profile-content");
    assert_eq!(stored["metadata"]["profile_source"], "role:Content");

    add_role(&board, "QA", "profile-qa").await;
    let started = start(&board, bare_request(board.item_id, "k-two"))
        .await
        .unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["agent_profile_id"], "profile-c1");
    assert_eq!(stored["metadata"]["profile_source"], "ambiguous");
}

#[tokio::test]
async fn the_request_snapshot_carries_what_the_item_is() {
    let board = board().await;
    add_profile(&board.repo, "profile-content", "{}", "{}").await;
    let mut request = bare_request(board.item_id, "k-item");
    request.agent_profile_id = Some("profile-content".into());
    request.metadata = json!({"mine": 1});
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(
        stored["metadata"]["item"],
        json!({"type": "task", "priority": "medium", "trusted": true})
    );
    assert_eq!(stored["metadata"]["mine"], 1);
}

#[tokio::test]
async fn the_request_snapshot_carries_the_item_as_data_beside_the_composed_prose() {
    let board = board().await;
    add_profile(&board.repo, "profile-content", "{}", "{}").await;
    let mut request = bare_request(board.item_id, "k-task");
    request.agent_profile_id = Some("profile-content".into());
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    let task = &stored["task"];
    assert_eq!(task["kind"], "Task");
    assert_eq!(task["source"], "manual");
    assert!(
        task["title"]
            .as_str()
            .is_some_and(|title| !title.is_empty())
    );
    // The data holds the profile's own text; the prose the other agents read is built on it.
    let prose = stored["resolved_agent_profile"]["instructions"]
        .as_str()
        .unwrap();
    assert!(prose.starts_with(task["instructions"].as_str().unwrap()));
    assert!(prose.contains("## The item"));
    // A first run has no previous attempt to carry answers from.
    assert!(stored["metadata"].get("previous_answers").is_none());
}

#[tokio::test]
async fn a_runs_limits_come_from_the_request_then_the_profile_then_the_provisional_limit() {
    let board = board().await;
    let with_tokens = r#"{"budgets":{"tokens":300000}}"#;
    let with_time = r#"{"timeout_seconds":1800}"#;
    // (profile limits, request budgets, request timeout, tokens it runs with, where they come from,
    // seconds it runs with); a project that did not change its provisional limit has 2 000 000.
    let rows = [
        (with_tokens, json!({}), None, 300_000, "profile", 3600),
        (
            with_tokens,
            json!({"tokens": null}),
            None,
            300_000,
            "profile",
            3600,
        ),
        (
            with_tokens,
            json!({"tokens": 5000}),
            None,
            5000,
            "run",
            3600,
        ),
        ("{}", json!({}), None, 2_000_000, "provisional", 3600),
        (
            "{}",
            json!({"tokens": null}),
            None,
            2_000_000,
            "provisional",
            3600,
        ),
        (with_time, json!({}), None, 2_000_000, "provisional", 1800),
        (with_time, json!({}), Some(60), 2_000_000, "provisional", 60),
    ];
    for (n, (limits, budgets, timeout, tokens, source, seconds)) in rows.into_iter().enumerate() {
        let id = format!("profile-limits-{n}");
        add_profile(&board.repo, &id, "{}", limits).await;
        let mut request = bare_request(board.item_id, &format!("k-limits-{n}"));
        request.agent_profile_id = Some(id);
        request.budgets = budgets;
        request.timeout_seconds = timeout;
        let started = start(&board, request).await.unwrap();
        let stored = snapshot(&board.repo, &started).await;
        for budgets in [
            &stored["budgets"],
            &stored["resolved_agent_profile"]["budgets"],
        ] {
            assert_eq!(
                budgets.get("tokens").and_then(Value::as_u64),
                Some(tokens),
                "row {n}"
            );
        }
        assert_eq!(stored["metadata"]["limit_source"], source, "row {n}");
        assert_eq!(stored["timeout_seconds"], seconds, "row {n}");
        assert_eq!(
            stored["resolved_agent_profile"]["timeout_seconds"], seconds,
            "row {n}"
        );
    }
}

#[tokio::test]
async fn a_token_limit_that_is_not_a_positive_whole_number_is_refused() {
    let board = board().await;
    for refused in [json!(0), json!(-5), json!("many")] {
        let mut request = bare_request(board.item_id, "k-limits-refused");
        request.agent_profile_id = Some("profile-c1".into());
        request.budgets = json!({"tokens": refused});
        let refusal = start(&board, request).await.unwrap_err();
        assert_eq!(refusal.status(), 400, "{refused}");
        assert_eq!(refusal.details["field"], "budgets.tokens");
    }
}

#[tokio::test]
async fn dollars_are_not_a_limit() {
    let board = board().await;
    let mut request = bare_request(board.item_id, "k-limits-dollars");
    request.agent_profile_id = Some("profile-c1".into());
    request.budgets = json!({"cost_usd": 1.5});
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.details["field"], "budgets.cost_usd");
    assert_eq!(refusal.details["reason"], "field_removed");
}

#[tokio::test]
async fn a_run_without_a_selector_needs_exactly_one_active_runner() {
    let board = board().await;
    let mut request = bare_request(board.item_id, "k-one");
    request.agent_profile_id = Some("profile-c1".into());
    start(&board, request.clone()).await.unwrap();

    add_runner(&board.repo, "runner-second").await;
    request.idempotency_key = "k-two-runners".into();
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.details["reason"], "select_where_this_runs");
}

// Of two active runners the one that answers is where a run without a machine goes; the one
// silent for hours is not a reason to refuse it.
#[tokio::test]
async fn a_run_without_a_selector_resolves_to_the_runner_that_is_online() {
    let board = board().await;
    add_runner(&board.repo, "runner-stale").await;
    let now = chrono::Utc::now();
    seed::agent_runner_heartbeat(
        &board.repo,
        "runner-active",
        &(now - chrono::Duration::seconds(5)).to_rfc3339(),
    )
    .await;
    seed::agent_runner_heartbeat(
        &board.repo,
        "runner-stale",
        &(now - chrono::Duration::hours(2)).to_rfc3339(),
    )
    .await;
    let mut request = bare_request(board.item_id, "k-live");
    request.agent_profile_id = Some("profile-c1".into());
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["selector"]["runner_id"], "runner-active");
}

// A run that names no agent takes it from the resolved profile's `tool_policy.agent`, else the
// project's default agent, else says to select one.
#[tokio::test]
async fn a_run_without_an_agent_takes_the_profiles_then_the_projects_then_asks() {
    let board = board().await;
    let mut request = bare_request(board.item_id, "k-no-agent");
    request.requested_harness_kind = None;

    // Nothing names an agent: asked to select one.
    request.agent_profile_id = Some("profile-c1".into());
    let refusal = start(&board, request.clone()).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.details["reason"], "select_an_agent");

    // The project's default agent.
    set_project(
        &board.repo,
        board.project_id,
        UpdateProject {
            default_harness: Some(Some("opencode".into())),
            ..Default::default()
        },
    )
    .await;
    request.idempotency_key = "k-project-agent".into();
    let started = start(&board, request.clone()).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["requested_harness_kind"], "opencode");

    // The role's profile names its agent, which beats the project's.
    add_profile(
        &board.repo,
        "profile-writer",
        &json!({"agent": "claude-code"}).to_string(),
        "{}",
    )
    .await;
    add_role(&board, "Content", "profile-writer").await;
    request.agent_profile_id = None;
    request.idempotency_key = "k-role-agent".into();
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["agent_profile_id"], "profile-writer");
    assert_eq!(stored["requested_harness_kind"], "claude-code");
}

#[tokio::test]
async fn the_repository_comes_from_the_project_when_omitted() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.repository_snapshot = None;

    // A project with no code: refused, even for an explicit repository.
    set_project(
        &board.repo,
        board.project_id,
        code(CodeOrigin::None, None, WorkspaceMode::LocalBranch),
    )
    .await;
    let refusal = start(&board, request.clone()).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.code, StableErrorCode::InvalidRequest);
    assert_eq!(refusal.details["reason"], "project_has_no_code");

    // A folder project: the snapshot is filled from the project.
    set_project(
        &board.repo,
        board.project_id,
        code(
            CodeOrigin::Folder,
            Some("/work/repo"),
            WorkspaceMode::InPlace,
        ),
    )
    .await;
    let started = start(&board, request.clone()).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(
        stored["repository"],
        json!({"kind":"git","remote":"/work/repo","base_revision":"HEAD","subdirectory":null,
            "workspace_mode":"in_place","repository_path":"/work/repo","push_after_run":true,
            "prepare":{"mode":"detect"}})
    );

    // The project's install setting is copied into the request as it stands.
    set_project(
        &board.repo,
        board.project_id,
        UpdateProject {
            prepare_command: Some(Some("make deps".into())),
            ..Default::default()
        },
    )
    .await;
    request.idempotency_key = "prepare-key".into();
    let started = start(&board, request.clone()).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(
        stored["repository"]["prepare"],
        json!({"mode":"replace","command":"make deps"})
    );

    // A url project always clones and has no local path.
    set_project(
        &board.repo,
        board.project_id,
        code(
            CodeOrigin::Url,
            Some("https://example.test/p.git"),
            WorkspaceMode::InPlace,
        ),
    )
    .await;
    request.idempotency_key = "url-key".into();
    let started = start(&board, request.clone()).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["repository"]["workspace_mode"], "clone");
    assert_eq!(stored["repository"]["repository_path"], Value::Null);

    // An explicit snapshot naming the project's repository may set the revision.
    request.idempotency_key = "explicit-key".into();
    request.repository_snapshot = Some(
        json!({"kind":"git","remote":"https://example.test/p","base_revision":"abc123","subdirectory":"crates"}),
    );
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["repository"]["remote"], "https://example.test/p.git");
    assert_eq!(stored["repository"]["base_revision"], "abc123");
    assert_eq!(stored["repository"]["subdirectory"], "crates");
}

#[tokio::test]
async fn a_request_with_a_different_repository_is_refused() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.repository_snapshot = Some(
        json!({"kind":"git","remote":"https://example.test/other.git","base_revision":"abc123","subdirectory":null}),
    );
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.code, StableErrorCode::InvalidRequest);
    assert_eq!(refusal.details["field"], "repository_snapshot");
    assert_eq!(refusal.details["reason"], "repository_differs_from_project");
}

#[tokio::test]
async fn a_request_that_still_carries_the_profile_snapshot_is_refused() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.agent_profile_snapshot = Some(Value::Null);
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.details["field"], "agent_profile_snapshot");
    assert_eq!(refusal.details["reason"], "field_removed");
}

#[tokio::test]
async fn an_unknown_profile_is_not_found_and_an_unknown_item_too() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.agent_profile_id = Some("missing".into());
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 404);
    assert_eq!(refusal.details["reason"], "agent_profile_not_found");

    let refusal = start(&board, full_request(uuid::Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(refusal.status(), 404);
    assert_eq!(refusal.details, json!({"resource": "item"}));
}

#[tokio::test]
async fn a_run_records_the_request_choice_to_decline_verify_and_push() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.verify = Some(false);
    request.push_branch = Some(false);
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["verify"], json!(false));
    assert_eq!(stored["push_branch"], json!(false));
}

#[tokio::test]
async fn the_principal_scopes_the_idempotency_key_and_names_the_creator() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(
        stored["created_by"],
        json!({"source": "operator_api", "subject_id": OPERATOR})
    );
}

#[tokio::test]
async fn a_level_is_refused_for_an_agent_other_than_docket() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.autonomy = Some(AutonomyLevel::L3);
    let refusal = start(&board, request).await.unwrap_err();
    assert_eq!(refusal.status(), 400);
    assert_eq!(refusal.code, StableErrorCode::InvalidRequest);
    assert_eq!(refusal.message, "A level applies only to a docket run");
    assert_eq!(refusal.details["field"], "autonomy");

    let mut request = full_request(board.item_id);
    request.requested_harness_kind = Some("docket".into());
    request.autonomy = Some(AutonomyLevel::L3);
    let started = start(&board, request).await.unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["autonomy"], "L3");
}

/// What a run the agent credential `agent:cred-1` asks for, and what an operator asks for, both
/// wanting every permission and the network.
async fn permissive_run(board: &support::Board, principal: &str) -> Value {
    let mut request = full_request(board.item_id);
    request.permission_policy.approvals = Some(tack_protocol::execution::Approvals::Auto);
    request.permission_policy.network = Some(true);
    request.origin = Some("operator".into());
    let started = start_run(&board.repo, SystemClock.now(), principal, request)
        .await
        .unwrap();
    snapshot(&board.repo, &started).await
}

#[tokio::test]
async fn a_run_an_agent_credential_starts_asks_first_has_no_network_and_is_an_mcp_run() {
    let board = board().await;
    let stored = permissive_run(&board, "agent:cred-1").await;
    assert_eq!(stored["permission_policy"]["approvals"], "ask");
    assert_eq!(stored["permission_policy"]["network"], false);
    assert_eq!(stored["origin"], "mcp");
}

#[tokio::test]
async fn an_agent_that_already_asks_for_ask_keeps_ask() {
    let board = board().await;
    let mut request = full_request(board.item_id);
    request.permission_policy.approvals = Some(tack_protocol::execution::Approvals::Ask);
    request.permission_policy.network = Some(false);
    let started = start_run(&board.repo, SystemClock.now(), "agent:cred-1", request)
        .await
        .unwrap();
    let stored = snapshot(&board.repo, &started).await;
    assert_eq!(stored["permission_policy"]["approvals"], "ask");
    assert_eq!(stored["permission_policy"]["network"], false);
}

#[tokio::test]
async fn the_operators_run_keeps_what_it_asked_for() {
    let board = board().await;
    let stored = permissive_run(&board, "operator:token:abc").await;
    assert_eq!(stored["permission_policy"]["approvals"], "auto");
    assert_eq!(stored["permission_policy"]["network"], true);
    assert_eq!(stored["origin"], Value::Null);
}

/// A request body's machine is both fields or neither, and only a runner or a fleet: the typed
/// request cannot hold anything else.
#[test]
fn a_request_body_names_a_runner_or_a_fleet_or_nothing() {
    use tack_app::runs::start_run::{level_from, selector_from};
    use tack_protocol::execution::{RunnerId, RunnerSelector};

    assert_eq!(selector_from(None, None).unwrap(), None);
    assert_eq!(
        selector_from(Some("fleet".into()), Some("f1".into())).unwrap(),
        Some(RunnerSelector::Fleet {
            fleet_id: "f1".into()
        })
    );
    assert_eq!(
        selector_from(Some("exact_runner".into()), Some("r1".into())).unwrap(),
        Some(RunnerSelector::ExactRunner {
            runner_id: RunnerId::new("r1")
        })
    );
    for (kind, id) in [
        (Some("any"), Some("x")),
        (Some("fleet"), None),
        (None, Some("x")),
    ] {
        let refusal = selector_from(kind.map(Into::into), id.map(Into::into)).unwrap_err();
        assert_eq!(refusal.status(), 400);
        assert_eq!(refusal.details["field"], "selector_kind");
    }
    assert_eq!(
        level_from(Some("L4".into())).unwrap(),
        Some(AutonomyLevel::L4)
    );
    assert_eq!(
        level_from(Some("l4".into())).unwrap_err().details["field"],
        "autonomy"
    );
}

/// Plants an attempt of `request_id`, under `runner-active`, whose evidence is `actual_execution`
/// (a raw JSON object literal, not a typed `ActualExecution`: only `$.git.branch` is read).
async fn leave_evidence(board: &support::Board, request_id: &str, actual_execution: Value) {
    seed::execution_attempt(
        &board.repo,
        AttemptSeed {
            id: &format!("attempt-{request_id}"),
            request_id,
            runner_id: "runner-active",
            state: ExecutionState::Failed,
            at: "2026-01-01T00:00:00Z",
            usage: None,
            actual_execution: Some(&actual_execution.to_string()),
        },
    )
    .await;
}

/// A full request (named profile, machine and repository) under its own idempotency key, so
/// several can be started on the same item without replaying one another.
fn keyed_request(item_id: uuid::Uuid, key: &str) -> tack_app::runs::start_run::RunRequest {
    let mut request = full_request(item_id);
    request.idempotency_key = key.into();
    request
}

#[tokio::test]
async fn from_run_sets_rework_base_to_the_branch_its_evidence_kept() {
    let board = board().await;
    let stopped = start(&board, keyed_request(board.item_id, "stopped-1"))
        .await
        .unwrap();
    leave_evidence(
        &board,
        &stopped.request_id,
        json!({"git": {"branch": "attempt-branch"}}),
    )
    .await;

    let mut continuing = keyed_request(board.item_id, "continue-1");
    continuing.from_run = Some(stopped.request_id.clone());
    let started = start(&board, continuing).await.unwrap();

    let snapshot = snapshot(&board.repo, &started).await;
    assert_eq!(snapshot["metadata"]["rework_base"], "attempt-branch");
}

#[tokio::test]
async fn from_run_is_refused_when_its_run_left_no_branch_to_continue_from() {
    let board = board().await;
    let no_evidence = start(&board, keyed_request(board.item_id, "no-evidence"))
        .await
        .unwrap();
    // Ran, but reported no git branch at all (an agent that never named one, or reported
    // nothing of the kind `$.git.branch` would read).
    leave_evidence(&board, &no_evidence.request_id, json!({"result": "ok"})).await;

    let mut continuing = keyed_request(board.item_id, "continue-no-evidence");
    continuing.from_run = Some(no_evidence.request_id.clone());
    let refusal = start(&board, continuing).await.unwrap_err();

    assert_eq!(refusal.status(), 422);
    assert_eq!(refusal.details["field"], "from_run");
    assert_eq!(refusal.details["reason"], "from_run_no_kept_work");
}

#[tokio::test]
async fn from_run_is_refused_when_it_names_a_run_of_another_project() {
    let board = board().await;
    let other_workspace = create_test_workspace(&board.repo).await;
    let other_project = make_project(&board.repo, other_workspace).await;
    let other_item = make_item(&board.repo, &other_project).await;
    seed::execution_request(
        &board.repo,
        "exec_other-project-run",
        &other_item.id.to_string(),
        "runner-active",
        "2026-01-01T00:00:00Z",
    )
    .await;
    leave_evidence(
        &board,
        "exec_other-project-run",
        json!({"git": {"branch": "someone-elses-branch"}}),
    )
    .await;

    let mut continuing = keyed_request(board.item_id, "continue-cross-project");
    continuing.from_run = Some("exec_other-project-run".into());
    let refusal = start(&board, continuing).await.unwrap_err();

    assert_eq!(refusal.status(), 422);
    assert_eq!(refusal.details["field"], "from_run");
    assert_eq!(refusal.details["reason"], "from_run_different_project");
}

#[tokio::test]
async fn from_run_is_refused_when_it_names_no_request() {
    let board = board().await;
    let mut continuing = keyed_request(board.item_id, "continue-unknown");
    continuing.from_run = Some("exec_does-not-exist".into());
    let refusal = start(&board, continuing).await.unwrap_err();

    assert_eq!(refusal.status(), 404);
    assert_eq!(refusal.details["field"], "from_run");
    assert_eq!(refusal.details["reason"], "from_run_not_found");
}
