//! Execution/fleet/runner/profile request bodies and display helpers shared
//! by `main.rs` (the `tack execution|fleet|runner|agent-profile|model-profile`
//! commands) and `mcp.rs` (the matching MCP tools), so the two entry points
//! can never build a differently-shaped request for the same operation.
//!
//! There is no `docs/contracts/runner-v1/` fixture for this surface — that
//! directory is the frozen authority for the *runner* wire protocol
//! (`/api/runner/v1/*`), a different, deliberately distinct domain from the
//! *operator* execution/fleet/runner/profile API this module targets (see
//! TODO.md III.0, "Vocabulary that must remain distinct"). The shape
//! authority for these routes is instead the request structs the handlers
//! themselves deserialize (`tack_api::handlers::executions::CreateExecution`
//! and `tack_api::handlers::runner_admin::{CreateFleet, CreateProfile,
//! CreateModelProfile, CreatePendingRunner}`), which every request-body
//! builder here is tested against directly (see the `shape` tests below) —
//! deserializing the exact JSON this module builds into the exact struct the
//! server deserializes, so a renamed or dropped field fails a test in this
//! crate instead of only surfacing as a live 400 later.

use serde_json::{Value, json};

/// Parse a CLI/MCP-supplied JSON blob argument. Unlike `tack field set`
/// (which falls back to treating unparsable input as a literal string,
/// appropriate for a single scalar field value), these arguments always map
/// onto a structured `Value` field on the wire — silently downgrading bad
/// JSON to a string here would send a request shaped nothing like what the
/// caller asked for. Fail with a clear, field-named message instead (III.2
/// rule 7: unsupported/invalid is reported, never quietly reinterpreted).
pub fn parse_json_field(raw: &str, field: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|e| format!("--{field} must be valid JSON: {e}"))
}

/// Same as [`parse_json_field`], but `None` (the flag was omitted) becomes
/// `{}` — every optional JSON-blob argument in this module defaults to an
/// empty object, matching the server's own defaults for the same fields
/// (`CreateFleet::default_policy`, `CreateProfile::tool_policy`/`limits`,
/// `CreatePendingRunner::labels`/`capability_snapshot`).
pub fn parse_json_field_or_empty(raw: Option<&str>, field: &str) -> Result<Value, String> {
    match raw {
        Some(s) => parse_json_field(s, field),
        None => Ok(json!({})),
    }
}

/// Generates a fresh per-invocation idempotency/recovery key when the caller
/// doesn't supply one. Deliberately *not* `uuid::Uuid` — the server treats
/// `idempotency_key`/`recovery_key` as an opaque `String` it hashes into a
/// scope, never as a parsed UUID (unlike `item_id`), so a random-enough
/// opaque string needs no extra dependency. A caller that wants a stable,
/// retry-safe key passes `--idempotency-key`/`--recovery-key` explicitly;
/// this default only has to avoid same-process collisions, not be
/// cryptographically unpredictable.
pub fn new_opaque_key(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{}-{nanos:x}", std::process::id())
}

// ── Execution request bodies ───────────────────────────────────────────────

/// One selector value from `--runner <id>` / `--fleet <id>` (mutually
/// exclusive; `main.rs`'s `clap::ArgGroup` enforces exactly one is given).
pub enum Selector {
    ExactRunner(String),
    Fleet(String),
}

impl Selector {
    fn kind(&self) -> &'static str {
        match self {
            Selector::ExactRunner(_) => "exact_runner",
            Selector::Fleet(_) => "fleet",
        }
    }
    fn id(&self) -> &str {
        match self {
            Selector::ExactRunner(id) | Selector::Fleet(id) => id,
        }
    }
}

