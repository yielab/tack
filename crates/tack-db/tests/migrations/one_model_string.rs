//! Migration 086: a model is one string. Seeds each old shape on a database
//! stopped at 085, upgrades in place, and reads back the new one.

use serde_json::{Value, json};
use tack_db::{init_pool, migrations};

const OLD_COMPLETION: &str = "085_request_waiting";

async fn seed(pool: &sqlx::SqlitePool) {
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name, default_model) VALUES ('p-explicit', 'ws', 'A', '{\"kind\":\"explicit\",\"provider\":\"openai\",\"model_id\":\"opaque/model-alpha\"}')",
        "INSERT INTO projects (id, workspace_id, name, default_model) VALUES ('p-auto', 'ws', 'B', '{\"kind\":\"auto\"}')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p-none', 'ws', 'C')",
    ] {
        sqlx::query(sql).execute(pool).await.expect(sql);
    }
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status, run_settings) VALUES ('item', 'p-none', 'T', 'To Do', ?)",
    )
    .bind(r#"{"harness":"codex","model_provider":"openai","model_id":"opaque/model-alpha","push":true}"#)
    .execute(pool)
    .await
    .expect("item");
    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, protocol_version, created_at, updated_at) \
         VALUES ('runner', 'r', 'h', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .expect("runner");

    for (id, provider, model) in [
        ("req-named", Some("openai"), Some("opaque/model-alpha")),
        ("req-none", None, None),
    ] {
        let snapshot = json!({
            "request_id": id,
            "requested_harness_kind": "codex",
            "requested_model_provider": provider,
            "requested_model_id": model,
        });
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
             requested_harness_kind, requested_model_provider, requested_model_id, \
             repository_snapshot, permission_policy, created_at, updated_at, request_snapshot) \
             VALUES (?, 'item', 's', ?, ?, 'exact_runner', 'runner', '{}', 'codex', ?, ?, '{}', '{}', \
             '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z', ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(provider)
        .bind(model)
        .bind(snapshot.to_string())
        .execute(pool)
        .await
        .expect("request");
    }

    // (attempt, source the old runner reported, model id it reported)
    for (n, (id, source, model)) in [
        ("att-reported", "harness_reported", "served-model"),
        (
            "att-requested",
            "requested_not_confirmed",
            "opaque/model-alpha",
        ),
        ("att-none", "not_observed", "unknown"),
    ]
    .into_iter()
    .enumerate()
    {
        let actual = json!({
            "harness_kind": "codex",
            "model_provider": "openai",
            "model_id": model,
            "model_observation_source": source,
        });
        sqlx::query(
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, \
             lease_issued_at, lease_expires_at, actual_execution, created_at, updated_at) \
             VALUES (?, 'req-named', ?, 'runner', ?, 'x', 'x', ?, 'x', 'x')",
        )
        .bind(id)
        .bind(n as i64 + 1)
        .bind(n as i64 + 1)
        .bind(actual.to_string())
        .execute(pool)
        .await
        .expect("attempt");
    }
}

async fn scalar(pool: &sqlx::SqlitePool, sql: &str) -> Option<String> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect(sql)
}

async fn json_of(pool: &sqlx::SqlitePool, sql: &str) -> Value {
    serde_json::from_str(&scalar(pool, sql).await.expect("a value")).expect("json")
}

#[tokio::test]
async fn migration_086_rewrites_every_old_model_shape_to_one_string() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, OLD_COMPLETION)
        .await
        .expect("up to 085");
    seed(&pool).await;
    migrations::run_all(&pool).await.expect("upgrade in place");

    // Requests: the column and the stored snapshot carry `requested_model`.
    assert_eq!(
        scalar(
            &pool,
            "SELECT requested_model FROM execution_requests WHERE id = 'req-named'"
        )
        .await,
        Some("opaque/model-alpha".into())
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT requested_model FROM execution_requests WHERE id = 'req-none'"
        )
        .await,
        None
    );
    assert_eq!(
        json_of(
            &pool,
            "SELECT request_snapshot FROM execution_requests WHERE id = 'req-named'"
        )
        .await,
        json!({"request_id": "req-named", "requested_harness_kind": "codex", "requested_model": "opaque/model-alpha"})
    );
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('execution_requests')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        !columns.iter().any(|c| c.starts_with("requested_model_")),
        "{columns:?}"
    );

    // Attempts: `model` and `model_source`, the old keys gone.
    for (id, model, source) in [
        ("att-reported", json!("served-model"), "reported"),
        ("att-requested", json!("opaque/model-alpha"), "requested"),
        ("att-none", Value::Null, "agent_default"),
    ] {
        assert_eq!(
            json_of(
                &pool,
                &format!("SELECT actual_execution FROM execution_attempts WHERE id = '{id}'")
            )
            .await,
            json!({"harness_kind": "codex", "model": model, "model_source": source}),
            "{id}"
        );
    }

    // Projects: an explicit default keeps its model id; auto and none become NULL.
    for (id, expected) in [
        ("p-explicit", Some("opaque/model-alpha")),
        ("p-auto", None),
        ("p-none", None),
    ] {
        assert_eq!(
            scalar(
                &pool,
                &format!("SELECT default_model FROM projects WHERE id = '{id}'")
            )
            .await
            .as_deref(),
            expected,
            "{id}"
        );
    }

    // Items: the old pair becomes `model`.
    assert_eq!(
        json_of(&pool, "SELECT run_settings FROM items WHERE id = 'item'").await,
        json!({"harness": "codex", "model": "opaque/model-alpha", "push": true})
    );
}
