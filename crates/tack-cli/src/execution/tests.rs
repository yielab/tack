use super::*;
// The shape authority for this module's bodies: the exact structs the
// handlers deserialize. `tack-cli` already depends on `tack-api` (to run
// `tack serve` in-process), so importing them here costs nothing extra
// and proves — mechanically, not by inspection — that every body this
// module builds deserializes into the real request type.
use tack_api::handlers::executions::{CreateExecution, RecoveryConfirmation};
use tack_api::handlers::runner_admin::{CreateFleet, CreatePendingRunner, CreateProfile};

/// Valid `PermissionPolicy` JSON (`{network}`; `tools` defaults to `[]`).
const PERMISSION_POLICY_JSON: &str = r#"{"network":false}"#;

#[test]
fn create_execution_body_matches_handler_struct_exact_runner() {
    let selector = Selector::ExactRunner("runr_1".into());
    let args = CreateExecutionArgs {
        item_id: "11111111-1111-1111-1111-111111111111",
        idempotency_key: Some("fixed-key"),
        agent_profile_id: "ap_1",
        requested_harness_kind: "claude_code",
        requested_model: Some("claude-sonnet"),
        permission_policy: PERMISSION_POLICY_JSON,
        timeout_seconds: 3600,
        ..Default::default()
    };
    let body = build_create_execution_body(&selector, &args).unwrap();
    assert_eq!(body["selector_kind"], "exact_runner");
    assert_eq!(body["selector_id"], "runr_1");
    assert_eq!(body["idempotency_key"], "fixed-key");

    let typed: CreateExecution = serde_json::from_value(body)
        .expect("CLI-built create_execution body must deserialize into CreateExecution");
    assert_eq!(typed.selector_kind, "exact_runner");
    assert_eq!(typed.selector_id, "runr_1");
    assert_eq!(typed.agent_profile_id, "ap_1");
    assert_eq!(typed.requested_harness_kind, "claude_code");
    assert_eq!(typed.timeout_seconds, 3600);
}

/// `permission_policy` is re-validated server-side against
/// `tack_orch::execution::PermissionPolicy`; `{}` fails it (`network` has no
/// default), so the example JSON must satisfy it and the CLI must require it.
#[test]
fn create_execution_permission_policy_satisfies_deeper_type() {
    use tack_orch::execution::PermissionPolicy;

    let _: PermissionPolicy = serde_json::from_str(PERMISSION_POLICY_JSON)
        .expect("example permission_policy must satisfy PermissionPolicy");
    let empty: Result<PermissionPolicy, _> = serde_json::from_str("{}");
    assert!(empty.is_err());
}

#[test]
fn create_execution_body_defaults_optional_blobs_to_empty() {
    let selector = Selector::Fleet("fleet_1".into());
    let args = CreateExecutionArgs {
        item_id: "22222222-2222-2222-2222-222222222222",
        agent_profile_id: "ap_1",
        requested_harness_kind: "codex",
        permission_policy: PERMISSION_POLICY_JSON,
        timeout_seconds: 60,
        ..Default::default()
    };
    let body = build_create_execution_body(&selector, &args).unwrap();
    assert_eq!(body["budgets"], json!({}));
    assert_eq!(body["environment"], json!({}));
    assert_eq!(body["metadata"], json!({}));
    // Auto-generated idempotency key must be non-empty and stable within
    // this single build (not re-derived on every access).
    assert!(!body["idempotency_key"].as_str().unwrap().is_empty());

    let _typed: CreateExecution =
        serde_json::from_value(body).expect("must deserialize with defaulted blobs");
}

#[test]
fn create_execution_rejects_invalid_json_in_a_named_field() {
    let selector = Selector::ExactRunner("r".into());
    let args = CreateExecutionArgs {
        item_id: "id",
        agent_profile_id: "ap",
        requested_harness_kind: "codex",
        permission_policy: PERMISSION_POLICY_JSON,
        budgets: Some("not json"),
        timeout_seconds: 1,
        ..Default::default()
    };
    let err = build_create_execution_body(&selector, &args).unwrap_err();
    assert!(err.contains("--budgets"), "unexpected: {err}");
}

