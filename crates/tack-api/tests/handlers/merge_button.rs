//! The human triggers of the merge: the Merge button (`POST /api/attempts/{id}/merge`),
//! the move to Done (`merge_on_done`), and MCP's mark (`X-Tack-Client: mcp`,
//! `origin: "mcp"` on the run). The GitHub is a mock; prill is a fake script.

use crate::autonomy::{
    Board, Setup, board, comments, fake_prill, finished_attempt, fixture, merge_row, put_autonomy,
};
use crate::common;
use crate::mrp::{BASE_REVISION, ClaimedAttempt, execution_request_body, operator_headers};
use axum::http::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{body_json, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The fake GitHub for `acme/widgets`: CI as the named fixture, and the merge
/// answering `merge_status` (200 merges at the head sha, 403 refuses the token).
async fn github(ci: &str, merge_status: u16, merges: u64) -> MockServer {
    let gh = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch": "trunk"})))
        .mount(&gh)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/widgets/pulls"))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            json!({"number": 7, "html_url": "https://github.com/acme/widgets/pull/7"}),
        ))
        .mount(&gh)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/repos/acme/widgets/commits/[0-9a-f]+/check-runs$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(ci)))
        .mount(&gh)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/repos/acme/widgets/commits/[0-9a-f]+/status$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("status-success.json")))
        .mount(&gh)
        .await;
    let answer = if merge_status == 200 {
        ResponseTemplate::new(200).set_body_json(fixture("merge-200.json"))
    } else {
        ResponseTemplate::new(merge_status).set_body_json(fixture("merge-403.json"))
    };
    Mock::given(method("PUT"))
        .and(path("/repos/acme/widgets/pulls/7/merge"))
        .and(body_json(json!({"sha": BASE_REVISION})))
        .respond_with(answer)
        .expect(merges)
        .mount(&gh)
        .await;
    gh
}

/// A board whose project grants no automatic merge (L3) and merges on Done, with
/// one finished attempt waiting for a person.
async fn finished(
    root: &std::path::Path,
    gh: &MockServer,
    setup: Setup,
) -> (Board, ClaimedAttempt) {
    let b = board(root, Some(&gh.uri()), &fake_prill(root)).await;
    let (status, put) = put_autonomy(&b, &setup).await;
    assert_eq!(status, StatusCode::OK, "{put}");
    sqlx::query("UPDATE projects SET merge_on_done = 1 WHERE id = ?")
        .bind(&b.project_id)
        .execute(b.state.repo.pool())
        .await
        .unwrap();
    let a = finished_attempt(&b, &setup).await;
    (b, a)
}

fn l3() -> Setup {
    Setup {
        domain_level: "L3",
        ..Setup::default()
    }
}

async fn press_merge(b: &Board, a: &ClaimedAttempt) -> (StatusCode, Value) {
    common::send(
        &b.app,
        "POST",
        &format!("/api/attempts/{}/merge", a.attempt_id),
        Value::Null,
        &operator_headers(),
    )
    .await
}

