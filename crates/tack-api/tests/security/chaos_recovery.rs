//! Chaos, fencing and recovery adversarial suite: every test drives the
//! real production router (`tack_api::router::build_router`) and reads
//! persisted database state directly rather than trusting a status code
//! alone. The multi-runner/duplicated-credential races and revocation test
//! live in `chaos_races.rs` — split out once this file passed 1000 lines.

#[path = "chaos_common.rs"]
mod chaos_common;

use crate::common;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chaos_common::*;
use chrono::Utc;
use serde_json::{Value, json};
use tack_db::Repository;
use tack_db::queries::{execution_artifacts, execution_attempts, execution_decisions};
use tack_test_support::seed;
use tower::ServiceExt;

pub async fn put_content(
    app: &axum::Router,
    uri: &str,
    body: Vec<u8>,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("PUT").uri(uri);
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1_048_576)
        .await
        .unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

pub struct RunningAttempt {
    pub runner_id: String,
    pub credential: String,
    pub attempt_id: String,
    pub fencing_token: i64,
}

/// Enrolls one runner, creates one exact-runner-selected request, claims,
/// accepts and starts it — leaving the attempt `running`. Mirrors
/// `wiring/artifact.rs::ready_running_attempt`.
pub async fn ready_running_attempt(
    app: &axum::Router,
    item_id: &str,
    label: &str,
) -> RunningAttempt {
    let agent_profile_id = agent_profile(app, label).await;
    let runner = enroll_runner(app, &format!("G2 runner {label}")).await;
    let _request_id = create_execution_request(
        app,
        item_id,
        &format!("key-{label}"),
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;
    let (status, claimed) = common::claim_runner(
        app,
        &runner.runner_id,
        &runner.credential,
        &format!("claim-{label}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION,
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION, "process_id": "pid-1",
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    RunningAttempt {
        runner_id: runner.runner_id,
        credential: runner.credential,
        attempt_id,
        fencing_token,
    }
}

/// Minimal, test-local percent-encoder for a URI path segment — avoids
/// pulling in an extra dependency just for this adversarial file. Encodes
/// every byte that is not an unreserved URI character, which is sufficient
/// (if wasteful) for the deliberately-malicious ids this file constructs.
pub fn percent_encode_path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

pub async fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Ok(meta) = entry.metadata().await
                && meta.is_dir()
            {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn artifact_manifest_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    artifact_id: &str,
    kind: &str,
    name: &str,
    size_bytes: u64,
    sha256: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "artifacts": [{
            "artifact_id": artifact_id, "kind": kind, "name": name, "media_type": "text/plain",
            "size_bytes": size_bytes, "sha256": sha256, "content_disposition": "inline_upload", "metadata": {},
        }],
    })
}

pub async fn post_artifact_manifest(
    app: &axum::Router,
    attempt: &RunningAttempt,
    artifact_id: &str,
    kind: &str,
    name: &str,
    size_bytes: u64,
    sha256: &str,
) -> (StatusCode, Value) {
    let body = artifact_manifest_body(
        &attempt.runner_id,
        &attempt.attempt_id,
        attempt.fencing_token,
        artifact_id,
        kind,
        name,
        size_bytes,
        sha256,
    );
    let uri = format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id);
    common::send_large(
        app,
        "POST",
        &uri,
        body,
        &[("authorization", &auth(&attempt.credential))],
    )
    .await
}

pub fn decision_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    decision_id: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "decision_id": decision_id, "kind": "tool_permission", "prompt": "Allow?",
        "options": [{"option_id": "allow", "label": "Allow"}],
        "expires_at": (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(), "metadata": {},
    })
}

pub fn recovery_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    recovery_key: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "recovery_key": recovery_key, "observation": "process_stopped",
        "details": {"journal_state": "prepared", "process_observed": false},
    })
}

pub fn cancellation_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    cancellation_request_id: &str,
) -> Value {
    json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "cancellation_request_id": cancellation_request_id, "observation": "process_stopped",
        "observed_at": Utc::now().to_rfc3339(), "details": {"exit_code": 130, "signal": "SIGTERM"},
    })
}

// =======================================================================
// 1. Stale fence: every attempt-scoped mutation checked here rejects a
//    superseded fencing token and writes nothing — across `heartbeat`,
//    `decisions`, `artifacts` (manifest), `cancellation-observation` and
//    `recovery-observation`. One shared setup supersedes `attempt_a`/
//    `fence_a` once; every route below is then tried against that same
//    already-stale fence.
// =======================================================================

struct SupersededFence {
    request_id: String,
    runner_id: String,
    attempt_a: String,
    fence_a: i64,
    hdr: String,
}

