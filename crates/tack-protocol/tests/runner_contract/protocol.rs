use crate::runner_contract::fixtures::{fixture_name, fixture_paths, load_value};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;

#[derive(Debug, Deserialize)]
struct ProtocolContract {
    protocol_version: u16,
    minimum_supported_version: u16,
    maximum_supported_version: u16,
    stable_error_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    request_id: String,
    retryable: bool,
    details: serde_json::Value,
}

#[test]
fn protocol_v1_and_error_fixture_set_are_exact() {
    let protocol: ProtocolContract = serde_json::from_value(load_value("protocol.json"))
        .expect("protocol fixture must match its contract shape");
    assert_eq!(protocol.protocol_version, 1);
    assert_eq!(protocol.minimum_supported_version, 1);
    assert_eq!(protocol.maximum_supported_version, 1);

    let expected = protocol
        .stable_error_codes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(expected.len(), protocol.stable_error_codes.len());

    let mut actual = BTreeSet::new();
    for path in fixture_paths()
        .into_iter()
        .filter(|path| fixture_name(path).starts_with("errors/"))
    {
        let name = fixture_name(&path);
        let bytes = fs::read(&path).expect("error fixture must be readable");
        let envelope: ErrorEnvelope = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{name} has the wrong envelope: {error}"));
        assert!(
            !envelope.error.message.is_empty(),
            "{name} has an empty message"
        );
        assert!(
            envelope.error.request_id.starts_with("req_"),
            "{name} has a non-example request id"
        );
        assert!(
            envelope.error.details.is_object(),
            "{name} details must be an object"
        );
        let _retryable = envelope.error.retryable;
        actual.insert(envelope.error.code);
    }

    assert_eq!(actual, expected);
}

#[test]
fn stable_error_code_mutation_is_detected() {
    let mut protocol: ProtocolContract = serde_json::from_value(load_value("protocol.json"))
        .expect("protocol fixture must match its contract shape");
    let removed = protocol
        .stable_error_codes
        .pop()
        .expect("protocol has stable errors");
    assert_eq!(removed, "internal_error");
    assert!(!protocol.stable_error_codes.contains(&removed));
}

/// Each claim, heartbeat, event-batch and completion fixture parses into the protocol's own
/// struct and serializes back to the same JSON value, so the structs and the contract cannot
/// drift apart.
#[test]
fn claim_heartbeat_event_batch_and_completion_fixtures_round_trip_through_the_structs() {
    use tack_protocol::execution::{
        ClaimRequest, ClaimResponse, CompletionRequest, CompletionResponse, EventBatchRequest,
        EventBatchResponse, HeartbeatRequest, HeartbeatResponse,
    };

    fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned>(name: &str) -> T {
        let fixture = load_value(name);
        let typed: T = serde_json::from_value(fixture.clone())
            .unwrap_or_else(|error| panic!("{name} does not fit its struct: {error}"));
        assert_eq!(
            serde_json::to_value(&typed).expect("struct serializes"),
            fixture,
            "{name} changed when it went through its struct"
        );
        typed
    }

    round_trips::<ClaimRequest>("claim.request.json");
    assert!(matches!(
        round_trips::<ClaimResponse>("claim.response.json"),
        ClaimResponse::Work(_)
    ));
    assert!(matches!(
        round_trips::<ClaimResponse>("claim.no-work.response.json"),
        ClaimResponse::Idle(_)
    ));
    round_trips::<HeartbeatRequest>("heartbeat.request.json");
    round_trips::<HeartbeatRequest>("heartbeat.request.two-active.json");
    round_trips::<HeartbeatResponse>("heartbeat.response.json");
    round_trips::<EventBatchRequest>("event-batch.request.json");
    round_trips::<EventBatchResponse>("event-batch.response.json");
    round_trips::<CompletionRequest>("completion.request.json");
    round_trips::<CompletionResponse>("completion.response.json");
}

/// The enrollment, refresh, accept and start, decision, artifact manifest and cancellation
/// fixtures parse into the protocol's own structs and serialize back to the same JSON value.
#[test]
fn enrollment_refresh_attempt_decision_artifact_and_cancellation_fixtures_round_trip_through_the_structs()
 {
    use tack_protocol::execution::{
        ArtifactManifestRequest, ArtifactManifestResponse, AttemptTransitionRequest,
        AttemptTransitionResponse, CancellationObservationRequest,
        CancellationObservationResponse, DecisionCreateRequest, DecisionCreateResponse,
        DecisionPollRequest, DecisionPollResponse, EnrollmentRequest, EnrollmentResponse,
        RefreshRequest, RefreshResponse,
    };

    fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned>(name: &str) {
        let fixture = load_value(name);
        let typed: T = serde_json::from_value(fixture.clone())
            .unwrap_or_else(|error| panic!("{name} does not fit its struct: {error}"));
        assert_eq!(
            serde_json::to_value(&typed).expect("struct serializes"),
            fixture,
            "{name} changed when it went through its struct"
        );
    }

    round_trips::<EnrollmentRequest>("enrollment.request.json");
    round_trips::<EnrollmentResponse>("enrollment.response.json");
    round_trips::<RefreshRequest>("refresh.request.json");
    round_trips::<RefreshResponse>("refresh.response.json");
    round_trips::<AttemptTransitionRequest>("accept.request.json");
    round_trips::<AttemptTransitionResponse>("accept.response.json");
    round_trips::<AttemptTransitionRequest>("start.request.json");
    round_trips::<AttemptTransitionResponse>("start.response.json");
    round_trips::<DecisionCreateRequest>("decision.create.request.json");
    round_trips::<DecisionCreateResponse>("decision.create.response.json");
    round_trips::<DecisionPollRequest>("decision.poll.request.json");
    round_trips::<DecisionPollResponse>("decision.poll.response.json");
    round_trips::<ArtifactManifestRequest>("artifact.request.json");
    round_trips::<ArtifactManifestResponse>("artifact.response.json");
    round_trips::<CancellationObservationRequest>("cancellation.request.json");
    round_trips::<CancellationObservationResponse>("cancellation.response.json");
}
