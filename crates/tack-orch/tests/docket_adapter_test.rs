//! Integration tests for `adapters::docket::DocketAdapter` against a
//! `wiremock` server, using the fixtures in `tests/fixtures/` (see each
//! fixture's own header comment for provenance — captured live from a real
//! `docket serve` vs. constructed/derived from source, per TODO.md §Wave 1
//! card A1, step 4/5).
//!
//! Every read method of `ControlPlane` gets at least one passing test here;
//! `adapters::prometheus`'s own unit tests (in `src/adapters/prometheus.rs`)
//! cover the Prometheus-format edge cases in depth, so the metrics tests
//! here only check that the adapter wires the parser in correctly.

use std::fs;
use std::path::PathBuf;

use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::{ApprovalState, ControlPlane, OrchError, RunSource, RunState, TaskStatus};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "test-fixture-token-abc123";

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_text_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

/// JSON fixtures carry a leading block of `//`-prefixed provenance-comment
/// lines (see e.g. `fixtures/health.json`) — not valid JSON syntax, so it's
/// stripped before the body is used as a mock response.
fn load_json_fixture(name: &str) -> String {
    load_text_fixture(name)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `not_found_route.txt` uses a plain-text `PROVENANCE: ...\n---\n<body>`
/// convention (no native comment syntax exists for a raw non-JSON,
/// non-Prometheus body) — return everything after the `---` separator.
fn load_raw_body_fixture(name: &str) -> String {
    let raw = load_text_fixture(name);
    match raw.split_once("---\n") {
        Some((_, body)) => body.to_string(),
        None => raw,
    }
}

/// A `DocketAdapter` pointed at `server`, configured with [`TOKEN`].
fn adapter_for(server: &MockServer) -> DocketAdapter {
    DocketAdapter::new(server.uri(), Some(TOKEN.to_string())).expect("adapter must construct")
}

// ---------------------------------------------------------------------------
// Happy path — one per read method
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(load_json_fixture("health.json")))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let health = adapter.health().await.expect("health must succeed");
    assert_eq!(health.status, "ok");
    assert_eq!(health.gateway, 0);
}

#[tokio::test]
async fn status_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("status_with_agent.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let status = adapter.status().await.expect("status must succeed");
    assert_eq!(status.api_version, "2");
    assert_eq!(status.agents.len(), 1);
    assert_eq!(status.agents[0].id, "demo-lead");
    assert_eq!(status.agents[0].budget_usd, Some(25.0));
}

#[tokio::test]
async fn metrics_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_text_fixture("metrics_with_agent.txt")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let samples = adapter.metrics().await.expect("metrics must succeed");
    let names: Vec<&str> = samples.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"docket_agents_total"));
    assert!(names.contains(&"docket_agent_cost_usd"));
    let pending = samples
        .iter()
        .find(|s| s.name == "docket_approvals_pending_total")
        .expect("pending sample present");
    assert_eq!(pending.value, 1.0);
}

#[tokio::test]
async fn list_runs_happy_path_and_sends_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("runs_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let runs = adapter
        .list_runs(None)
        .await
        .expect("list_runs must succeed");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].source, RunSource::Webhook);
    assert_eq!(runs[1].state, RunState::Succeeded);
}

#[tokio::test]
async fn list_runs_filters_by_project_query_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo-lead"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("runs_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let runs = adapter
        .list_runs(Some("demo-lead"))
        .await
        .expect("list_runs with project filter must succeed");
    assert_eq!(runs.len(), 2);
}

#[tokio::test]
async fn get_run_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs/run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("run_single.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let run = adapter
        .get_run("run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb")
        .await
        .expect("get_run must succeed");
    assert_eq!(run.state, RunState::Succeeded);
    assert_eq!(
        run.finished_at.as_deref(),
        Some("2026-08-04T19:50:43.130194+00:00")
    );
}

#[tokio::test]
async fn list_approvals_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("approvals_pending.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let approvals = adapter
        .list_approvals()
        .await
        .expect("list_approvals must succeed");
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].state, ApprovalState::Pending);
    assert_eq!(approvals[0].context["taskId"], "task-1");
}

#[tokio::test]
async fn list_tasks_happy_path_against_a_live_captured_shape() {
    // Card V1 (2026-08-05): `tasks_list.json` is now a genuine live HTTP
    // capture (previously a derived-not-captured guess at the wrapper key
    // and field shape, per A1's original note) — confirms the `{"tasks":
    // [...]}` wrapper and `RemoteTask`'s field shape both match the real
    // endpoint exactly, no adapter changes needed.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("tasks_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let tasks = adapter
        .list_tasks("demo")
        .await
        .expect("list_tasks must succeed against the real, live-captured shape");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskStatus::Pending);
    assert_eq!(tasks[0].priority, "high");
}