#[derive(Default)]
pub struct CreateExecutionArgs<'a> {
    pub item_id: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub agent_profile_id: &'a str,
    pub requested_harness_kind: &'a str,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    /// JSON object matching `tack_orch::execution::AgentProfileSnapshot`:
    /// `{name, instructions, tool_policy, timeout_seconds, budgets}` are all
    /// required (`subdirectory`-style `Option` fields aside, this struct has
    /// none). Required, not defaulted to `{}` — an empty object always fails
    /// this shape server-side, so defaulting to it would just move a
    /// guaranteed error one step later instead of catching it at the CLI.
    pub agent_profile_snapshot: &'a str,
    pub repository_snapshot: &'a str,
    /// JSON object matching `tack_orch::execution::PermissionPolicy`:
    /// `{network: bool, tools: [string] (optional, default [])}`. Required
    /// for the same reason as `agent_profile_snapshot` — `network` has no
    /// default server-side, so `{}` cannot succeed.
    pub permission_policy: &'a str,
    pub budgets: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub metadata: Option<&'a str>,
    pub timeout_seconds: u64,
    pub status_map_policy_id: Option<&'a str>,
}

/// Already-parsed values for the same fields `CreateExecutionArgs` carries as
/// argv strings. `mcp.rs`'s `create_execution` tool receives these blobs as
/// native JSON-RPC values (an LLM caller writes a nested object, not a
/// stringified one) and builds this directly; `build_create_execution_body`
/// below parses the CLI's `--flag '<json text>'` strings into exactly this
/// shape and then defers to it. Either entry point ends here, so neither can
/// diverge from the other.
pub struct CreateExecutionValues<'a> {
    pub item_id: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub agent_profile_id: &'a str,
    pub requested_harness_kind: &'a str,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    pub agent_profile_snapshot: Value,
    pub repository_snapshot: Value,
    pub permission_policy: Value,
    pub budgets: Value,
    pub environment: Value,
    pub metadata: Value,
    pub timeout_seconds: u64,
    pub status_map_policy_id: Option<&'a str>,
}

/// The single place that assembles a `POST /api/executions` body
/// (`CreateExecution`'s exact field set) from already-parsed values. See
/// [`CreateExecutionValues`] for why both the CLI and MCP entry points call
/// this instead of building the `json!` object independently.
pub fn create_execution_body(selector: &Selector, values: CreateExecutionValues<'_>) -> Value {
    let idempotency_key = values
        .idempotency_key
        .map(str::to_owned)
        .unwrap_or_else(|| new_opaque_key("cli-exec"));
    json!({
        "item_id": values.item_id,
        "idempotency_key": idempotency_key,
        "selector_kind": selector.kind(),
        "selector_id": selector.id(),
        "agent_profile_id": values.agent_profile_id,
        "requested_harness_kind": values.requested_harness_kind,
        "requested_model_provider": values.requested_model_provider,
        "requested_model_id": values.requested_model_id,
        "agent_profile_snapshot": values.agent_profile_snapshot,
        "repository_snapshot": values.repository_snapshot,
        "permission_policy": values.permission_policy,
        "budgets": values.budgets,
        "environment": values.environment,
        "metadata": values.metadata,
        "timeout_seconds": values.timeout_seconds,
        "status_map_policy_id": values.status_map_policy_id,
    })
}

/// Builds the `POST /api/executions` body from the CLI's argv-string flags,
/// parsing each JSON-blob flag and then delegating to
/// [`create_execution_body`] — see that function's doc for why.
pub fn build_create_execution_body(
    selector: &Selector,
    args: &CreateExecutionArgs<'_>,
) -> Result<Value, String> {
    let values = CreateExecutionValues {
        item_id: args.item_id,
        idempotency_key: args.idempotency_key,
        agent_profile_id: args.agent_profile_id,
        requested_harness_kind: args.requested_harness_kind,
        requested_model_provider: args.requested_model_provider,
        requested_model_id: args.requested_model_id,
        agent_profile_snapshot: parse_json_field(
            args.agent_profile_snapshot,
            "agent-profile-snapshot",
        )?,
        repository_snapshot: parse_json_field(args.repository_snapshot, "repository")?,
        permission_policy: parse_json_field(args.permission_policy, "permission-policy")?,
        budgets: parse_json_field_or_empty(args.budgets, "budgets")?,
        environment: parse_json_field_or_empty(args.environment, "environment")?,
        metadata: parse_json_field_or_empty(args.metadata, "metadata")?,
        timeout_seconds: args.timeout_seconds,
        status_map_policy_id: args.status_map_policy_id,
    };
    Ok(create_execution_body(selector, values))
}

