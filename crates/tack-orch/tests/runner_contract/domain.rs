use crate::fixtures::{fixture_name, fixture_paths, load_value};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::fs;
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, AttemptSnapshot, ExecutionRequestSnapshot,
    ExecutionState, ProtocolErrorEnvelope, RecoveryDisposition, RecoveryObservation,
    RecoveryObservationRequest, RecoveryObservationResponse, RequestedModelId,
    RequestedModelProvider, RunnerCapabilities, TransitionActor, Usage, validate_transition,
};

fn assert_exact_round_trip<T>(name: &str, value: &Value)
where
    T: DeserializeOwned + Serialize + Debug,
{
    let typed: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("{name} did not deserialize into the domain: {error}"));
    let encoded = serde_json::to_value(typed)
        .unwrap_or_else(|error| panic!("{name} did not serialize from the domain: {error}"));
    assert_eq!(encoded, *value, "{name} changed during domain round-trip");
}

#[test]
fn frozen_domain_fragments_round_trip_exactly() {
    let capabilities = load_value("capabilities.json");
    assert_exact_round_trip::<RunnerCapabilities>("capabilities.json", &capabilities);

    let claim = load_value("claim.response.json");
    assert_exact_round_trip::<ExecutionRequestSnapshot>(
        "claim.response.json.request",
        &claim["request"],
    );
    assert_exact_round_trip::<AttemptSnapshot>("claim.response.json.attempt", &claim["attempt"]);

    let completion = load_value("completion.request.json");
    assert_exact_round_trip::<ActualExecution>(
        "completion.request.json.actual_execution",
        &completion["actual_execution"],
    );
    assert_exact_round_trip::<Usage>("completion.request.json.usage", &completion["usage"]);

    let recovery_request = load_value("recovery-observation.request.json");
    assert_exact_round_trip::<RecoveryObservationRequest>(
        "recovery-observation.request.json",
        &recovery_request,
    );
    let recovery_response = load_value("recovery-observation.response.json");
    assert_exact_round_trip::<RecoveryObservationResponse>(
        "recovery-observation.response.json",
        &recovery_response,
    );

    for path in fixture_paths()
        .into_iter()
        .filter(|path| fixture_name(path).starts_with("errors/"))
    {
        let name = fixture_name(&path);
        let bytes = fs::read(path).expect("error fixture must remain readable");
        let value: Value = serde_json::from_slice(&bytes).expect("error fixture must be JSON");
        assert_exact_round_trip::<ProtocolErrorEnvelope>(&name, &value);
    }
}

#[test]
fn recovery_observation_requires_every_field_and_rejects_unknown_enums() {
    let request = load_value("recovery-observation.request.json");
    for field in [
        "protocol_version",
        "runner_id",
        "attempt_id",
        "fencing_token",
        "recovery_key",
        "observation",
        "details",
    ] {
        let mut mutated = request.clone();
        mutated
            .as_object_mut()
            .expect("recovery request fixture must be an object")
            .remove(field);
        assert!(
            serde_json::from_value::<RecoveryObservationRequest>(mutated).is_err(),
            "missing recovery request field {field} must be rejected"
        );
    }
    for (path, invalid) in [
        (vec!["observation"], "unknown_observation"),
        (vec!["details", "journal_state"], "secret_material"),
    ] {
        let mut mutated = request.clone();
        let mut value = &mut mutated;
        for key in path {
            value = value
                .get_mut(key)
                .expect("recovery request fixture path must exist");
        }
        *value = Value::String(invalid.to_owned());
        assert!(
            serde_json::from_value::<RecoveryObservationRequest>(mutated).is_err(),
            "invalid recovery request enum {invalid} must be rejected"
        );
    }

    let response = load_value("recovery-observation.response.json");
    for field in [
        "protocol_version",
        "attempt_id",
        "recovery_key",
        "disposition",
        "replayed",
        "committed_at",
    ] {
        let mut mutated = response.clone();
        mutated
            .as_object_mut()
            .expect("recovery response fixture must be an object")
            .remove(field);
        assert!(
            serde_json::from_value::<RecoveryObservationResponse>(mutated).is_err(),
            "missing recovery response field {field} must be rejected"
        );
    }
    let mut invalid_disposition = response;
    invalid_disposition["disposition"] = Value::String("retry_forever".to_owned());
    assert!(
        serde_json::from_value::<RecoveryObservationResponse>(invalid_disposition).is_err(),
        "unknown recovery disposition must be rejected"
    );
}

