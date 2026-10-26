use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct VersionedFixture {
    protocol_version: ProtocolVersion,
    #[serde(flatten)]
    fields: BTreeMap<String, serde_json::Value>,
}

fn assert_versioned_fixture_round_trip(raw: &str) {
    let original: serde_json::Value = serde_json::from_str(raw).expect("fixture JSON");
    let typed: VersionedFixture = serde_json::from_str(raw).expect("protocol v1 fixture");
    assert_eq!(
        serde_json::to_value(typed).expect("serialize fixture"),
        original,
        "fixture must round-trip without dropping additive fields"
    );
}

/// Parses `value` as `T` and asserts re-serializing it reproduces `value`
/// exactly — the shared shape behind every "fixture round-trips" test in
/// this file that isn't checking anything fixture-specific beyond that.
fn assert_round_trips<T: serde::de::DeserializeOwned + Serialize>(value: serde_json::Value) {
    let typed: T = serde_json::from_value(value.clone()).expect("typed fixture");
    assert_eq!(
        serde_json::to_value(typed).expect("serialize fixture"),
        value
    );
}

#[test]
fn protocol_rejects_non_v1() {
    assert!(serde_json::from_str::<ProtocolVersion>("2").is_err());
}

#[test]
fn stale_lease_code_is_stable() {
    let error = ExecutionError::StaleLease {
        attempt_id: AttemptId::new("att_future"),
        current_fencing_token: FencingToken(8),
    };
    assert_eq!(error.code(), "stale_lease");
}

#[test]
fn every_versioned_fixture_round_trips_intact() {
    for fixture in [
        include_str!("../../../../../docs/contracts/runner-v1/artifact.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/artifact.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/cancellation.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/cancellation.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/capabilities.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.no-work.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/completion.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/completion.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.create.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.create.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.poll.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.poll.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/enrollment.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/enrollment.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/event-batch.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/event-batch.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/heartbeat.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/heartbeat.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/limits.json"),
        include_str!("../../../../../docs/contracts/runner-v1/protocol.json"),
        include_str!("../../../../../docs/contracts/runner-v1/recovery-observation.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/recovery-observation.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/refresh.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/refresh.response.json"),
    ] {
        assert_versioned_fixture_round_trip(fixture);
    }
}

/// Reuses [`ERROR_FIXTURES`] rather than a second copy of the same file
/// list — [`stable_error_code_retryable_matches_every_fixture_and_constructor`]
/// below checks `retryable`/constructor conformance against the same list;
/// this one just checks every error fixture parses as `ProtocolErrorEnvelope`
/// and serializes back byte-for-byte.
#[test]
fn every_error_fixture_round_trips_as_a_typed_error() {
    for (name, fixture) in ERROR_FIXTURES {
        let original: serde_json::Value =
            serde_json::from_str(fixture).unwrap_or_else(|e| panic!("{name}: {e}"));
        let typed: ProtocolErrorEnvelope =
            serde_json::from_str(fixture).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            serde_json::to_value(typed).expect("serialize error"),
            original,
            "{name}"
        );
    }
}

/// Every file under `docs/contracts/runner-v1/errors/`, paired with its
/// bare filename. This is the single list the two tests below share: the
/// retryable/constructor conformance test walks the fixture bytes, and
/// the coverage test below checks this list against the real directory
/// so a fixture added on disk without a matching entry here is a build
/// failure, not a silent gap.
const ERROR_FIXTURES: &[(&str, &str)] = &[
    (
        "artifact-checksum-mismatch.json",
        include_str!(
            "../../../../../docs/contracts/runner-v1/errors/artifact-checksum-mismatch.json"
        ),
    ),
    (
        "conflict.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/conflict.json"),
    ),
    (
        "decision-expired.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/decision-expired.json"),
    ),
    (
        "forbidden.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/forbidden.json"),
    ),
    (
        "idempotency-conflict.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/idempotency-conflict.json"),
    ),
    (
        "internal-error.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/internal-error.json"),
    ),
    (
        "invalid-request.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-request.json"),
    ),
    (
        "invalid-transition.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-transition.json"),
    ),
    (
        "not-found.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/not-found.json"),
    ),
    (
        "payload-too-large.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/payload-too-large.json"),
    ),
    (
        "rate-limited.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/rate-limited.json"),
    ),
    (
        "runner-revoked.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/runner-revoked.json"),
    ),
    (
        "stale-lease.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/stale-lease.json"),
    ),
    (
        "unauthorized.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/unauthorized.json"),
    ),
    (
        "unsupported-protocol.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/unsupported-protocol.json"),
    ),
];

