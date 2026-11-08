use super::*;
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

/// A wall clock needs both ends; one missing end is `None`, never a partial duration.
#[test]
fn wall_clock_needs_both_ends() {
    let started = rfc3339("2026-08-06T12:00:00Z");
    let ended = rfc3339("2026-08-06T12:00:05Z");
    assert_eq!(wall_clock_ms(Some(started), Some(ended)), Some(5_000));
    assert_eq!(wall_clock_ms(Some(started), None), None);
    assert_eq!(wall_clock_ms(None, Some(ended)), None);
    // A clock that ran backwards clamps to zero instead of underflowing.
    assert_eq!(wall_clock_ms(Some(ended), Some(started)), Some(0));
}

#[test]
fn derive_attempt_facts_treats_malformed_json_as_unreported() {
    let facts = derive_attempt_facts(Some("opaque/model-alpha"), Some("not json"), None, None);
    assert_eq!(facts.model_provenance, None);
    assert_eq!(facts.wall_clock_ms, None);
}

#[test]
fn derive_attempt_facts_end_to_end_from_a_real_fixture() {
    let actual_execution_json =
        include_str!("../../../../docs/contracts/runner-v1/completion.request.json");
    let completion: serde_json::Value =
        serde_json::from_str(actual_execution_json).expect("fixture JSON");
    let actual = completion["actual_execution"].to_string();
    let facts = derive_attempt_facts(
        Some("opaque/model-alpha"),
        Some(&actual),
        Some(rfc3339("2026-08-06T12:20:05Z")),
        Some(rfc3339("2026-08-06T12:25:00Z")),
    );
    assert_eq!(
        facts.model_provenance,
        Some(ModelProvenance {
            requested: Some("opaque/model-alpha".to_string()),
            served: Some("opaque/model-alpha".to_string()),
            source: "reported".to_string(),
        })
    );
    assert_eq!(facts.wall_clock_ms, Some(295_000));
}
