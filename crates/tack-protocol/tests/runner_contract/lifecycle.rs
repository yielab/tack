use crate::fixtures::load_value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LifecycleContract {
    protocol_version: u16,
    states: Vec<String>,
    terminal_states: Vec<String>,
    rules: Vec<TransitionRule>,
    replay_rule: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TransitionRule {
    from: String,
    allow: BTreeMap<String, Vec<String>>,
    deny: Vec<String>,
}

fn lifecycle_contract() -> LifecycleContract {
    serde_json::from_value(load_value("lifecycle-transitions.json"))
        .expect("lifecycle fixture must match its contract shape")
}

fn validate(contract: &LifecycleContract) -> Result<(), String> {
    if contract.protocol_version != 1 {
        return Err("protocol_version must remain 1".to_owned());
    }

    let states = contract.states.iter().collect::<BTreeSet<_>>();
    if states.len() != contract.states.len() {
        return Err("states must be unique".to_owned());
    }
    if contract.rules.len() != states.len() {
        return Err("every state must have exactly one source rule".to_owned());
    }

    let mut sources = BTreeSet::new();
    for rule in &contract.rules {
        if !states.contains(&rule.from) {
            return Err(format!("unknown source state {}", rule.from));
        }
        if !sources.insert(&rule.from) {
            return Err(format!("duplicate source rule {}", rule.from));
        }

        let mut targets = BTreeSet::new();
        for (target, actors) in &rule.allow {
            if !states.contains(target) {
                return Err(format!("unknown allowed target {target}"));
            }
            if actors.is_empty() {
                return Err(format!(
                    "allowed transition {} -> {target} has no actor",
                    rule.from
                ));
            }
            if !targets.insert(target) {
                return Err(format!("duplicate transition {} -> {target}", rule.from));
            }
        }
        for target in &rule.deny {
            if !states.contains(target) {
                return Err(format!("unknown denied target {target}"));
            }
            if !targets.insert(target) {
                return Err(format!(
                    "transition {} -> {target} is both allowed and denied",
                    rule.from
                ));
            }
        }
        if targets != states {
            return Err(format!(
                "source {} does not classify every target",
                rule.from
            ));
        }
    }

    for terminal in &contract.terminal_states {
        let rule = contract
            .rules
            .iter()
            .find(|rule| &rule.from == terminal)
            .ok_or_else(|| format!("terminal state {terminal} has no rule"))?;
        if !rule.allow.is_empty() {
            return Err(format!(
                "terminal state {terminal} has an outbound transition"
            ));
        }
    }

    Ok(())
}

#[test]
fn lifecycle_fixture_classifies_all_one_hundred_ordered_pairs_once() {
    let contract = lifecycle_contract();
    validate(&contract).expect("frozen lifecycle contract must be complete");

    let classified = contract
        .rules
        .iter()
        .map(|rule| rule.allow.len() + rule.deny.len())
        .sum::<usize>();
    assert_eq!(classified, 100);
}

#[test]
fn lifecycle_mutations_fail_with_stable_named_reasons() {
    let original = lifecycle_contract();

    let mut missing = original.clone();
    missing.rules[0].deny.pop();
    assert_eq!(
        validate(&missing).unwrap_err(),
        "source queued does not classify every target"
    );

    let mut duplicate = original.clone();
    duplicate.rules[0].deny.push("leased".to_owned());
    assert_eq!(
        validate(&duplicate).unwrap_err(),
        "transition queued -> leased is both allowed and denied"
    );

    let mut terminal_reopened = original;
    terminal_reopened.rules[5]
        .allow
        .insert("queued".to_owned(), vec!["operator".to_owned()]);
    terminal_reopened.rules[5]
        .deny
        .retain(|target| target != "queued");
    assert_eq!(
        validate(&terminal_reopened).unwrap_err(),
        "terminal state succeeded has an outbound transition"
    );
}

#[test]
fn lifecycle_fixture_structural_round_trip_preserves_unknown_model_independence() {
    let contract = lifecycle_contract();
    let encoded = serde_json::to_value(&contract).expect("lifecycle contract serializes");
    let decoded: LifecycleContract =
        serde_json::from_value(encoded).expect("lifecycle contract deserializes");
    assert_eq!(decoded.states, contract.states);
    assert_eq!(decoded.replay_rule, contract.replay_rule);
}