#[test]
fn every_error_fixture_file_on_disk_is_in_the_conformance_list() {
    // Reads the real directory at test time (not compile time) so a
    // fixture file added to disk without a matching `ERROR_FIXTURES`
    // entry fails this test, rather than silently skipping the new code.
    let dir = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/contracts/runner-v1/errors"
    ));
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .expect("docs/contracts/runner-v1/errors must exist")
        .map(|entry| {
            entry
                .expect("readable directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();

    let mut listed: Vec<&str> = ERROR_FIXTURES.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();

    assert_eq!(
        on_disk, listed,
        "docs/contracts/runner-v1/errors/ changed; add or remove the matching \
         entry in ERROR_FIXTURES (crates/tack-orch/src/execution/types.rs)"
    );
}

#[test]
fn stable_error_code_retryable_matches_every_fixture() {
    for (name, fixture) in ERROR_FIXTURES {
        let envelope: ProtocolErrorEnvelope =
            serde_json::from_str(fixture).unwrap_or_else(|e| panic!("{name}: {e}"));
        let parsed = envelope.error;

        assert_eq!(
            parsed.code.retryable(),
            parsed.retryable,
            "{name}: StableErrorCode::retryable() for {:?} must match the \
             fixture's `retryable` value",
            parsed.code
        );

        let constructed = ProtocolError::new(
            parsed.code,
            parsed.message.clone(),
            parsed.request_id.clone(),
            parsed.details.clone(),
        );
        assert_eq!(
            constructed, parsed,
            "{name}: ProtocolError::new must reproduce the fixture exactly, \
             including `retryable`"
        );
    }
}

#[test]
fn core_domain_snapshots_match_their_exact_fixture_shapes() {
    let capabilities_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.json"
    ))
    .expect("capability fixture JSON");
    assert_round_trips::<crate::execution::RunnerCapabilities>(capabilities_json);

    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture JSON");
    assert_round_trips::<ExecutionRequestSnapshot>(claim["request"].clone());
    assert_round_trips::<AttemptSnapshot>(claim["attempt"].clone());

    let completion: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/completion.request.json"
    ))
    .expect("completion fixture JSON");
    assert_round_trips::<ActualExecution>(completion["actual_execution"].clone());
    assert_round_trips::<Usage>(completion["usage"].clone());
}

#[test]
fn recovery_observation_request_and_response_round_trip_exactly() {
    let request_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.request.json"
    ))
    .expect("recovery request fixture JSON");
    let request: RecoveryObservationRequest =
        serde_json::from_value(request_json.clone()).expect("typed recovery request");
    assert_eq!(
        serde_json::to_value(&request).expect("serialize recovery request"),
        request_json
    );
    assert_eq!(request.observation, RecoveryObservation::ProcessStopped);
    assert_eq!(
        request.details.journal_state,
        RecoveryJournalState::Prepared
    );
    assert!(!request.details.process_observed);

    let response_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.response.json"
    ))
    .expect("recovery response fixture JSON");
    let response: RecoveryObservationResponse =
        serde_json::from_value(response_json.clone()).expect("typed recovery response");
    assert_eq!(
        serde_json::to_value(&response).expect("serialize recovery response"),
        response_json
    );
    assert_eq!(
        response.disposition,
        RecoveryDisposition::SafePreSpawnRequeue
    );
    assert!(!response.replayed);
}

