use super::*;

#[test]
fn opaque_model_ids_and_additive_fields_round_trip() {
    let raw = r#"{
      "protocol_version":1,"runner_version":"0.1.0","reported_at":"2026-08-06T12:00:00Z",
      "labels":{},"concurrency":{"total":1,"available":1},"harnesses":[{
        "harness_kind":"codex","installed_version":"1","probe_error":null,
        "probed_at":"2026-08-06T12:00:00Z","model_combinations":[{
          "model_provider":"openai","model_ids":["opaque/model-alpha"],"discovery":"reported","future_combo":true
        }]
      }],"features":{
        "cancel":{"support":"supported","reason":null},"resume":{"support":"unsupported","reason":"no"},
        "decisions":{"support":"supported","reason":null},"artifacts":{"support":"supported","reason":null},
        "usage":{"support":"advisory","reason":"partial"}
      },"limits":{"event_payload_bytes_max":1,"artifact_content_bytes_max":2},"future_capability":{"nested":true}
    }"#;
    let parsed: RunnerCapabilities = serde_json::from_str(raw).expect("capabilities fixture");
    assert_eq!(
        parsed.harnesses[0].model_combinations[0].model_ids[0].as_str(),
        "opaque/model-alpha"
    );
    let round_trip = serde_json::to_value(parsed).expect("serialize");
    assert_eq!(round_trip["future_capability"]["nested"], true);
    assert_eq!(
        round_trip["harnesses"][0]["model_combinations"][0]["future_combo"],
        true
    );
}

/// A runner built before this metadata existed sends a `ModelCombination`
/// with no `model_metadata` key at all. It must still parse (default to
/// an empty map) and, critically, must not gain the key on
/// re-serialization — `skip_serializing_if` is what keeps this payload
/// byte-identical to what an older runner actually sent.
#[test]
fn model_metadata_absent_on_an_older_runner_still_round_trips() {
    let raw = serde_json::json!({
        "model_provider": "openai",
        "model_ids": ["opaque/model-alpha"],
        "discovery": "reported"
    });
    let parsed: ModelCombination = serde_json::from_value(raw.clone())
        .expect("an older runner's model combination must still parse");
    assert!(
        parsed.model_metadata.is_empty(),
        "no model_metadata key means no per-model metadata was reported, not a claim of zero"
    );
    let round_trip = serde_json::to_value(&parsed).expect("serialize");
    assert_eq!(
        round_trip, raw,
        "an older runner's payload must round-trip byte-for-byte, with no model_metadata key materializing"
    );
}

/// A provider's catalog quotes some models fully and others partially —
/// proves price, context window and modality all round-trip per model,
/// and that a model the catalog said nothing about stays absent from
/// the map rather than appearing with zeroed fields.
fn model_metadata_entry<'a>(parsed: &'a ModelCombination, id: &str) -> &'a ModelMetadata {
    parsed
        .model_metadata
        .get(&ModelId::new(id))
        .unwrap_or_else(|| panic!("missing metadata for {id}"))
}

fn multi_model_metadata_fixture() -> serde_json::Value {
    serde_json::json!({
        "model_provider": "vercel-ai-gateway",
        "model_ids": ["openai/gpt-5.6-sol", "anthropic/claude-opus-5"],
        "discovery": "catalog_reported",
        "model_metadata": {
            "openai/gpt-5.6-sol": {
                "context_window": 400000,
                "price": {"input": "0.000002", "output": "0.000008"},
                "modality": {"input": ["text"], "output": ["text"]}
            },
            "anthropic/claude-opus-5": {
                "context_window": 500000
            }
        }
    })
}

#[test]
fn model_metadata_round_trips_price_window_and_modality() {
    let raw = multi_model_metadata_fixture();
    let parsed: ModelCombination =
        serde_json::from_value(raw.clone()).expect("populated per-model metadata must parse");
    assert_eq!(
        parsed.model_metadata.len(),
        2,
        "an entry was left out of the map"
    );

    let sol = model_metadata_entry(&parsed, "openai/gpt-5.6-sol");
    assert_eq!(sol.context_window, Some(400_000));
    assert!(sol.price.is_some());
    assert!(sol.modality.is_some());

    let opus = model_metadata_entry(&parsed, "anthropic/claude-opus-5");
    assert_eq!(opus.context_window, Some(500_000));
    assert!(
        opus.price.is_none(),
        "a model the catalog quoted no price for stays unmeasured, never zero"
    );

    assert_eq!(
        serde_json::to_value(&parsed).expect("serialize"),
        raw,
        "populated per-model metadata must round-trip exactly"
    );
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
