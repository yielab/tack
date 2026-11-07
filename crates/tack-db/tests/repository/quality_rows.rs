//! The rows behind the Quality figures: one per attempt with the facts a rate or a cut needs, and the
//! questions and steps counted by agent. Every number is worked out by hand in the comments.

use crate::common;

use tack_db::Repository;
use tack_db::queries::metrics::{action_counts, quality_attempts, question_rows};

const MRP: &str = "application/vnd.tack.mrp+json";

use common::seed::statement as run;

/// A request with one attempt in `state` on `item` at `at`.
async fn attempt(repo: &Repository, id: &str, item: &str, state: &str, at: &str, reason: &str) {
    run(
        repo,
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
         repository_snapshot, permission_policy, metadata, request_snapshot, requested_harness_kind, \
         prompt_template_sha, created_at, updated_at) \
         VALUES (?1, ?2, 's', ?1, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', \
         '{\"resolved_agent_profile\":{\"name\":\"Implementer\",\"tool_policy\":{\"docket\":{\"team\":\"web\"}}}}', \
         'claude-code', 'sha-1', ?3, ?3)",
        &[id, item, at],
    )
    .await;
    run(
        repo,
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, \
         state, terminal_reason, lease_issued_at, lease_expires_at, created_at, updated_at) \
         VALUES (?1, ?1, 1, 'run', 1, ?2, NULLIF(?3, ''), ?4, ?4, ?4, ?4)",
        &[id, state, reason, at],
    )
    .await;
}

#[tokio::test]
async fn each_attempt_carries_its_facts_and_the_window_leaves_the_old_ones_out() {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let plain = common::make_item(&repo, &project).await.id.to_string();
    let weekly = common::make_item(&repo, &project).await.id.to_string();
    let next = common::make_item(&repo, &project).await.id.to_string();
    let after = common::make_item(&repo, &project).await.id.to_string();
    run(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        &[],
    )
    .await;
    // A chain of recurring tasks: weekly, its successor, and the successor's.
    run(
        &repo,
        "UPDATE items SET recurrence = 'weekly' WHERE id = ?",
        &[&weekly],
    )
    .await;
    run(
        &repo,
        "UPDATE items SET recurrence = 'weekly', recurrence_of = ? WHERE id = ?",
        &[&weekly, &next],
    )
    .await;
    run(
        &repo,
        "UPDATE items SET recurrence = 'weekly', recurrence_of = ? WHERE id = ?",
        &[&next, &after],
    )
    .await;

    attempt(&repo, "old", &plain, "failed", "2026-09-01T09:00:00Z", "").await;
    attempt(
        &repo,
        "first",
        &plain,
        "succeeded",
        "2026-10-10T09:00:00Z",
        "",
    )
    .await;
    attempt(
        &repo,
        "refused",
        &plain,
        "failed",
        "2026-10-11T09:00:00Z",
        r#"{"code":"harness_rejected"}"#,
    )
    .await;
    attempt(
        &repo,
        "w1",
        &weekly,
        "succeeded",
        "2026-10-12T09:00:00Z",
        "",
    )
    .await;
    attempt(&repo, "w2", &next, "succeeded", "2026-10-13T09:00:00Z", "").await;
    attempt(&repo, "w3", &after, "failed", "2026-10-14T09:00:00Z", "").await;
    run(
        &repo,
        "INSERT INTO attempt_reviews (attempt_id, verdict, reviewed_at) VALUES ('first', 'accepted', '2026-10-10T10:00:00Z')",
        &[],
    )
    .await;
    run(
        &repo,
        "INSERT INTO pull_requests (attempt_id, repo, number, url, state, opened_at) \
         VALUES ('first', 'o/r', 1, 'u', 'merged', '2026-10-10T11:00:00Z')",
        &[],
    )
    .await;
    run(
        &repo,
        "INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, \
         sha256, content_reference, created_at) VALUES ('x1', 'first', 'a', 'report', 'p', ?, 1, 'x', 'p1.json', '2026-10-10T09:30:00Z')",
        &[MRP],
    )
    .await;

    let project = project.id.to_string();
    let all = quality_attempts(&repo, &project, MRP, None).await.unwrap();
    assert_eq!(all.len(), 6);
    let windowed = quality_attempts(&repo, &project, MRP, Some("2026-10-01T00:00:00Z"))
        .await
        .unwrap();
    assert_eq!(windowed.len(), 5);
    assert!(windowed.iter().all(|a| a.attempt_id != "old"));

    let by_id = |id: &str| windowed.iter().find(|a| a.attempt_id == id).unwrap();
    let first = by_id("first");
    // The "old" attempt was the item's first, so "first" is not.
    assert!(!first.first_of_item);
    assert_eq!(
        (
            first.profile.as_deref(),
            first.harness.as_deref(),
            first.team.as_deref(),
            first.template.as_deref(),
        ),
        (
            Some("Implementer"),
            Some("claude-code"),
            Some("web"),
            Some("sha-1")
        )
    );
    assert_eq!(first.verdict.as_deref(), Some("accepted"));
    assert_eq!(first.pull_request.as_deref(), Some("merged"));
    assert_eq!(first.pack_reference.as_deref(), Some("p1.json"));
    assert_eq!(first.recurring_root, None);
    // The chain shares the first task as its root; the plain task is no chain.
    for id in ["w1", "w2", "w3"] {
        assert_eq!(
            by_id(id).recurring_root.as_deref(),
            Some(weekly.as_str()),
            "{id}"
        );
        assert!(by_id(id).first_of_item, "{id}");
    }
}