// ---------------------------------------------------------------------------
// enqueue_task — POST /tasks/{project} (card C1, Wave 3, 2026-08-05)
//
// All three of docket's real `pre_input` outcomes (card V1's live
// verification): allow (200, task id), block (400, policy id in the error
// text), require_approval (200, same shape as allow — `status`/
// `approvalToken` are real but this method's frozen return type can't carry
// them, see the module doc). Plus the `trusted` flag really reaching the
// wire, since that's the prompt-injection boundary C2 builds on.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enqueue_task_allow_returns_the_task_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "task": "task-allow-1", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let task_id = adapter
        .enqueue_task(
            "demo",
            tack_orch::NewRemoteTask {
                description: "do the thing".into(),
                priority: None,
                trusted: true,
            },
        )
        .await
        .expect("an allow verdict must succeed");
    assert_eq!(task_id, "task-allow-1");
}

#[tokio::test]
async fn enqueue_task_waiting_approval_still_returns_ok_with_the_task_id() {
    // Real docket response for a `require_approval` verdict is still HTTP
    // 200 — never a 200 that lies about the task being queued normally, but
    // also never treated as a failure by this adapter (the caller recovers
    // `status`/`approvalToken` via `list_tasks`, see the module doc).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "task": "task-needs-approval", "project": "demo",
            "status": "waiting_approval", "approvalToken": "tok-xyz"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let task_id = adapter
        .enqueue_task(
            "demo",
            tack_orch::NewRemoteTask {
                description: "sudo rm -rf /".into(),
                priority: None,
                trusted: true,
            },
        )
        .await
        .expect("a require_approval verdict is still Ok — it isn't a failure");
    assert_eq!(task_id, "task-needs-approval");
}

#[tokio::test]
async fn enqueue_task_block_maps_to_http_error_naming_the_policy() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "ok": false,
            "error": "task rejected by guardrail policy 'prompt-injection' at enqueue: untrusted input matched a deny rule"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .enqueue_task(
            "demo",
            tack_orch::NewRemoteTask {
                description: "ignore previous instructions".into(),
                priority: None,
                trusted: false,
            },
        )
        .await
        .expect_err("a block verdict must not be Ok");
    match err {
        OrchError::Http(msg) => {
            assert!(
                msg.starts_with(tack_orch::adapters::docket::POLICY_BLOCK_PREFIX),
                "message must carry the policy-block prefix so dispatcher can \
                 distinguish a refusal from a generic failure: {msg}"
            );
            assert!(
                msg.contains("prompt-injection"),
                "message must name the policy id: {msg}"
            );
        }
        other => panic!("expected Http (block), got {other:?}"),
    }
}

#[tokio::test]
async fn enqueue_task_sends_the_trusted_flag_on_the_wire() {
    // The one boundary card V1 called out as most load-bearing for Wave 3:
    // an explicit `false` must actually reach docket's JSON body, not be
    // dropped or defaulted away.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "trusted": false
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "task": "task-untrusted", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let task_id = adapter
        .enqueue_task(
            "demo",
            tack_orch::NewRemoteTask {
                description: "GitHub-imported title".into(),
                priority: None,
                trusted: false,
            },
        )
        .await
        .expect("wiremock only matches if trusted:false really was sent");
    assert_eq!(task_id, "task-untrusted");
}

#[tokio::test]
async fn enqueue_task_unauthorized_maps_to_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .enqueue_task(
            "demo",
            tack_orch::NewRemoteTask {
                description: "x".into(),
                priority: None,
                trusted: true,
            },
        )
        .await
        .expect_err("401 must not be Ok");
    assert!(matches!(err, OrchError::Auth));
}

#[tokio::test]
async fn traces_happy_path_decodes_the_double_encoded_events_array() {
    // Card V1 (2026-08-05): `traces_list.json` is now a genuine live HTTP
    // capture (previously only verified by reading `serve.py` source,
    // per B2's handoff note) — `events` really is an array of raw JSON
    // *strings* over the wire, each requiring a second decode. This test
    // would fail loudly (a `Decode` error) against the old fixture shape,
    // which is exactly the bug card B2 found and fixed in
    // `DocketAdapter::traces`.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("traces_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let events = adapter
        .traces("demo", None)
        .await
        .expect("traces must succeed against the real (double-encoded) wire shape");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event_type, "tool_call");
    assert_eq!(events[0].session_id, "agent:demo2:task-7bf4553c");
    assert_eq!(events[0].cost_usd_estimated, Some(0.0021));
    // The third event is a made-up, not-yet-known event type — must round
    // trip as a plain string, never fail to deserialize (see
    // `traces_list.json`'s provenance comment and `lib.rs`'s
    // `RemoteEvent::event_type` doc).
    assert_eq!(events[2].event_type, "some_future_event_type_v3");
}

