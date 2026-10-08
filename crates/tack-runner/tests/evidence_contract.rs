//! `docs/contracts/evidence-v1/attempt-evidence.example.json` round-trips
//! through the Rust type that writes `evidence.json`, and its bytes are pinned.

use tack_runner::evidence::AttemptEvidence;

const EXAMPLE: &str =
    include_str!("../../../docs/contracts/evidence-v1/attempt-evidence.example.json");

/// FNV-1a 64 of the example's bytes, pinned like `runner_contract/fixtures.rs`.
const EXAMPLE_FNV1A64: u64 = 0x0f64_7195_33b7_8713;

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn the_example_round_trips_and_its_pin_holds() {
    assert_eq!(fnv1a64(EXAMPLE.as_bytes()), EXAMPLE_FNV1A64);
    let value: serde_json::Value = serde_json::from_str(EXAMPLE).expect("example is JSON");
    let evidence: AttemptEvidence = serde_json::from_value(value.clone()).expect("typed");
    assert_eq!(evidence.files.len(), 2);
    assert_eq!(serde_json::to_value(&evidence).expect("serializes"), value);
}
