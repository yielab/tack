use super::*;

#[test]
fn additive_fields_round_trip() {
    let raw = r#"{
      "protocol_version":1,"runner_version":"0.1.0","reported_at":"2026-08-06T12:00:00Z",
      "labels":{},"concurrency":{"total":1,"available":1},"harnesses":[{
        "harness_kind":"codex","installed_version":"1","probe_error":null,
        "probed_at":"2026-08-06T12:00:00Z","requires_model":true,"requires_model_reason":"name one","models":{"source":"listed","ids":["a"],"reason":null},"sign_in":{"state":"signed_out","detail":"Not logged in","command":"codex login"},"docs":{"models":"https://example.test/m","sign_in":"https://example.test/s"},"future_harness_key":true
      }],"features":{
        "cancel":{"support":"supported","reason":null},"resume":{"support":"unsupported","reason":"no"},
        "decisions":{"support":"supported","reason":null},"artifacts":{"support":"supported","reason":null},
        "usage":{"support":"advisory","reason":"partial"}
      },"limits":{"event_payload_bytes_max":1,"artifact_content_bytes_max":2},"future_capability":{"nested":true}
    }"#;
    let parsed: RunnerCapabilities = serde_json::from_str(raw).expect("capabilities fixture");
    let harness = &parsed.harnesses[0];
    assert!(harness.requires_model);
    assert_eq!(
        harness.models.as_ref().map(|m| m.source),
        Some(ModelListSource::Listed)
    );
    assert_eq!(
        harness.sign_in.as_ref().map(|s| s.state),
        Some(SignInState::SignedOut)
    );
    assert_eq!(harness.default_model, None);
    let round_trip = serde_json::to_value(parsed).expect("serialize");
    assert_eq!(round_trip["future_capability"]["nested"], true);
    assert_eq!(round_trip["harnesses"][0]["future_harness_key"], true);
}

/// `RunnerCapabilities` cannot parse either embedded snapshot: both
/// fixtures omit `runner_version` (a sibling field in the enclosing
/// envelope, not nested under `capabilities`), which `RunnerCapabilities`
/// requires. `EmbeddedCapabilitySnapshot` is the type for this shape;
/// this test proves it parses enrollment's full example and refresh's
/// sparse one (`"harnesses": []`, `"features": {}`) unchanged, and that
/// unknown keys still survive a round trip.
/// Neither fixture parses as `RunnerCapabilities` (both omit
/// `runner_version`, a sibling field in the enclosing envelope) but both
/// parse as, and round-trip exactly through, `EmbeddedCapabilitySnapshot`.
/// Returns the parsed value so the caller can check fixture-specific shape.
fn assert_embedded_snapshot_round_trips(
    capabilities: &serde_json::Value,
) -> EmbeddedCapabilitySnapshot {
    assert!(
        serde_json::from_value::<RunnerCapabilities>(capabilities.clone()).is_err(),
        "an embedded snapshot omitting runner_version must not parse as RunnerCapabilities"
    );
    let parsed: EmbeddedCapabilitySnapshot =
        serde_json::from_value(capabilities.clone()).expect("embedded capabilities");
    assert_eq!(
        serde_json::to_value(&parsed).expect("serialize embedded capabilities"),
        *capabilities,
        "an embedded snapshot must round-trip exactly"
    );
    parsed
}

#[test]
fn embedded_capability_snapshot_parses_full_and_sparse_fixtures() {
    let enrollment: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/enrollment.request.json"
    ))
    .expect("enrollment fixture JSON");
    let parsed_enrollment = assert_embedded_snapshot_round_trips(&enrollment["capabilities"]);
    assert_eq!(parsed_enrollment.harnesses.len(), 1);
    assert_eq!(parsed_enrollment.features["cancel"]["support"], "supported");

    let refresh: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/refresh.request.json"
    ))
    .expect("refresh fixture JSON");
    let parsed_refresh = assert_embedded_snapshot_round_trips(&refresh["capabilities"]);
    assert!(parsed_refresh.harnesses.is_empty());
    assert_eq!(parsed_refresh.features, serde_json::json!({}));

    let mut with_future_field = enrollment["capabilities"].clone();
    with_future_field["future_capability_field"] = serde_json::json!({"nested": true});
    let parsed_additive: EmbeddedCapabilitySnapshot =
        serde_json::from_value(with_future_field.clone()).expect("parse additive field");
    assert_eq!(
        serde_json::to_value(parsed_additive).expect("serialize additive field"),
        with_future_field,
        "unrecognised keys on an embedded snapshot must survive a round trip"
    );
}

#[test]
fn team_ceiling_fields_are_optional_and_round_trip() {
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.json"
    ))
    .expect("capabilities fixture JSON");
    let parsed: RunnerCapabilities = serde_json::from_value(old.clone()).expect("old snapshot");
    assert_eq!(parsed.harnesses[0].autonomy_ceiling, None);
    assert_eq!(parsed.harnesses[0].teams, None);
    assert_eq!(serde_json::to_value(&parsed).expect("serialize"), old);

    let with: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.with-teams.json"
    ))
    .expect("with-teams fixture JSON");
    let parsed: RunnerCapabilities = serde_json::from_value(with.clone()).expect("with teams");
    let harness = &parsed.harnesses[0];
    assert_eq!(harness.autonomy_ceiling.as_deref(), Some("L3"));
    let teams = harness.teams.as_ref().expect("teams reported");
    assert_eq!(teams[0].violations[0].fix, "run it lights-out");
    assert_eq!(serde_json::to_value(&parsed).expect("serialize"), with);
}