pub fn selector_from_flags(runner: Option<&str>, fleet: Option<&str>) -> Result<Selector, String> {
    match (runner, fleet) {
        (Some(r), None) => Ok(Selector::ExactRunner(r.to_string())),
        (None, Some(f)) => Ok(Selector::Fleet(f.to_string())),
        (None, None) => Err("exactly one of --runner or --fleet is required".to_string()),
        (Some(_), Some(_)) => Err("--runner and --fleet are mutually exclusive".to_string()),
    }
}

/// Builds the `POST /api/executions/{id}/requeue` body (`RecoveryConfirmation`).
pub fn build_requeue_body(recovery_key: &str, reason: &str) -> Value {
    json!({ "recovery_key": recovery_key, "reason": reason })
}

// ── Fleet / profile / runner bodies ────────────────────────────────────────

/// Builds the `POST /api/runner-fleets` body (`CreateFleet`).
pub fn build_create_fleet_body(
    name: &str,
    concurrency_limit: Option<i64>,
    default_policy: Option<&str>,
) -> Result<Value, String> {
    Ok(json!({
        "name": name,
        "concurrency_limit": concurrency_limit,
        "default_policy": parse_json_field_or_empty(default_policy, "policy")?,
    }))
}

/// Builds the `POST /api/agent-profiles` body (`CreateProfile`).
pub fn build_create_agent_profile_body(
    name: &str,
    instructions: &str,
    tool_policy: Option<&str>,
    limits: Option<&str>,
) -> Result<Value, String> {
    Ok(json!({
        "name": name,
        "instructions": instructions,
        "tool_policy": parse_json_field_or_empty(tool_policy, "tool-policy")?,
        "limits": parse_json_field_or_empty(limits, "limits")?,
    }))
}

/// Builds the `POST /api/model-profiles` body (`CreateModelProfile`). No JSON
/// blob arguments, so this cannot fail on parse.
pub fn build_create_model_profile_body(
    name: &str,
    model_provider: &str,
    model_id: &str,
    config_reference: Option<&str>,
) -> Value {
    json!({
        "name": name,
        "model_provider": model_provider,
        "model_id": model_id,
        "config_reference": config_reference,
    })
}

#[derive(Default)]
pub struct EnrollRunnerArgs<'a> {
    pub name: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub labels: Option<&'a str>,
    pub capability_snapshot: Option<&'a str>,
    pub protocol_version: Option<i64>,
    pub enrollment_lifetime_seconds: Option<i64>,
}

/// Builds the `POST /api/runners/enrollment` body (`CreatePendingRunner`).
/// Takes no secret — the raw enrollment token is generated server-side and
/// returned in the response, never supplied by the caller, so it never has
/// to flow through a CLI argument (and therefore never through `argv`/`ps`)
/// on the way in.
pub fn build_enroll_runner_body(args: &EnrollRunnerArgs<'_>) -> Result<Value, String> {
    let mut body = json!({
        "name": args.name,
        "labels": parse_json_field_or_empty(args.labels, "labels")?,
        "total_capacity": args.total_capacity,
        "available_capacity": args.available_capacity,
        "capability_snapshot": parse_json_field_or_empty(args.capability_snapshot, "capability-snapshot")?,
    });
    if let Some(v) = args.protocol_version {
        body["protocol_version"] = json!(v);
    }
    if let Some(v) = args.enrollment_lifetime_seconds {
        body["enrollment_lifetime_seconds"] = json!(v);
    }
    Ok(body)
}

// ── Display helpers ─────────────────────────────────────────────────────────