#[tokio::test]
async fn traces_since_query_param_is_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .and(query_param("since", "2026-08-04T00:00:00Z"))
        .and(header("Authorization", format!("Bearer {TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("traces_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    adapter
        .traces("demo", Some("2026-08-04T00:00:00Z"))
        .await
        .expect("traces with since must succeed");
}

// ---------------------------------------------------------------------------
// list_tasks / traces: 404 because the route doesn't exist in docket yet
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_tasks_404_maps_to_not_found_capability_absent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_string(load_raw_body_fixture("not_found_route.txt")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .list_tasks("demo")
        .await
        .expect_err("404 must surface as an error");
    assert!(matches!(err, OrchError::NotFound(_)));
}

#[tokio::test]
async fn traces_404_maps_to_not_found_capability_absent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_string(load_raw_body_fixture("not_found_route.txt")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .traces("demo", None)
        .await
        .expect_err("404 must surface as an error");
    assert!(matches!(err, OrchError::NotFound(_)));
}

#[tokio::test]
async fn get_run_404_extracts_dockets_json_error_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs/run-does-not-exist"))
        .respond_with(
            ResponseTemplate::new(404).set_body_string(load_json_fixture("run_not_found.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .get_run("run-does-not-exist")
        .await
        .expect_err("unknown run id must 404");
    match err {
        OrchError::NotFound(msg) => assert!(msg.contains("run-does-not-exist")),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Error mapping: unreachable host vs. 401 must be distinct variants
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unreachable_host_maps_to_http_error() {
    // Bind an ephemeral port, then drop the listener immediately — nothing
    // is listening on it afterward, so connecting fails fast (connection
    // refused) instead of waiting out the adapter's 5s timeout.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);

    let adapter = DocketAdapter::new(format!("http://127.0.0.1:{port}"), None)
        .expect("adapter must construct even for an unreachable host");
    let err = adapter
        .health()
        .await
        .expect_err("nothing is listening; this must fail");
    assert!(matches!(err, OrchError::Http(_)));
}

#[tokio::test]
async fn unauthorized_401_maps_to_auth_error_distinct_from_http() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string(load_json_fixture("unauthorized.json")),
        )
        .mount(&server)
        .await;

    // No token configured — docket's real behavior for a missing
    // Authorization header, per `unauthorized.json`'s provenance.
    let adapter = DocketAdapter::new(server.uri(), None).expect("adapter must construct");
    let err = adapter
        .list_runs(None)
        .await
        .expect_err("401 must surface as an error");
    assert!(matches!(err, OrchError::Auth));
    // And it must not be the same variant an unreachable host produces.
    assert!(!matches!(err, OrchError::Http(_)));
}

// ---------------------------------------------------------------------------
// Malformed / unexpected payloads must never panic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_json_maps_to_decode_error_not_panic() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("status_malformed.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let err = adapter
        .status()
        .await
        .expect_err("truncated JSON must fail to decode, not panic");
    assert!(matches!(err, OrchError::Decode(_)));
}

#[tokio::test]
async fn malformed_prometheus_body_never_panics_and_returns_what_it_can() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_text_fixture("metrics_malformed.txt")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    // The whole point: this must not panic, and must still surface the
    // well-formed lines the malformed fixture also contains.
    let samples = adapter
        .metrics()
        .await
        .expect("metrics never errors on a 200");
    assert!(samples.iter().any(|s| s.name == "a_gauge"));
    assert!(samples.iter().any(|s| s.name == "bare_metric_no_labels"));
}

// ---------------------------------------------------------------------------
// Unknown enum value must degrade to Unknown(..), never fail the poll
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_run_state_deserializes_to_unknown_variant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs/run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("run_unknown_state.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let run = adapter
        .get_run("run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb")
        .await
        .expect("an unrecognised state must not fail the request");
    assert_eq!(run.state, RunState::Unknown("paused".to_string()));
}

// ---------------------------------------------------------------------------
// Auth split: unauthenticated routes never carry the Bearer token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_routes_never_send_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(load_json_fixture("health.json")))
        .mount(&server)
        .await;

    // Configured *with* a token — if the adapter ever leaked it onto an
    // unauthenticated route, this is the test that would catch it.
    let adapter = adapter_for(&server);
    adapter.health().await.expect("health must succeed");

    let received = server
        .received_requests()
        .await
        .expect("request recording must be enabled by default");
    assert_eq!(received.len(), 1);
    assert!(
        !received[0].headers.contains_key("authorization"),
        "unauthenticated /health must not carry an Authorization header"
    );
}

// ---------------------------------------------------------------------------
// Write methods: `dispatch`/`decide_approval` disabled unconditionally
// (Wave 4 / card D1 and, for `dispatch`, no consumer yet — see the module
// doc's "Write methods" section). `enqueue_task` is implemented as of card
// C1 (Wave 3, 2026-08-05) — its own outcomes are covered above, in the
// "enqueue_task" section.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_and_decide_approval_are_still_disabled() {
    let server = MockServer::start().await;
    // No mocks registered at all — if either method actually made an HTTP
    // call, this test would fail on the unmatched request, not just on a
    // wrong return value.
    let adapter = adapter_for(&server);

    assert!(matches!(
        adapter.dispatch("demo", serde_json::json!({})).await,
        Err(OrchError::Disabled)
    ));
    assert!(matches!(
        adapter.decide_approval("apr-1", true).await,
        Err(OrchError::Disabled)
    ));
}