/// What `GET /api/executions/{id}/attempts` says the button may do.
async fn merge_state(b: &Board, a: &ClaimedAttempt) -> Value {
    let (status, body) = common::send(
        &b.app,
        "GET",
        &format!("/api/executions/{}/attempts", a.request_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"][0]["merge"].clone()
}

async fn move_to_done(b: &Board, client: Option<&str>) {
    let mut headers = operator_headers();
    if let Some(client) = client {
        headers.push(("x-tack-client", client));
    }
    let (status, body) = common::send(
        &b.app,
        "PATCH",
        &format!("/api/items/{}", b.item_id),
        json!({"status": "Done"}),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The merge on Done runs in the background: wait for its row or its word.
    for _ in 0..100 {
        let done: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tack_merges WHERE trigger = 'done'")
                .fetch_one(b.state.repo.pool())
                .await
                .unwrap();
        if done > 0 || comments(b).await.contains("over MCP") {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the move to Done left no merge row and no word");
}

async fn mark_mcp(b: &Board, a: &ClaimedAttempt) {
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot = json_set(request_snapshot, '$.origin', 'mcp') WHERE id = ?",
    )
    .bind(&a.request_id)
    .execute(b.state.repo.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn the_button_merges_with_the_head_sha() {
    let root = tempfile::tempdir().unwrap();
    let gh = github("check-runs-all-success.json", 200, 1).await;
    let (b, a) = finished(root.path(), &gh, l3()).await;
    assert_eq!(merge_state(&b, &a).await["enabled"], true);

    let (status, body) = press_merge(&b, &a).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["merge_sha"],
        "e5bd3914e2e596debea16f433f57875b5b90bcd6"
    );
    let row = merge_row(&b, &a.attempt_id).await.expect("a row");
    assert_eq!(row.0, "button");
    assert!(row.3.is_none() && row.4.is_some(), "{row:?}");
    assert!(comments(&b).await.contains("with the Merge button"));
    let after = merge_state(&b, &a).await;
    assert_eq!(after["enabled"], false);
    assert_eq!(after["reason"], "Already merged");
    assert_eq!(after["merged"]["trigger"], "button");
    let (status, item) = common::send(
        &b.app,
        "GET",
        &format!("/api/items/{}", b.item_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["item"]["merge_note"], Value::Null, "{item}");
}

/// One row per press GitHub's facts hold: the setup, whether the button stays
/// enabled after it (CI is read on the next press; a token refusal disables it
/// until the token changes), and the words the press and the button give.
#[tokio::test]
async fn each_held_press_says_why_and_the_button_says_whether_to_retry() {
    let rows: Vec<(&str, Setup, &str, u16, bool, bool, &str)> = vec![
        (
            "no pull request",
            Setup {
                linked: false,
                ..l3()
            },
            "check-runs-all-success.json",
            200,
            false,
            false,
            "not linked to a GitHub issue",
        ),
        (
            "CI running",
            l3(),
            "check-runs-in-progress.json",
            200,
            true,
            true,
            "CI is still running",
        ),
        (
            "CI red",
            l3(),
            "check-runs-one-failure.json",
            200,
            true,
            true,
            "CI failed",
        ),
        (
            "token refused",
            l3(),
            "check-runs-all-success.json",
            403,
            true,
            false,
            "Contents: write",
        ),
    ];
    for (name, setup, ci, merge_status, pressed, enabled_after, words) in rows {
        let root = tempfile::tempdir().unwrap();
        let gh = github(ci, merge_status, u64::from(pressed && merge_status == 403)).await;
        let (b, a) = finished(root.path(), &gh, setup).await;
        let (status, body) = press_merge(&b, &a).await;
        assert_eq!(status, StatusCode::CONFLICT, "{name}: {body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains(words),
            "{name}: {body}"
        );
        let merge = merge_state(&b, &a).await;
        assert_eq!(merge["enabled"], enabled_after, "{name}: {merge}");
        assert!(
            merge["reason"].as_str().unwrap_or_default().contains(words),
            "{name}: {merge}"
        );
        let (status, item) = common::send(
            &b.app,
            "GET",
            &format!("/api/items/{}", b.item_id),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {item}");
        assert!(
            item["item"]["merge_note"]
                .as_str()
                .unwrap_or_default()
                .contains(words),
            "{name}: {item}"
        );
    }
}

#[tokio::test]
async fn a_move_to_done_merges_and_a_move_over_mcp_does_not() {
    let root = tempfile::tempdir().unwrap();
    let gh = github("check-runs-all-success.json", 200, 1).await;
    let (b, a) = finished(root.path(), &gh, l3()).await;
    move_to_done(&b, None).await;
    let row = merge_row(&b, &a.attempt_id).await.expect("a row");
    assert_eq!(row.0, "done");
    assert!(row.3.is_none() && row.4.is_some(), "{row:?}");
    assert!(comments(&b).await.contains("when the task moved to Done"));

    let root = tempfile::tempdir().unwrap();
    let gh = github("check-runs-all-success.json", 200, 0).await;
    let (b, a) = finished(root.path(), &gh, l3()).await;
    move_to_done(&b, Some("mcp")).await;
    assert!(merge_row(&b, &a.attempt_id).await.is_none());
    assert!(
        comments(&b)
            .await
            .contains("Moved to Done over MCP: not merged; use Merge")
    );
}

#[tokio::test]
async fn an_mcp_origin_run_is_refused_by_done_and_auto_and_accepted_by_the_button() {
    let root = tempfile::tempdir().unwrap();
    let gh = github("check-runs-all-success.json", 200, 1).await;
    let (b, a) = finished(root.path(), &gh, Setup::default()).await;
    mark_mcp(&b, &a).await;

    tack_api::autonomy::tick(&b.state).await;
    let row = merge_row(&b, &a.attempt_id).await.expect("a row");
    assert_eq!(row.0, "auto");
    assert!(row.3.unwrap_or_default().contains("came through MCP"));

    move_to_done(&b, None).await;
    let row = merge_row(&b, &a.attempt_id).await.expect("a row");
    assert_eq!(row.0, "done");
    assert!(row.3.unwrap_or_default().contains("came through MCP"));
    assert!(row.4.is_none());

    let (status, body) = press_merge(&b, &a).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(merge_row(&b, &a.attempt_id).await.unwrap().0, "button");
}

#[tokio::test]
async fn a_run_created_with_origin_mcp_keeps_it_in_the_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let gh = github("check-runs-all-success.json", 200, 0).await;
    let (b, a) = finished(root.path(), &gh, l3()).await;
    let profile: String =
        sqlx::query_scalar("SELECT agent_profile_id FROM execution_requests WHERE id = ?")
            .bind(&a.request_id)
            .fetch_one(b.state.repo.pool())
            .await
            .unwrap();
    for (key, origin, kept) in [
        ("o-mcp", Some("mcp"), true),
        ("o-other", Some("cli"), false),
    ] {
        let mut body = execution_request_body(&b.item_id, key, &a.runner_id, &profile, None);
        if let Some(origin) = origin {
            body["origin"] = json!(origin);
        }
        let (status, created) =
            common::send(&b.app, "POST", "/api/executions", body, &operator_headers()).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let origin: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(request_snapshot, '$.origin') FROM execution_requests WHERE id = ?",
        )
        .bind(created["request_id"].as_str().unwrap())
        .fetch_one(b.state.repo.pool())
        .await
        .unwrap();
        assert_eq!(origin.as_deref(), kept.then_some("mcp"), "{key}");
    }
}