#[tokio::test]
async fn questions_are_counted_by_agent_and_kind_over_the_steps_it_reported() {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await.id.to_string();
    run(
        &repo,
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        &[],
    )
    .await;
    attempt(&repo, "A", &item, "succeeded", "2026-10-10T09:00:00Z", "").await;
    // Four decisions: allowed, denied, expired unanswered, and one of another kind allowed.
    for (id, kind, state, answer) in [
        (
            "d1",
            "approval",
            "resolved",
            r#"{"option_id":"allow_once"}"#,
        ),
        ("d2", "approval", "resolved", r#"{"option_id":"deny"}"#),
        ("d3", "approval", "expired", ""),
        (
            "d4",
            "choice",
            "resolved",
            r#"{"option_id":"allow_always"}"#,
        ),
    ] {
        run(
            &repo,
            "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, state, prompt, answer, \
             created_at, updated_at) VALUES (?1, 'A', ?1, ?2, ?3, 'p', NULLIF(?4, ''), '2026-10-10T09:10:00Z', '2026-10-10T09:10:00Z')",
            &[id, kind, state, answer],
        )
        .await;
    }
    // Eight steps: seven model calls and one hop; a tool line is not a step.
    for n in 0..9 {
        let kind = match n {
            0 => "agent.hop_started",
            1 => "agent.untracked",
            _ => "agent.model_call",
        };
        run(
            &repo,
            "INSERT INTO execution_events (id, attempt_id, event_id, sequence, source, kind, payload, occurred_at, created_at) \
             VALUES (?1, 'A', ?1, ?2, 'runner', ?3, '{}', '2026-10-10T09:05:00Z', '2026-10-10T09:05:00Z')",
            &[&format!("e{n}"), &n.to_string(), kind],
        )
        .await;
    }
    let project = project.id.to_string();
    let rows = question_rows(&repo, &project, None).await.unwrap();
    let got: Vec<(&str, i64, i64, i64)> = rows
        .iter()
        .map(|r| (r.class.as_str(), r.asked, r.answered, r.granted))
        .collect();
    // approval: 3 asked, 2 answered, 1 granted; choice: 1 asked, 1 answered, 1 granted.
    assert_eq!(got, [("approval", 3, 2, 1), ("choice", 1, 1, 1)]);
    assert_eq!(rows[0].harness.as_deref(), Some("claude-code"));
    assert_eq!(
        action_counts(&repo, &project, None).await.unwrap(),
        [(Some("claude-code".to_owned()), 8)]
    );
}
