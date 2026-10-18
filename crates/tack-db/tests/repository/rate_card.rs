//! `rate_card::spend_by_model`: one query over the attempts, per model, per
//! pull-request repository and per team, from `by_model` when an attempt
//! carries it and from the run's one model when it does not.

use crate::common;

use tack_db::repo::rate_card::{SpendRow, spend_by_model};

#[tokio::test]
async fn spend_by_model_splits_an_attempt_that_used_two_models() {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await.id.to_string();
    let at = "2026-10-14T09:00:00Z";
    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, ?, ?)",
    )
    .bind(at)
    .bind(at)
    .execute(repo.pool())
    .await
    .unwrap();

    // (request, requested model, team snapshot, actual_execution, usage)
    let tokens = |i: i64, o: i64| {
        format!(
            r#""tokens_in":{{"value":{i},"source":"measured"}},"tokens_out":{{"value":{o},"source":"measured"}}"#
        )
    };
    let rows = [
        (
            "R1",
            "ignored/for-by-model",
            None,
            "{}".to_owned(),
            format!(
                r#"{{{},"by_model":{{"opaque/a":{{"input":100,"output":10}},"opaque/b":{{"input":50,"output":5,"cache_read":25}}}}}}"#,
                tokens(175, 15)
            ),
        ),
        (
            "R2",
            "opaque/x",
            None,
            r#"{"model":"opaque/a"}"#.to_owned(),
            format!("{{{}}}", tokens(200, 20)),
        ),
        (
            "R3",
            "opaque/c",
            Some("alpha"),
            "{}".to_owned(),
            format!("{{{}}}", tokens(7, 3)),
        ),
    ];
    for (id, requested, team, actual, usage) in &rows {
        let snapshot = match team {
            Some(team) => format!(
                r#"{{"resolved_agent_profile":{{"tool_policy":{{"docket":{{"team":"{team}"}}}}}}}}"#
            ),
            None => "{}".to_owned(),
        };
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
             repository_snapshot, permission_policy, metadata, requested_model, request_snapshot, \
             created_at, updated_at) \
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&item)
        .bind(id)
        .bind(requested)
        .bind(snapshot)
        .bind(at)
        .bind(at)
        .execute(repo.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
             fencing_token, lease_issued_at, lease_expires_at, created_at, updated_at, \
             actual_execution, usage) VALUES (?, ?, 1, 'run', 1, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("A-{id}"))
        .bind(id)
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(actual)
        .bind(usage)
        .execute(repo.pool())
        .await
        .unwrap();
    }

    let project = project.id.to_string();
    let mut got = spend_by_model(repo.pool(), &project, None).await.unwrap();
    got.sort_by(|a, b| a.model.cmp(&b.model));
    let row =
        |model: &str, team: Option<&str>, tokens_in, cache_read, tokens_out, attempts| SpendRow {
            model: Some(model.into()),
            repo: None,
            team: team.map(Into::into),
            tokens_in,
            cache_read,
            tokens_out,
            attempts,
        };
    // `tokens_in` of a `by_model` entry counts its cache reads and writes, and
    // `cache_read` says how many of them were reads.
    assert_eq!(
        got,
        vec![
            row("opaque/a", None, 300, 0, 30, 2),
            row("opaque/b", None, 75, 25, 5, 1),
            row("opaque/c", Some("alpha"), 7, 0, 3, 1),
        ]
    );

    // The window is on the attempt's creation.
    let later = spend_by_model(repo.pool(), &project, Some("2026-10-15T00:00:00Z"))
        .await
        .unwrap();
    assert!(later.is_empty(), "{later:?}");

    // A card prices each row: $1 per token in, $2 per token out, model c unlisted.
    let card: tack_core::rate_card::RateCard = serde_json::from_value(serde_json::json!({
        "as_of": "2026-10-14",
        "models": {
            "opaque/a": {"input_per_mtok": 1e6, "output_per_mtok": 2e6},
            "opaque/b": {"input_per_mtok": 1e6, "output_per_mtok": 2e6}
        },
        "ci_minute": 0.0,
        "human_hour": 0.0
    }))
    .unwrap();
    let priced: f64 = got
        .iter()
        .filter_map(|r| {
            tack_core::rate_card::price(&card, r.model.as_deref()?, r.tokens_in, r.tokens_out)
        })
        .sum();
    assert!(
        (priced - (300.0 + 60.0 + 75.0 + 10.0)).abs() < 1e-6,
        "{priced}"
    );
}
