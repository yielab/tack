use crate::fixtures::{fixture_name, fixture_paths, load_value};
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