#[test]
fn recovery_dispositions_match_the_frozen_lifecycle_contract() {
    let lifecycle = load_value("lifecycle-transitions.json");
    let recovery = &lifecycle["recovery_observation"];
    assert_eq!(recovery["actor"], "recovery_service");
    assert_eq!(
        recovery["observations"],
        serde_json::json!(["process_stopped", "process_running", "ambiguous"])
    );

    let active = [
        ExecutionState::Leased,
        ExecutionState::Preparing,
        ExecutionState::Running,
        ExecutionState::WaitingDecision,
    ];
    for state in active {
        assert!(
            RecoveryDisposition::SafePreSpawnRequeue
                .is_compatible_with(state, RecoveryObservation::ProcessStopped)
        );
        assert!(
            validate_transition(
                state,
                RecoveryDisposition::SafePreSpawnRequeue
                    .attempt_transition()
                    .expect("safe recovery has an attempt transition"),
                TransitionActor::RecoveryService,
            )
            .is_ok()
        );
        assert!(
            RecoveryDisposition::NeedsOperator
                .is_compatible_with(state, RecoveryObservation::ProcessRunning)
        );
        assert!(
            validate_transition(
                state,
                RecoveryDisposition::NeedsOperator
                    .attempt_transition()
                    .expect("needs-operator recovery has an attempt transition"),
                TransitionActor::RecoveryService,
            )
            .is_ok()
        );
    }
    assert!(
        !RecoveryDisposition::SafePreSpawnRequeue
            .is_compatible_with(ExecutionState::Running, RecoveryObservation::Ambiguous)
    );
    assert!(
        RecoveryDisposition::AlreadyTerminal
            .is_compatible_with(ExecutionState::Succeeded, RecoveryObservation::Ambiguous)
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
fn opaque_model_ids_survive_punctuation_byte_for_byte() {
    let capabilities: RunnerCapabilities = serde_json::from_value(load_value("capabilities.json"))
        .expect("capability fixture must match the domain");
    let ids = &capabilities.harnesses[0].model_combinations[0].model_ids;
    assert_eq!(ids[0].as_str(), "opaque/model-alpha");
    assert_eq!(ids[1].as_str(), "opaque:model-beta");
}

#[test]
fn requested_and_actual_model_values_are_distinct_types() {
    assert_ne!(
        TypeId::of::<RequestedModelId>(),
        TypeId::of::<ActualModelId>()
    );
    assert_ne!(
        TypeId::of::<RequestedModelProvider>(),
        TypeId::of::<ActualModelProvider>()
    );
}

fn state(name: &str) -> ExecutionState {
    serde_json::from_value(Value::String(name.to_owned()))
        .unwrap_or_else(|error| panic!("unknown frozen lifecycle state {name}: {error}"))
}

fn actor(name: &str) -> TransitionActor {
    match name {
        "scheduler" => TransitionActor::Scheduler,
        "operator" => TransitionActor::Operator,
        "lease_owner" => TransitionActor::LeaseOwner,
        "recovery_service" => TransitionActor::RecoveryService,
        other => panic!("unknown frozen transition actor {other}"),
    }
}

#[test]
fn production_lifecycle_matches_every_state_pair_and_actor() {
    let fixture = load_value("lifecycle-transitions.json");
    let states = fixture["states"]
        .as_array()
        .expect("lifecycle states must be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("lifecycle state must be text")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let actors = ["scheduler", "operator", "lease_owner", "recovery_service"];

    let rules = fixture["rules"]
        .as_array()
        .expect("lifecycle rules must be an array")
        .iter()
        .map(|rule| {
            let from = rule["from"]
                .as_str()
                .expect("lifecycle source must be text")
                .to_owned();
            let allowed = rule["allow"]
                .as_object()
                .expect("allow must be an object")
                .iter()
                .map(|(target, actor_values)| {
                    let allowed_actors = actor_values
                        .as_array()
                        .expect("allowed actors must be an array")
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .expect("allowed actor must be text")
                                .to_owned()
                        })
                        .collect::<BTreeSet<_>>();
                    (target.clone(), allowed_actors)
                })
                .collect::<BTreeMap<_, _>>();
            (from, allowed)
        })
        .collect::<BTreeMap<_, _>>();

    for from in &states {
        for to in &states {
            for actor_name in actors {
                let expected = rules
                    .get(from)
                    .and_then(|targets| targets.get(to))
                    .is_some_and(|allowed| allowed.contains(actor_name));
                let actual = validate_transition(state(from), state(to), actor(actor_name)).is_ok();
                assert_eq!(
                    actual, expected,
                    "production lifecycle disagrees for {from} -> {to} as {actor_name}"
                );
            }
        }
    }
}