#[test]
fn recovery_observation_additive_fields_survive_a_round_trip() {
    let request_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.request.json"
    ))
    .expect("recovery request fixture JSON");
    let mut additive_request = request_json.clone();
    additive_request["future_request_field"] = serde_json::json!({"kept": true});
    additive_request["details"]["future_evidence"] = serde_json::json!("kept");
    let parsed: RecoveryObservationRequest =
        serde_json::from_value(additive_request.clone()).expect("parse additive request");
    assert_eq!(
        serde_json::to_value(parsed).expect("serialize additive request"),
        additive_request
    );

    let response_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.response.json"
    ))
    .expect("recovery response fixture JSON");
    let mut additive_response = response_json.clone();
    additive_response["future_response_field"] = serde_json::json!(42);
    let parsed: RecoveryObservationResponse =
        serde_json::from_value(additive_response.clone()).expect("parse additive response");
    assert_eq!(
        serde_json::to_value(parsed).expect("serialize additive response"),
        additive_response
    );
}

const ACTIVE_EXECUTION_STATES: [ExecutionState; 4] = [
    ExecutionState::Leased,
    ExecutionState::Preparing,
    ExecutionState::Running,
    ExecutionState::WaitingDecision,
];

/// Every observation in `observations` is compatible with `disposition` at
/// `state`, and `disposition`'s own attempt transition (out of `state`) is
/// itself a legal lifecycle transition for the recovery service.
fn assert_recovery_disposition_valid_for(
    disposition: RecoveryDisposition,
    state: ExecutionState,
    observations: &[RecoveryObservation],
) {
    use crate::execution::{TransitionActor, validate_transition};
    for &observation in observations {
        assert!(disposition.is_compatible_with(state, observation));
    }
    assert!(
        validate_transition(
            state,
            disposition
                .attempt_transition()
                .expect("recovery disposition transitions attempt"),
            TransitionActor::RecoveryService,
        )
        .is_ok()
    );
}

#[test]
fn recovery_dispositions_are_compatible_with_every_active_state() {
    for state in ACTIVE_EXECUTION_STATES {
        assert_recovery_disposition_valid_for(
            RecoveryDisposition::SafePreSpawnRequeue,
            state,
            &[RecoveryObservation::ProcessStopped],
        );
        assert_recovery_disposition_valid_for(
            RecoveryDisposition::NeedsOperator,
            state,
            &[
                RecoveryObservation::ProcessStopped,
                RecoveryObservation::ProcessRunning,
                RecoveryObservation::Ambiguous,
            ],
        );
    }
}

#[test]
fn recovery_dispositions_reject_mismatched_state_pairs() {
    assert!(
        !RecoveryDisposition::SafePreSpawnRequeue
            .is_compatible_with(ExecutionState::Running, RecoveryObservation::ProcessRunning)
    );
    assert!(
        !RecoveryDisposition::SafePreSpawnRequeue.is_compatible_with(
            ExecutionState::Succeeded,
            RecoveryObservation::ProcessStopped
        )
    );
    for state in [
        ExecutionState::Succeeded,
        ExecutionState::Failed,
        ExecutionState::Cancelled,
    ] {
        assert!(
            RecoveryDisposition::AlreadyTerminal
                .is_compatible_with(state, RecoveryObservation::Ambiguous)
        );
    }
    assert!(
        !RecoveryDisposition::AlreadyTerminal
            .is_compatible_with(ExecutionState::Leased, RecoveryObservation::ProcessStopped)
    );
}