#[test]
fn create_execution_args_require_policy() {
    let selector = Selector::ExactRunner("r".into());
    let missing_policy = CreateExecutionArgs {
        item_id: "id",
        agent_profile_id: "ap",
        requested_harness_kind: "codex",
        permission_policy: "",
        timeout_seconds: 1,
        ..Default::default()
    };
    assert!(build_create_execution_body(&selector, &missing_policy).is_err());
}

#[test]
fn selector_from_flags_requires_exactly_one() {
    assert!(selector_from_flags(None, None).is_err());
    assert!(selector_from_flags(Some("r"), Some("f")).is_err());
    assert!(matches!(
        selector_from_flags(Some("r"), None).unwrap(),
        Selector::ExactRunner(id) if id == "r"
    ));
    assert!(matches!(
        selector_from_flags(None, Some("f")).unwrap(),
        Selector::Fleet(id) if id == "f"
    ));
}

#[test]
fn requeue_body_matches_recovery_confirmation() {
    let body = build_requeue_body("rk-1", "runner crashed, verified no process");
    let typed: RecoveryConfirmation =
        serde_json::from_value(body).expect("must deserialize into RecoveryConfirmation");
    assert_eq!(typed.recovery_key, "rk-1");
    assert_eq!(typed.reason, "runner crashed, verified no process");
}

#[test]
fn fleet_body_matches_create_fleet() {
    let body = build_create_fleet_body("fleet-a", Some(4), None).unwrap();
    let typed: CreateFleet =
        serde_json::from_value(body).expect("must deserialize into CreateFleet");
    assert_eq!(typed.name, "fleet-a");
    assert_eq!(typed.concurrency_limit, Some(4));
    assert_eq!(typed.default_policy, json!({}));
}

#[test]
fn agent_profile_body_matches_create_profile() {
    let body = build_create_agent_profile_body("profile-a", "be concise", None, Some(r#"{"x":1}"#))
        .unwrap();
    let typed: CreateProfile =
        serde_json::from_value(body).expect("must deserialize into CreateProfile");
    assert_eq!(typed.name, "profile-a");
    assert_eq!(typed.instructions, "be concise");
    assert_eq!(typed.tool_policy, json!({}));
    assert_eq!(typed.limits, json!({"x": 1}));
}

#[test]
fn enroll_runner_body_matches_create_pending_runner() {
    let args = EnrollRunnerArgs {
        name: "runner-a",
        total_capacity: 2,
        available_capacity: 2,
        ..Default::default()
    };
    let body = build_enroll_runner_body(&args).unwrap();
    let typed: CreatePendingRunner =
        serde_json::from_value(body).expect("must deserialize into CreatePendingRunner");
    assert_eq!(typed.name, "runner-a");
    assert_eq!(typed.total_capacity, 2);
    assert_eq!(typed.available_capacity, 2);
    // Server-side defaults apply when the CLI omits the optional fields.
    assert_eq!(typed.protocol_version, 1);
    assert_eq!(typed.enrollment_lifetime_seconds, 60 * 60);
}

#[test]
fn enroll_runner_body_never_carries_a_token_field() {
    // The raw enrollment token is server-generated and returned in the
    // response; this request body must have no field that could carry a
    // caller-supplied secret (there would be nothing sensible for a
    // caller to put there anyway, but this pins the absence).
    let args = EnrollRunnerArgs {
        name: "runner-a",
        total_capacity: 1,
        available_capacity: 1,
        ..Default::default()
    };
    let body = build_enroll_runner_body(&args).unwrap();
    let obj = body.as_object().unwrap();
    for key in obj.keys() {
        assert!(
            !key.to_lowercase().contains("token") && !key.to_lowercase().contains("credential"),
            "unexpected secret-shaped field in enrollment request: {key}"
        );
    }
}

#[test]
fn describe_state_flags_needs_operator_and_lost_distinctly() {
    assert!(describe_state("needs_operator").contains("NEEDS OPERATOR"));
    assert!(describe_state("lost").contains("LOST"));
    assert_eq!(describe_state("running"), "");
    assert_eq!(describe_state("succeeded"), " (done)");
    assert!(describe_state("needs_operator") != describe_state("lost"));
    assert!(describe_state("something_future_state").contains("unrecognized"));
}
