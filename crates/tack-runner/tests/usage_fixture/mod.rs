//! The frozen usage fixture, for the suites that build a completion without varying it.

pub fn usage() -> tack_protocol::execution::Usage {
    serde_json::from_str(
        r#"{
            "tokens_in":{"value":1,"source":"measured"},
            "tokens_out":{"value":2,"source":"measured"},
            "duration_ms":{"value":3,"source":"measured"},
            "cost_usd":{"value":null,"source":"not_measured"}
        }"#,
    )
    .expect("usage")
}