#[test]
fn recovery_disposition_transition_targets_are_fixed() {
    assert_eq!(
        RecoveryDisposition::SafePreSpawnRequeue.request_transition(),
        Some(ExecutionState::Queued)
    );
    assert_eq!(
        RecoveryDisposition::NeedsOperator.request_transition(),
        Some(ExecutionState::NeedsOperator)
    );
    assert_eq!(
        RecoveryDisposition::AlreadyTerminal.attempt_transition(),
        None
    );
    assert_eq!(
        RecoveryDisposition::AlreadyTerminal.request_transition(),
        None
    );
}

#[test]
fn a_counted_token_is_a_new_one() {
    let usage = |reported: serde_json::Value| -> Usage {
        let mut value = serde_json::json!({
            "duration_ms": {"value": null, "source": "not_measured"},
            "cost_usd": {"value": null, "source": "not_measured"},
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(reported.as_object().unwrap().clone());
        serde_json::from_value(value).expect("usage")
    };
    let measured = |n: u64| serde_json::json!({"value": n, "source": "measured"});
    let unmeasured = serde_json::json!({"value": null, "source": "not_measured"});
    // 100 000 in with 90 000 read from the cache, 2 000 out.
    let cached = usage(serde_json::json!({
        "tokens_in": measured(100_000), "tokens_out": measured(2_000), "cache_read_tokens": 90_000,
    }));
    assert_eq!(cached.counted(), Some(12_000));
    // No cache figure counts whole; nothing measured is not zero.
    let whole = usage(serde_json::json!({"tokens_in": measured(500), "tokens_out": measured(100)}));
    assert_eq!(whole.counted(), Some(600));
    let none = usage(serde_json::json!({"tokens_in": unmeasured, "tokens_out": unmeasured}));
    assert_eq!(none.counted(), None);
}

/// The claim fixture's request, as `harness` with `tools` for the run and the
/// profile's `tool_policy` and `kind`.
fn request_for(
    harness: &str,
    tools: &[&str],
    tool_policy: serde_json::Value,
    kind: Option<&str>,
) -> ExecutionRequestSnapshot {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = HarnessKind::new(harness);
    request.permission_policy.tools = tools.iter().map(|tool| (*tool).to_owned()).collect();
    request.resolved_agent_profile.tool_policy = tool_policy;
    if let Some(kind) = kind {
        request
            .resolved_agent_profile
            .additional
            .insert("kind".to_owned(), kind.into());
    }
    request
}

#[test]
fn a_run_expects_a_change_when_its_explicit_tools_write_else_when_its_kind_is_not_read_only() {
    let none = serde_json::json!({});
    for (harness, tools, policy, kind, expected) in [
        (
            "claude-code",
            vec!["Read", "Edit"],
            none.clone(),
            None,
            true,
        ),
        ("claude-code", vec!["bash"], none.clone(), None, true),
        (
            "claude-code",
            vec!["Read", "Grep", "Glob"],
            none.clone(),
            None,
            false,
        ),
        // claude-code reads the profile's list when the run names none.
        (
            "claude-code",
            vec![],
            serde_json::json!({"tools": {"claude-code": ["Read", "Grep"]}}),
            None,
            false,
        ),
        (
            "claude-code",
            vec![],
            serde_json::json!({"tools": {"claude-code": ["Read", "Write"]}}),
            None,
            true,
        ),
        ("codex", vec![], none.clone(), None, false),
        ("codex", vec!["*"], none.clone(), None, true),
        ("opencode", vec!["webfetch"], none.clone(), None, false),
        ("opencode", vec!["edit"], none.clone(), None, true),
        // Fabulada's researcher writes a document: its explicit list names `write`.
        (
            "docket",
            vec![],
            serde_json::json!({"tools": {"docket": ["read", "fetch", "write"]}}),
            Some("researcher"),
            true,
        ),
        (
            "docket",
            vec!["read", "fetch", "write"],
            none.clone(),
            Some("researcher"),
            true,
        ),
        (
            "docket",
            vec![],
            serde_json::json!({"tools": {"docket": ["read", "fetch"]}}),
            Some("researcher"),
            false,
        ),
        (
            "docket",
            vec![],
            serde_json::json!({"tools": {"docket": ["*"]}}),
            Some("researcher"),
            false,
        ),
        ("docket", vec!["*"], none.clone(), Some("researcher"), false),
        // With `*` or no list, a reviewer, researcher or planner does not change project files.
        ("codex", vec!["*"], none.clone(), Some("reviewer"), false),
        ("docket", vec!["*"], none.clone(), Some("planner"), false),
        (
            "claude-code",
            vec!["Edit"],
            none.clone(),
            Some("researcher"),
            true,
        ),
        ("codex", vec!["*"], none.clone(), Some("implementer"), true),
        ("codex", vec!["*"], none.clone(), Some("custom"), true),
        // docket applies its team's own tools: an unnamed list may write.
        ("docket", vec![], none.clone(), None, true),
        ("docket", vec!["*"], none.clone(), None, true),
        (
            "docket",
            vec!["*"],
            serde_json::json!({"docket": {"denied_tools": ["fetch"]}}),
            None,
            true,
        ),
        (
            "docket",
            vec!["*"],
            serde_json::json!({"docket": {"denied_tools": ["Write", "edit", "bash"]}}),
            None,
            false,
        ),
        (
            "docket",
            vec![],
            serde_json::json!({"tools": {"docket": ["write"]}, "docket": {"denied_tools": ["write"]}}),
            None,
            false,
        ),
    ] {
        let request = request_for(harness, &tools, policy.clone(), kind);
        assert_eq!(
            request.expects_change(),
            expected,
            "{harness} {tools:?} {policy} {kind:?}"
        );
    }
}

#[test]
fn execution_state_and_recovery_disposition_names_round_trip() {
    use std::str::FromStr;

    for state in [
        ExecutionState::Queued,
        ExecutionState::Leased,
        ExecutionState::Preparing,
        ExecutionState::Running,
        ExecutionState::WaitingDecision,
        ExecutionState::Succeeded,
        ExecutionState::Failed,
        ExecutionState::Cancelled,
        ExecutionState::Lost,
        ExecutionState::NeedsOperator,
    ] {
        assert_eq!(ExecutionState::from_str(state.as_str()), Ok(state));
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::Value::String(state.as_str().to_owned()),
            "as_str must be the serde name"
        );
    }
    assert!(ExecutionState::from_str("done").is_err());

    for disposition in [
        RecoveryDisposition::SafePreSpawnRequeue,
        RecoveryDisposition::NeedsOperator,
        RecoveryDisposition::AlreadyTerminal,
    ] {
        assert_eq!(
            RecoveryDisposition::from_str(disposition.as_str()),
            Ok(disposition)
        );
        assert_eq!(
            serde_json::to_value(disposition).unwrap(),
            serde_json::Value::String(disposition.as_str().to_owned()),
        );
    }
    assert!(RecoveryDisposition::from_str("requeue").is_err());
}

#[test]
fn the_autonomy_a_request_carries_reads_back_and_a_stranger_reads_as_absent() {
    let mut request = request_for("docket", &[], serde_json::json!({}), None);
    assert_eq!(request.autonomy(), None, "the claim fixture names none");
    for level in [AutonomyLevel::L3, AutonomyLevel::L4, AutonomyLevel::L5] {
        request
            .additional
            .insert("autonomy".to_owned(), level.as_str().into());
        assert_eq!(request.autonomy(), Some(level));
        let wire = serde_json::to_value(&request).expect("request");
        assert_eq!(wire["autonomy"], level.as_str());
        let again: ExecutionRequestSnapshot = serde_json::from_value(wire).expect("request");
        assert_eq!(again.autonomy(), Some(level), "it survives the wire");
    }
    request
        .additional
        .insert("autonomy".to_owned(), "L9".into());
    assert_eq!(request.autonomy(), None);
}