/// Short, fixed-width-friendly annotation for an execution request's
/// `state`, used in both `execution list`'s table and `execution get`'s
/// detail view so `needs_operator` and `lost` never render as if they were
/// just another quiet in-progress value (III-E5 acceptance: distinct,
/// visible outcomes, not collapsed into one generic line). States come from
/// the frozen III.1.1 lifecycle; an unrecognized value is flagged rather
/// than silently printed bare, in case a newer server adds one this CLI
/// doesn't know about yet. `execution get` prints additional guidance below
/// this marker for `needs_operator`/`lost` — see `main.rs`'s
/// `cmd_execution_get` — since a single-record detail view has room for
/// prose that a table column does not.
pub fn describe_state(state: &str) -> &'static str {
    match state {
        "queued" | "leased" | "preparing" | "running" | "waiting_decision" => "",
        "succeeded" => " (done)",
        "failed" => " (failed)",
        "cancelled" => " (cancelled)",
        "lost" => " (LOST)",
        "needs_operator" => " (NEEDS OPERATOR)",
        _ => " (unrecognized state)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The shape authority for this module's bodies: the exact structs the
    // handlers deserialize. `tack-cli` already depends on `tack-api` (to run
    // `tack serve` in-process), so importing them here costs nothing extra
    // and proves — mechanically, not by inspection — that every body this
    // module builds deserializes into the real request type.
    use tack_api::handlers::executions::{CreateExecution, RecoveryConfirmation};
    use tack_api::handlers::runner_admin::{
        CreateFleet, CreateModelProfile, CreatePendingRunner, CreateProfile,
    };

    /// Valid `AgentProfileSnapshot` JSON (`{name, instructions, tool_policy,
    /// timeout_seconds, budgets}`) reused across tests that don't care about
    /// its content, only that it round-trips.
    const AGENT_PROFILE_SNAPSHOT_JSON: &str = r#"{"name":"a","instructions":"be careful","tool_policy":{},"timeout_seconds":60,"budgets":{}}"#;
    /// Valid `PermissionPolicy` JSON (`{network}`; `tools` defaults to `[]`).
    const PERMISSION_POLICY_JSON: &str = r#"{"network":false}"#;
    /// Valid `RepositorySnapshot` JSON (`{kind, remote, base_revision}`;
    /// `subdirectory` is a genuine `Option` and may be omitted).
    const REPOSITORY_SNAPSHOT_JSON: &str =
        r#"{"kind":"git","remote":"https://example.test/repo.git","base_revision":"main"}"#;

    #[test]
    fn create_execution_body_matches_the_handler_struct_exact_runner() {
        let selector = Selector::ExactRunner("runr_1".into());
        let args = CreateExecutionArgs {
            item_id: "11111111-1111-1111-1111-111111111111",
            idempotency_key: Some("fixed-key"),
            agent_profile_id: "ap_1",
            requested_harness_kind: "claude_code",
            requested_model_provider: Some("anthropic"),
            requested_model_id: Some("claude-sonnet"),
            agent_profile_snapshot: AGENT_PROFILE_SNAPSHOT_JSON,
            repository_snapshot: REPOSITORY_SNAPSHOT_JSON,
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

    /// One level deeper than `CreateExecution`'s own (loosely-`Value`-typed)
    /// fields: `agent_profile_snapshot`, `repository_snapshot` and
    /// `permission_policy` are re-validated server-side against
    /// `tack_orch::execution::{AgentProfileSnapshot, RepositorySnapshot,
    /// PermissionPolicy}` when the handler builds `ExecutionRequestSnapshot`
    /// (`crates/tack-api/src/handlers/executions.rs::create_execution`).
    /// A live smoke test against a running server is what surfaced this —
    /// `CreateExecution` alone accepts `{}` for all three fields (they're
    /// untyped `Value`), but the deeper snapshot rejects it with
    /// `missing field \`network\`` (or `name`, or `remote`, ...). This test
    /// pins that the module's *documented-required* example JSON for these
    /// three fields actually satisfies the deeper types, so nobody has to
    /// rediscover this by hand against a live server again.
    #[test]
    fn create_execution_nested_blobs_satisfy_the_deeper_snapshot_types() {
        use tack_orch::execution::{AgentProfileSnapshot, PermissionPolicy, RepositorySnapshot};

        let _: AgentProfileSnapshot = serde_json::from_str(AGENT_PROFILE_SNAPSHOT_JSON)
            .expect("example agent_profile_snapshot must satisfy AgentProfileSnapshot");
        let _: RepositorySnapshot = serde_json::from_str(REPOSITORY_SNAPSHOT_JSON)
            .expect("example repository_snapshot must satisfy RepositorySnapshot");
        let _: PermissionPolicy = serde_json::from_str(PERMISSION_POLICY_JSON)
            .expect("example permission_policy must satisfy PermissionPolicy");

        // And confirm the historical failure mode really does fail — `{}`
        // for permission_policy specifically, since that was the exact
        // shape that produced "missing field `network`" against a live
        // server during this card's manual smoke test.
        let empty: Result<PermissionPolicy, _> = serde_json::from_str("{}");
        assert!(
            empty.is_err(),
            "an empty permission_policy must still fail — pins why this field is required, not defaulted"
        );
    }

    #[test]
    fn create_execution_body_defaults_optional_blobs_to_empty_object() {
        let selector = Selector::Fleet("fleet_1".into());
        let args = CreateExecutionArgs {
            item_id: "22222222-2222-2222-2222-222222222222",
            agent_profile_id: "ap_1",
            requested_harness_kind: "codex",
            agent_profile_snapshot: AGENT_PROFILE_SNAPSHOT_JSON,
            repository_snapshot: "{}",
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
            agent_profile_snapshot: AGENT_PROFILE_SNAPSHOT_JSON,
            repository_snapshot: "not json",
            permission_policy: PERMISSION_POLICY_JSON,
            timeout_seconds: 1,
            ..Default::default()
        };
        let err = build_create_execution_body(&selector, &args).unwrap_err();
        assert!(err.contains("--repository"), "unexpected: {err}");
    }

    #[test]
    fn create_execution_requires_agent_profile_snapshot_and_permission_policy() {
        // Neither has a safe empty default (both fail the deeper snapshot
        // types), so this module requires them rather than silently sending
        // `{}` and letting the server 400 one step later.
        let selector = Selector::ExactRunner("r".into());
        let missing_profile = CreateExecutionArgs {
            item_id: "id",
            agent_profile_id: "ap",
            requested_harness_kind: "codex",
            agent_profile_snapshot: "",
            repository_snapshot: REPOSITORY_SNAPSHOT_JSON,
            permission_policy: PERMISSION_POLICY_JSON,
            timeout_seconds: 1,
            ..Default::default()
        };
        assert!(build_create_execution_body(&selector, &missing_profile).is_err());

        let missing_policy = CreateExecutionArgs {
            item_id: "id",
            agent_profile_id: "ap",
            requested_harness_kind: "codex",
            agent_profile_snapshot: AGENT_PROFILE_SNAPSHOT_JSON,
            repository_snapshot: REPOSITORY_SNAPSHOT_JSON,
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
        let body =
            build_create_agent_profile_body("profile-a", "be concise", None, Some(r#"{"x":1}"#))
                .unwrap();
        let typed: CreateProfile =
            serde_json::from_value(body).expect("must deserialize into CreateProfile");
        assert_eq!(typed.name, "profile-a");
        assert_eq!(typed.instructions, "be concise");
        assert_eq!(typed.tool_policy, json!({}));
        assert_eq!(typed.limits, json!({"x": 1}));
    }

    #[test]
    fn model_profile_body_matches_create_model_profile() {
        let body = build_create_model_profile_body("gpt", "openai", "gpt-5", None);
        let typed: CreateModelProfile =
            serde_json::from_value(body).expect("must deserialize into CreateModelProfile");
        assert_eq!(typed.name, "gpt");
        assert_eq!(typed.model_provider, "openai");
        assert_eq!(typed.model_id, "gpt-5");
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
}
