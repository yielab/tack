use crate::fixtures::{fixture_name, fixture_paths, load_value};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::fs;
use tack_orch::execution::{
    validate_transition, ActualExecution, ActualModelId, ActualModelProvider, AttemptSnapshot,
    ExecutionRequestSnapshot, ExecutionState, ProtocolErrorEnvelope, RequestedModelId,
    RequestedModelProvider, RunnerCapabilities, TransitionActor, Usage,
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
