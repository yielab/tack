//! The one canonical form of a JSON value: object keys sorted at every depth, so two values that
//! say the same thing serialize to the same text whatever order their keys arrived in. Request
//! snapshots, replay fingerprints and stored payloads are compared in this form.

use serde_json::Value;

/// `value` with every object's keys in sorted order.
pub fn canonical_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical_json(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        value => value,
    }
}
