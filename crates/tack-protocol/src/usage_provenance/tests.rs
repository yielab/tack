use super::*;
use std::collections::BTreeMap;

fn measured<T>(value: T) -> Measurement<T> {
    Measurement {
        value: Some(value),
        source: MeasurementSource::Measured,
        additional: BTreeMap::new(),
    }
}

fn rfc3339(ts: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(ts)
        .unwrap()
        .with_timezone(&Utc)
}

fn actual(model: Option<&str>, source: &str) -> ActualExecution {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/completion.request.json"
    ))
    .expect("fixture JSON");
    let mut actual: ActualExecution =
        serde_json::from_value(fixture["actual_execution"].clone()).expect("actual_execution");
    actual.model = model.map(str::to_owned);
    actual.model_source = source.to_owned();
    actual
}

/// Both sides stay visible and verbatim, whether or not they agree, and
/// the source rides along: nothing is reconciled.
#[test]
fn provenance_is_requested_served_and_source_and_nothing_else() {
    let cases = [
        (Some("opaque/sonnet"), Some("opaque/sonnet"), "reported"),
        (Some("opaque/sonnet"), Some("opaque/haiku"), "reported"),
        (Some("opaque:sonnet"), Some("opaque:sonnet"), "requested"),
        (None, Some("opaque/model-alpha"), "reported"),
        (None, None, "agent_default"),
    ];
    for (requested, served, source) in cases {
        let provenance = model_provenance(requested, &actual(served, source));
        assert_eq!(
            serde_json::to_value(&provenance).expect("serialize"),
            serde_json::json!({"requested": requested, "served": served, "source": source}),
        );
    }
}

/// Absent usage must never serialize as zero. Asserts the literal JSON
/// shape, not just the Rust value, per CLAUDE.md's "assert the absence
/// directly."
#[test]
fn absent_usage_never_serializes_as_zero() {
    let economics = build_usage_economics(None, None, None, None);
    let json = serde_json::to_value(&economics).expect("serialize");
    assert_eq!(
        json,
        serde_json::json!({
            "model_token_cost_usd_estimated": {"value": null, "source": "not_measured"},
            "runner_time_cost": {
                "wall_clock_ms": null,
                "cost_usd_estimated": {"value": null, "source": "not_measured"}
            }
        })
    );
    // Literal-value sanity check on top of structural equality: no
    // numeric zero anywhere in the serialized economics.
    let raw = json.to_string();
    assert!(
        !raw.contains(":0"),
        "must never encode absent usage as 0: {raw}"
    );
    assert!(
        !raw.contains(":0.0"),
        "must never encode absent usage as 0.0: {raw}"
    );
}

/// The positive control for the test above: real inputs must actually
/// produce real, non-null values — proving the null-everywhere case
/// above is not simply a vacuous "always null" implementation.
#[test]
fn present_usage_and_timestamps_produce_real_values() {
    let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ended = DateTime::parse_from_rfc3339("2026-08-06T12:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let usage = Usage {
        tokens_in: measured(1234),
        tokens_out: measured(456),
        duration_ms: measured(1_800_000),
        cost_usd: measured(0.42),
        additional: BTreeMap::new(),
    };
    let economics = build_usage_economics(Some(&usage), Some(started), Some(ended), Some(3.0));
    assert_eq!(economics.model_token_cost_usd_estimated.value, Some(0.42));
    assert_eq!(
        economics.model_token_cost_usd_estimated.source,
        MeasurementSource::Measured
    );
    assert_eq!(economics.runner_time_cost.wall_clock_ms, Some(1_800_000));
    // 30 minutes at $3.00/hour = $1.50, and this figure is an
    // independent estimate — it must never equal (or be silently
    // summed with) the harness's own $0.42 cost_usd.
    assert_eq!(
        economics.runner_time_cost.cost_usd_estimated.value,
        Some(1.5)
    );
    assert_eq!(
        economics.runner_time_cost.cost_usd_estimated.source,
        MeasurementSource::Estimated
    );
    assert_ne!(
        economics.runner_time_cost.cost_usd_estimated.value,
        economics.model_token_cost_usd_estimated.value
    );
}

/// A wall clock is derivable even with no rate configured — it must not
/// collapse to `not_measured` just because the dollar estimate does.
#[test]
fn wall_clock_is_known_even_without_a_configured_rate() {
    let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ended = DateTime::parse_from_rfc3339("2026-08-06T12:00:05Z")
        .unwrap()
        .with_timezone(&Utc);
    let cost = compute_runner_time_cost(Some(started), Some(ended), None);
    assert_eq!(cost.wall_clock_ms, Some(5_000));
    assert_eq!(cost.cost_usd_estimated.value, None);
    assert_eq!(
        cost.cost_usd_estimated.source,
        MeasurementSource::NotMeasured
    );
}

#[test]
fn derive_attempt_facts_treats_malformed_json_as_unreported() {
    let facts = derive_attempt_facts(
        Some("opaque/model-alpha"),
        Some("not json"),
        Some("also not json"),
        None,
        None,
        None,
    );
    assert_eq!(facts.model_provenance, None);
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.value,
        None
    );
}

#[test]
fn derive_attempt_facts_end_to_end_from_a_real_fixture() {
    let actual_execution_json =
        include_str!("../../../../docs/contracts/runner-v1/completion.request.json");
    let completion: serde_json::Value =
        serde_json::from_str(actual_execution_json).expect("fixture JSON");
    let actual = completion["actual_execution"].to_string();
    let usage = completion["usage"].to_string();
    let facts = derive_attempt_facts(
        Some("opaque/model-alpha"),
        Some(&actual),
        Some(&usage),
        Some(rfc3339("2026-08-06T12:20:05Z")),
        Some(rfc3339("2026-08-06T12:25:00Z")),
        None,
    );
    assert_eq!(
        facts.model_provenance,
        Some(ModelProvenance {
            requested: Some("opaque/model-alpha".to_string()),
            served: Some("opaque/model-alpha".to_string()),
            source: "reported".to_string(),
        })
    );
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.value,
        None
    );
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.source,
        MeasurementSource::NotMeasured
    );
    assert_eq!(
        facts.usage_economics.runner_time_cost.wall_clock_ms,
        Some(295_000)
    );
}
