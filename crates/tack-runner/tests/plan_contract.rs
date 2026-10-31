//! `docs/contracts/plan-v1/plan.example.json` round-trips through the Rust
//! type that reads `tack-plan.json`, and its bytes are pinned.

use tack_runner::evidence::PlanV1;

const EXAMPLE: &str = include_str!("../../../docs/contracts/plan-v1/plan.example.json");

const NESTED_EXAMPLE: &str =
    include_str!("../../../docs/contracts/plan-v1/plan.nested.example.json");

/// FNV-1a 64 of the example's bytes, pinned like `evidence_contract.rs`.
const EXAMPLE_FNV1A64: u64 = 0xd8de_3976_e6f0_fe70;
/// FNV-1a 64 of the second example, which carries `type`, `role`, `team` and `parent`.
const NESTED_EXAMPLE_FNV1A64: u64 = 0x3871_8447_f832_6da7;

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn the_example_round_trips_and_its_pin_holds() {
    assert_eq!(fnv1a64(EXAMPLE.as_bytes()), EXAMPLE_FNV1A64);
    let value: serde_json::Value = serde_json::from_str(EXAMPLE).expect("example is JSON");
    let plan: PlanV1 = serde_json::from_value(value.clone()).expect("typed");
    assert_eq!(plan.subtasks.len(), 3);
    assert_eq!(plan.subtasks[2].depends_on, [0, 1]);
    assert_eq!(serde_json::to_value(&plan).expect("serializes"), value);
}

#[test]
fn the_second_example_with_type_role_team_and_parent_round_trips_and_its_pin_holds() {
    assert_eq!(fnv1a64(NESTED_EXAMPLE.as_bytes()), NESTED_EXAMPLE_FNV1A64);
    let value: serde_json::Value = serde_json::from_str(NESTED_EXAMPLE).expect("example is JSON");
    let plan: PlanV1 = serde_json::from_value(value.clone()).expect("typed");
    assert_eq!(plan.v, "1");
    assert_eq!(plan.subtasks[0].team.as_deref(), Some("my-saas-dev"));
    assert_eq!(plan.subtasks[1].parent, Some(0));
    assert_eq!(plan.subtasks[2].item_type.as_deref(), Some("task"));
    assert_eq!(plan.subtasks[2].role.as_deref(), Some("Frontend"));
    assert_eq!(serde_json::to_value(&plan).expect("serializes"), value);
}
