//! Enqueueing execution requests, snapshot validation, and the upgrade that
//! adds the execution tables.

use crate::repository::execution_fixture::Fixture;

use chrono::Duration;
use tack_db::{init_pool, migrations, repo::execution::EnqueueResult};

#[tokio::test]
async fn enqueue_rejects_malformed_and_contradictory_snapshots() {
    let fx = Fixture::new().await;
    let mut missing = fx.request("snapshot-missing", "snapshot-missing", "same");
    missing.request_snapshot = Box::leak(
        missing
            .request_snapshot
            .replace("\"metadata\":{\"source\":\"test\"}", "")
            .replace(",,", ",")
            .replace(",}", "}")
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(missing, fx.clock.now())
            .await
            .is_err()
    );

    let mut malformed = fx.request("snapshot-malformed", "snapshot-malformed", "same");
    malformed.request_snapshot = "{not-json";
    assert!(
        fx.repo
            .enqueue_execution(malformed, fx.clock.now())
            .await
            .is_err()
    );

    let mut contradictory = fx.request("snapshot-cross", "snapshot-cross", "same");
    contradictory.request_snapshot = Box::leak(
        contradictory
            .request_snapshot
            .replace(
                "\"request_id\":\"snapshot-cross\"",
                "\"request_id\":\"other\"",
            )
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(contradictory, fx.clock.now())
            .await
            .is_err()
    );

    let mut wrong_created_at = fx.request("snapshot-clock", "snapshot-clock", "same");
    wrong_created_at.request_snapshot = Box::leak(
        wrong_created_at
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(wrong_created_at, fx.clock.now())
            .await
            .is_err()
    );

    for (suffix, from, to) in [
        (
            "created-by",
            r#""created_by":{"source":"operator","subject_id":"test"}"#,
            r#""created_by":"operator""#,
        ),
        (
            "profile-policy",
            r#""tool_policy":{"mode":"safe"}"#,
            r#""tool_policy":"safe""#,
        ),
        ("budgets", r#""budgets":{"limit":1}"#, r#""budgets":1"#),
        ("repository-kind", r#""kind":"git""#, r#""kind":1"#),
        (
            "metadata",
            r#""metadata":{"source":"test"}"#,
            r#""metadata":false"#,
        ),
        (
            "environment",
            r#""value":"test","secret_reference":null"#,
            r#""value":"test","secret_reference":"secret://mode""#,
        ),
    ] {
        let id = Box::leak(format!("snapshot-{suffix}").into_boxed_str());
        let mut invalid = fx.request(id, id, "same");
        invalid.request_snapshot =
            Box::leak(invalid.request_snapshot.replace(from, to).into_boxed_str());
        assert!(
            fx.repo
                .enqueue_execution(invalid, fx.clock.now())
                .await
                .is_err(),
            "{suffix}"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn old_schema_upgrades_to_all_nine_execution_tables() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .unwrap();
    migrations::run_all(&pool).await.unwrap();
    for table in [
        "agent_fleets",
        "agent_runners",
        "agent_fleet_members",
        "agent_profiles",
        "execution_requests",
        "execution_attempts",
        "execution_events",
        "execution_artifacts",
        "execution_decisions",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{table} missing after upgrade");
    }
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations WHERE name BETWEEN '039_agent_fleets' AND '048_execution_decisions'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(migration_count, 10);
}

#[tokio::test]
async fn enqueue_is_idempotent_and_conflicting_reuse_is_rejected() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-c", "key-a", "different").await,
        EnqueueResult::Conflict
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn enqueue_replay_compares_frozen_snapshot_before_current_time() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    fx.clock.advance(Duration::seconds(1));
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into()),
        "an exact retry uses the original frozen snapshot rather than the new clock"
    );
    let mut changed_timestamp = fx.request("request-a", "key-a", "same");
    changed_timestamp.request_snapshot = Box::leak(
        changed_timestamp
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert_eq!(
        fx.repo
            .enqueue_execution(changed_timestamp, fx.clock.now())
            .await
            .unwrap(),
        EnqueueResult::Conflict,
        "same idempotency key with a changed frozen timestamp is not an exact replay"
    );
}

/// The operator's attempt list reads each attempt's pull request, review and merge for the whole
/// request, matching every outcome to its own attempt, and an unknown request has none.
#[tokio::test]
async fn attempts_come_with_their_own_pull_request_review_and_merge() {
    use tack_db::queries::execution_attempts::attempts_with_outcomes;

    let fx = Fixture::new().await;
    fx.enqueue("req-outcomes", "key-outcomes", "same").await;
    fx.claim("req-outcomes", Duration::seconds(30))
        .await
        .expect("claimed");
    // The first attempt is the claimed one; two more are planted so that each of the three
    // outcomes belongs to a different attempt and one attempt has none.
    for number in 2..=4 {
        sqlx::query("INSERT INTO execution_attempts (id,request_id,attempt_number,runner_id,fencing_token,state,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES (?,?,?,?,?,'failed','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')")
            .bind(format!("att-{number}"))
            .bind("req-outcomes")
            .bind(number)
            .bind("runner-a")
            .bind(number)
            .execute(fx.repo.pool())
            .await
            .expect("plant an attempt");
    }
    fx.repo
        .record_pull_request(
            "req-outcomes",
            "acme/widgets",
            7,
            "https://example.test/7",
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO attempt_reviews (attempt_id,verdict,reason,reviewed_at) VALUES ('att-2','accepted','fine','2026-01-02T00:00:00Z')")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&fx.item_id)
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    tack_test_support::seed::tack_merge_auto(
        &fx.repo,
        "att-3",
        &project_id,
        "2026-01-03T00:00:00Z",
    )
    .await;

    let rows = attempts_with_outcomes(&fx.repo, "req-outcomes")
        .await
        .unwrap();
    let ids: Vec<_> = rows.iter().map(|row| row.attempt.id.as_str()).collect();
    assert_eq!(ids, ["req-outcomes", "att-2", "att-3", "att-4"]);
    let pull_request = |row: &tack_db::queries::execution_attempts::AttemptWithOutcomes| {
        row.pull_request.as_ref().map(|pr| pr.number)
    };
    assert_eq!(pull_request(&rows[0]), Some(7));
    assert!(rows[0].review.is_none() && rows[0].merge.is_none());
    assert_eq!(rows[1].review.as_ref().unwrap().verdict, "accepted");
    assert!(rows[1].pull_request.is_none() && rows[1].merge.is_none());
    assert_eq!(rows[2].merge.as_ref().unwrap().trigger, "auto");
    assert!(rows[2].pull_request.is_none() && rows[2].review.is_none());
    assert!(rows[3].pull_request.is_none() && rows[3].review.is_none() && rows[3].merge.is_none());

    assert!(
        attempts_with_outcomes(&fx.repo, "no-such-request")
            .await
            .unwrap()
            .is_empty()
    );
}