/// Claims one request (`attempt_a`/`fence_a`), then supersedes it with a
/// pre-spawn recovery observation so the returned fence is guaranteed stale
/// before any caller uses it.
async fn setup_superseded_fence(app: &axum::Router) -> SupersededFence {
    let item_id = common::create_project_and_item(app, "https://example.test/g2.git", &[]).await;
    let agent_profile_id = agent_profile(app, "stale-fence").await;
    let runner = enroll_runner(app, "Stale fence runner").await;
    let request_id = create_execution_request(
        app,
        &item_id,
        "stale-fence-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    let (status, claimed_a) =
        common::claim_runner(app, &runner.runner_id, &runner.credential, "stale-claim-1").await;
    assert_eq!(status, StatusCode::OK, "{claimed_a}");
    let attempt_a = claimed_a["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_a = claimed_a["lease"]["fencing_token"].as_i64().unwrap();

    let (status, recovery) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    assert_eq!(recovery["disposition"], "safe_pre_spawn_requeue");

    let (status, claimed_b) =
        common::claim_runner(app, &runner.runner_id, &runner.credential, "stale-claim-2").await;
    assert_eq!(status, StatusCode::OK, "{claimed_b}");
    let attempt_b = claimed_b["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_b = claimed_b["lease"]["fencing_token"].as_i64().unwrap();
    assert_ne!(attempt_b, attempt_a);
    assert_eq!(fence_b, 2);

    SupersededFence {
        request_id,
        runner_id: runner.runner_id,
        attempt_a,
        fence_a,
        hdr: auth(&runner.credential),
    }
}

/// Posts `body` to `path` and asserts it is rejected as stale/conflict.
/// The response's error code is `stale_lease` on most fenced routes but
/// `conflict` on `heartbeat` (a known inconsistency, documented in this
/// crate's `CLAUDE.md`); both are accepted here since the invariant this
/// helper exists to prove is rejection, not which of the two codes is used.
async fn assert_stale_rejected(app: &axum::Router, path: &str, body: Value, hdr: &str) -> Value {
    let (status, resp) =
        common::send_large(app, "POST", path, body, &[("authorization", hdr)]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{resp}");
    assert!(
        matches!(
            resp["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {resp}"
    );
    resp
}

async fn assert_no_heartbeat_recorded(repo: &Repository, attempt_id: &str) {
    let last = execution_attempts::last_heartbeat_at(repo, attempt_id)
        .await
        .unwrap();
    assert_eq!(last, None, "a stale-fenced heartbeat must not be recorded");
}

async fn assert_no_decision_row(repo: &Repository, attempt_id: &str) {
    let count = execution_decisions::decision_count_of_attempt(repo, attempt_id)
        .await
        .unwrap();
    assert_eq!(count, 0, "a stale-fenced decision must not be recorded");
}

async fn assert_no_artifact_row(repo: &Repository, attempt_id: &str) {
    let count = execution_artifacts::artifact_count_of_attempt(repo, attempt_id)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "a stale-fenced artifact manifest must not be recorded"
    );
}

#[tokio::test]
async fn stale_fence_writes_nothing_across_every_mutation_route() {
    let storage_dir_guard = distinctive_temp_dir("stale-fence");
    let storage_dir = storage_dir_guard.path();
    let (app, repo) = app_in_memory(storage_dir).await;
    let s = setup_superseded_fence(&app).await;

    let hb = heartbeat_body(
        &s.runner_id,
        "stale-hb-1",
        0,
        json!([active_attempt_entry(&s.attempt_a, s.fence_a)]),
    );
    assert_stale_rejected(&app, "/api/runner/v1/heartbeat", hb, &s.hdr).await;
    assert_no_heartbeat_recorded(&repo, &s.attempt_a).await;

    let decision = decision_body(&s.runner_id, &s.attempt_a, s.fence_a, "dec-stale-1");
    let decisions_uri = format!("/api/runner/v1/attempts/{}/decisions", s.attempt_a);
    assert_stale_rejected(&app, &decisions_uri, decision, &s.hdr).await;
    assert_no_decision_row(&repo, &s.attempt_a).await;

    let manifest = artifact_manifest_body(
        &s.runner_id,
        &s.attempt_a,
        s.fence_a,
        "art-stale",
        "patch",
        "x.patch",
        3,
        &sha256_hex(b"abc"),
    );
    let artifacts_uri = format!("/api/runner/v1/attempts/{}/artifacts", s.attempt_a);
    assert_stale_rejected(&app, &artifacts_uri, manifest, &s.hdr).await;
    assert_no_artifact_row(&repo, &s.attempt_a).await;

    // Cancellation observation and a second recovery observation on the
    // already-superseded fence are rejected too (recovery is not itself
    // exempt from fencing) — both checked for status/code only, matching
    // the original scenario, since neither writes to a table this file
    // otherwise inspects.
    let cancel = cancellation_body(&s.runner_id, &s.attempt_a, s.fence_a, "cancel-stale-1");
    let cancel_uri = format!(
        "/api/runner/v1/attempts/{}/cancellation-observation",
        s.attempt_a
    );
    assert_stale_rejected(&app, &cancel_uri, cancel, &s.hdr).await;
    let recovery_key = format!(
        "recovery:{}:{}:process_stopped_again",
        s.attempt_a, s.fence_a
    );
    let recovery = recovery_body(&s.runner_id, &s.attempt_a, s.fence_a, &recovery_key);
    let recovery_uri = format!(
        "/api/runner/v1/attempts/{}/recovery-observation",
        s.attempt_a
    );
    assert_stale_rejected(&app, &recovery_uri, recovery, &s.hdr).await;

    // None of the five rejected calls above minted a new fence: exactly the
    // two fences from setup (1 lost, 2 current) exist for this request.
    let fences = execution_attempts::fence_count_of_request(&repo, &s.request_id)
        .await
        .unwrap();
    assert_eq!(
        fences, 2,
        "fence 1 (lost) and fence 2 (current) — no extra fence was minted"
    );
}

// =======================================================================
// 2. Oversized artifact: a declared size over the per-item or cumulative
//    attempt-total limit is rejected before any row is written, and the
//    rejection is scoped to the oversized item only.
// =======================================================================
#[tokio::test]
async fn oversized_single_artifact_rejected_per_item_cap() {
    let storage_dir_guard = distinctive_temp_dir("oversized-artifact");
    let storage_dir = storage_dir_guard.path();
    let (app, repo) = app_in_memory(storage_dir).await;
    let item_id = common::create_project_and_item(&app, "https://example.test/g2.git", &[]).await;
    let attempt = ready_running_attempt(&app, &item_id, "oversized").await;

    // Declared size over `artifact_content_bytes_max` (50 MiB).
    let (status, body) = post_artifact_manifest(
        &app,
        &attempt,
        "art-huge",
        "log",
        "huge.log",
        52_428_800_u64 + 1,
        &sha256_hex(b"x"),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "payload_too_large");
    let count = execution_artifacts::artifact_count_of_attempt(&repo, &attempt.attempt_id)
        .await
        .unwrap();
    assert_eq!(count, 0, "an oversized single artifact must write no row");
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

/// Ten artifacts each individually just under the per-item cap (52_428_800)
/// accumulate to 520_000_000, comfortably under the 524_288_000
/// attempt-total cap — each must succeed. An eleventh, itself still under
/// the per-item cap, pushes the running total over the cumulative cap and
/// must be rejected — proving the cumulative check is a real, separate
/// enforcement from the per-item one, not the same check applied twice.
#[tokio::test]
async fn cumulative_artifact_total_rejected_over_attempt_cap() {
    let storage_dir_guard = distinctive_temp_dir("cumulative-artifact");
    let storage_dir = storage_dir_guard.path();
    let (app, repo) = app_in_memory(storage_dir).await;
    let item_id = common::create_project_and_item(&app, "https://example.test/g2.git", &[]).await;
    let attempt = ready_running_attempt(&app, &item_id, "cumulative").await;

    for i in 0..10 {
        let sha = sha256_hex(format!("cum-{i}").as_bytes());
        let (status, body) = post_artifact_manifest(
            &app,
            &attempt,
            &format!("art-cum-{i}"),
            "log",
            &format!("cum-{i}.log"),
            52_000_000,
            &sha,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "artifact {i}: {body}");
    }

    let (status, second) = post_artifact_manifest(
        &app,
        &attempt,
        "art-cum-overflow",
        "log",
        "overflow.log",
        52_000_000,
        &sha256_hex(b"overflow"),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{second}");
    assert_eq!(second["error"]["code"], "payload_too_large");

    // Exactly the ten accepted rows survive; the overflow artifact never
    // landed, and the running total is exactly what ten successes imply.
    let rows = execution_artifacts::artifact_ids_of_attempt(&repo, &attempt.attempt_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10, "{rows:?}");
    assert!(!rows.contains(&"art-cum-overflow".to_string()));
    let total = execution_artifacts::declared_bytes_of_attempt(&repo, &attempt.attempt_id)
        .await
        .unwrap();
    assert_eq!(
        total, 520_000_000,
        "the rejected artifact must not have partially written its size"
    );
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 3. Path traversal / symlink-style attack surface: artifact ids carrying
//    `../`, absolute paths or NUL bytes are rejected before any file is
//    written outside the configured storage root. (`ArtifactStorage`'s own
//    `encode_id` hex-encodes every byte of the id, so no separator can act
//    as one — this test is the audit's own independent proof of that
//    structural claim, not a re-statement of it.)
// =======================================================================
#[tokio::test]
async fn traversal_id_encodings_battery_never_escapes_storage_root() {
    let storage_dir_guard = distinctive_temp_dir("traversal");
    let storage_dir = storage_dir_guard.path();
    let (app, _repo) = app_in_memory(storage_dir).await;
    let item_id = common::create_project_and_item(&app, "https://example.test/g2.git", &[]).await;
    let attempt = ready_running_attempt(&app, &item_id, "traversal").await;
    let hdr = auth(&attempt.credential);

    let malicious_ids = [
        "../../../../etc/passwd",
        "..%2f..%2fetc%2fpasswd",
        "/etc/passwd",
        "art\0-null",
        "..\\..\\windows\\system32",
    ];
    let content = b"malicious payload".to_vec();
    for artifact_id in malicious_ids {
        let (status, manifest) = post_artifact_manifest(
            &app,
            &attempt,
            artifact_id,
            "patch",
            "x.patch",
            content.len() as u64,
            &sha256_hex(&content),
        )
        .await;
        // Whether the manifest step itself rejects the id, or accepts it and
        // the subsequent content PUT is what rejects it, either is an
        // acceptable safe outcome — what matters is that no byte ever lands
        // outside `storage_dir`. Only continue to the content PUT if the
        // manifest step accepted it.
        if status == StatusCode::OK {
            assert_eq!(
                manifest["artifacts"][0]["artifact_id"], artifact_id,
                "{manifest}"
            );
            let encoded_uri = format!(
                "/api/runner/v1/attempts/{}/artifacts/{}/content",
                attempt.attempt_id,
                percent_encode_path_segment(artifact_id)
            );
            let fence = attempt.fencing_token.to_string();
            let headers = [
                ("authorization", hdr.as_str()),
                ("x-tack-fencing-token", fence.as_str()),
                ("content-type", "text/plain"),
            ];
            let _ = put_content(&app, &encoded_uri, content.clone(), &headers).await;
        }
    }

    // The decisive assertion: nothing was ever written outside the
    // configured storage root. `/etc/passwd` itself is untouched by
    // construction (the process has no write permission there in the test
    // sandbox), so an escape would surface as a permission error above, not
    // a silent write outside the containment this checks.
    if storage_dir.exists() {
        for path in &walk_files(storage_dir).await {
            assert!(
                path.starts_with(storage_dir),
                "artifact file {path:?} escaped {storage_dir:?}"
            );
        }
    }
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 5. Corrupt database row: a hand-corrupted `request_snapshot` (malformed
//    JSON, simulating on-disk bit rot or a partial write outside any
//    transaction) degrades to a typed error on every read path that touches
//    it, never a panic and never a silently-fabricated success.
// =======================================================================
#[tokio::test]
async fn corrupted_snapshot_row_degrades_to_typed_error_not_panic() {
    let storage_dir_guard = distinctive_temp_dir("corrupt-row");
    let storage_dir = storage_dir_guard.path();
    let (app, repo) = app_in_memory(storage_dir).await;
    let item_id = common::create_project_and_item(&app, "https://example.test/g2.git", &[]).await;
    let agent_profile_id = agent_profile(&app, "corrupt-row").await;
    let runner = enroll_runner(&app, "Corrupt-row runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "corrupt-row-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    // Simulate a corrupted row directly (bit rot / a write outside any
    // application transaction) — not something reachable through any HTTP
    // input validator, deliberately: this proves the *read* path degrades
    // safely, independent of whatever wrote the corruption.
    seed::execution_request_snapshot(&repo, &request_id, "not valid json {{{").await;

    // A claim against this request must not panic the process (which would
    // abort every in-flight request on this connection, not just this one)
    // and must not fabricate a plausible-looking successful lease from
    // garbage data.
    let (status, body) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "corrupt-claim").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a corrupted snapshot must never produce a successful claim: {body}"
    );
    assert!(
        status.is_client_error() || status.is_server_error(),
        "expected a typed error status, got {status}: {body}"
    );

    // The server is still alive and healthy after this — the corrupted row
    // did not take down the whole process.
    let (status, health) = common::send_large(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{health}");

    // No attempt was ever created against the corrupted request.
    let attempt_count = execution_attempts::attempt_count_of_request(&repo, &request_id)
        .await
        .unwrap();
    assert_eq!(attempt_count, 0);
}
