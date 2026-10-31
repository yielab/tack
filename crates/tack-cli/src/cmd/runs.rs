use clap::Subcommand;
use serde_json::json;

use tack_cli::client::TackClient;

use tack_cli::execution;

use super::common::*;

/// `execution create`'s flags, boxed at the `ExecutionAction::Create` call
/// site below — `CreateExecution` has by far the widest field set of any
/// command in this CLI, and clippy's `large_enum_variant` is right that
/// inlining it makes every other `ExecutionAction`/`Commands` variant pay
/// for stack space it doesn't use. A plain `#[derive(clap::Args)]` struct
/// behind a `Box` keeps the flags exactly as they'd read inline (`clap`
/// flattens them back out for parsing) without that cost.
#[derive(clap::Args)]
pub(crate) struct ExecutionCreateArgs {
    /// Item ID this execution is for
    item_id: String,
    /// Idempotency key (defaults to a fresh one each call; pass a stable
    /// value to safely retry the exact same request)
    #[arg(long)]
    idempotency_key: Option<String>,
    /// Run on this exact runner ID (mutually exclusive with --fleet; with
    /// neither, the profile's fleet, else the only active runner)
    #[arg(long)]
    runner: Option<String>,
    /// Run on any eligible runner in this fleet (mutually exclusive with --runner)
    #[arg(long)]
    fleet: Option<String>,
    /// Agent profile ID (omit to use the item's role, else the project's default)
    #[arg(long)]
    agent_profile: Option<String>,
    /// Requested harness kind, e.g. codex, claude-code (omit to use the
    /// profile's agent, else the project's default agent)
    #[arg(long)]
    harness: Option<String>,
    /// The model to run, passed to the agent as given (omit to use the project's
    /// default for its default agent, else the agent's own choice)
    #[arg(long)]
    model: Option<String>,
    /// Permission/tool policy, as a JSON object. Required field: network
    /// (bool). Optional: tools (array of strings, default []) — there is no
    /// safe empty default for this field (network has none server-side).
    #[arg(long)]
    permission_policy: String,
    /// Budgets, as a JSON object (default: {})
    #[arg(long)]
    budgets: Option<String>,
    /// Bounded environment metadata, as a JSON object (default: {})
    #[arg(long)]
    environment: Option<String>,
    /// Free-form metadata, as a JSON object (default: {})
    #[arg(long)]
    metadata: Option<String>,
    /// Execution timeout, in seconds (default: the profile's time limit, else one hour)
    #[arg(long)]
    timeout_seconds: Option<u64>,
    /// Status-map policy ID: done_on_success or done_on_mrp_accepted
    #[arg(long)]
    status_map_policy: Option<String>,
    /// Output raw JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
pub(crate) enum ExecutionAction {
    /// Create (or idempotently replay) an execution request
    Create {
        #[command(flatten)]
        args: Box<ExecutionCreateArgs>,
    },
    /// List execution requests (newest first)
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Get one execution request's current lifecycle state
    Get {
        /// Execution request ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Request cancellation of an execution (recorded as a request only —
    /// the runner observes and reports the actual outcome)
    Cancel {
        /// Execution request ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Requeue a needs_operator execution after an audited recovery decision
    Reconcile {
        /// Execution request ID
        id: String,
        /// Idempotency key scoping this recovery confirmation
        #[arg(long)]
        recovery_key: String,
        /// Human-readable justification for the recovery decision
        #[arg(long)]
        reason: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum FleetAction {
    /// Create a runner fleet
    Create {
        /// Fleet name
        name: String,
        /// Optional concurrency limit
        #[arg(long)]
        concurrency_limit: Option<i64>,
        /// Default policy, as a JSON object (default: {})
        #[arg(long)]
        policy: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List runner fleets
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum RunnerAction {
    /// Create a pending runner and issue a one-time enrollment token.
    ///
    /// The raw token is shown exactly once — by the server, in this
    /// response — and is never stored anywhere retrievable again. Pass
    /// --out to save it straight to a file (written atomically, owner-only)
    /// instead of printing it to the terminal.
    Enroll {
        /// Runner name
        name: String,
        /// Total execution slot capacity
        #[arg(long)]
        total_capacity: i64,
        /// Currently available execution slot capacity
        #[arg(long)]
        available_capacity: i64,
        /// Labels, as a JSON object (default: {})
        #[arg(long)]
        labels: Option<String>,
        /// Initial capability snapshot, as a JSON object (default: {})
        #[arg(long)]
        capability_snapshot: Option<String>,
        /// Runner protocol version this runner will speak (default: 1)
        #[arg(long)]
        protocol_version: Option<i64>,
        /// How long the enrollment token stays redeemable, in seconds (default: 3600)
        #[arg(long)]
        enrollment_lifetime_secs: Option<i64>,
        /// Write the enrollment response to this file (atomic, owner-only)
        /// instead of printing the raw token to the terminal
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Output raw JSON (to stdout; independent of --out)
        #[arg(long)]
        json: bool,
    },
    /// Revoke a runner (its credential stops authenticating immediately)
    Revoke {
        /// Runner ID
        runner_id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Revoke a specific unredeemed enrollment token
    RevokeToken {
        /// Runner ID
        runner_id: String,
        /// Enrollment token ID
        token_id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Run the runner role in this process, speaking runner-v1 over HTTP
    /// against a Tack server exactly like the standalone tack-runner binary.
    Start {
        /// Optional TOML configuration file.
        #[arg(long)]
        config: Option<std::path::PathBuf>,
        /// Runner protocol endpoint. Overrides file and environment configuration.
        #[arg(long)]
        api_url: Option<String>,
        /// Stable identifier sent to the control plane.
        #[arg(long)]
        runner_id: Option<String>,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
        /// Enrollment credential. Prefer TACK_RUNNER_ENROLLMENT_TOKEN so it is not visible in shell history.
        #[arg(long, hide_env_values = true)]
        enrollment_token: Option<String>,
    },
    /// Report which harness binaries this machine has, what each declares
    /// it can do, and where its credentials come from — the same
    /// discovery/capability probe `runner start` performs, without
    /// enrolling anything or requiring a server.
    Doctor {
        /// Output the raw capability snapshot as JSON instead of the
        /// human-readable report.
        #[arg(long)]
        json: bool,
        /// The runner's configuration file; defaults to TACK_LOCAL_RUNNER_CONFIG.
        #[arg(long)]
        config: Option<std::path::PathBuf>,
    },
    /// Manage this machine's runner-local secret store — the secrets
    /// a harness's environment can reference via `secret_reference` without
    /// ever putting them in a request body, a log line, or the board's own
    /// database.
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum SecretAction {
    /// Store a secret under `name` . The
    /// value is read from TACK_RUNNER_SECRET_VALUE or, if unset, from
    /// stdin — never from argv.
    Set {
        /// Entry name
        name: String,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
    },
    /// List the entry names held in the store. Never prints a value.
    List {
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a secret. Not an error if it was already absent.
    Remove {
        /// Entry name
        name: String,
        /// Local directory for runner state. Overrides file and environment configuration.
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ServiceAction {
    /// Write the service unit and start it, enabled to start at login
    Install,
    /// Stop the service and remove its unit. The data root is left untouched.
    Uninstall,
    /// Show whether the service is active and where to check its health
    Status,
}

#[derive(Subcommand)]
pub(crate) enum AgentProfileAction {
    /// Create an agent profile
    Create {
        /// Profile name
        name: String,
        /// Agent instructions
        #[arg(long)]
        instructions: String,
        /// Tool policy, as a JSON object (default: {})
        #[arg(long)]
        tool_policy: Option<String>,
        /// Limits, as a JSON object (default: {})
        #[arg(long)]
        limits: Option<String>,
        /// Fleet ID runs with this profile go to when they name no runner or fleet
        #[arg(long)]
        fleet: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List agent profiles
    List {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}

// ─── Execution commands ───────────────────────────────────────────────────────
//
// Every body sent below is built by `tack_cli::execution` — the same module
// `mcp.rs`'s execution tools call into — so the CLI and MCP entry points can
// never construct a differently-shaped request for the same operation:
// a CLI/MCP-issued request produces the same payload
// shape as the UI would. See `execution.rs`'s module doc for why the
// backend's own request structs, not a hand-written contract fixture, are
// the shape authority for this surface.

struct ExecutionCreateOpts {
    item_id: String,
    idempotency_key: Option<String>,
    runner: Option<String>,
    fleet: Option<String>,
    agent_profile: Option<String>,
    harness: Option<String>,
    model: Option<String>,
    permission_policy: String,
    budgets: Option<String>,
    environment: Option<String>,
    metadata: Option<String>,
    timeout_seconds: Option<u64>,
    status_map_policy: Option<String>,
}

fn cmd_execution_create(
    client: &TackClient,
    opts: ExecutionCreateOpts,
    as_json: bool,
) -> anyhow::Result<()> {
    let selector = execution::selector_from_flags(opts.runner.as_deref(), opts.fleet.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    let args = execution::CreateExecutionArgs {
        item_id: &opts.item_id,
        idempotency_key: opts.idempotency_key.as_deref(),
        agent_profile_id: opts.agent_profile.as_deref(),
        requested_harness_kind: opts.harness.as_deref(),
        requested_model: opts.model.as_deref(),
        permission_policy: &opts.permission_policy,
        budgets: opts.budgets.as_deref(),
        environment: opts.environment.as_deref(),
        metadata: opts.metadata.as_deref(),
        timeout_seconds: opts.timeout_seconds,
        status_map_policy_id: opts.status_map_policy.as_deref(),
    };
    let body = execution::build_create_execution_body(selector.as_ref(), &args)
        .map_err(|e| anyhow::anyhow!(e))?;
    let resp = client.post("/executions", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["request_id"].as_str().unwrap_or("?");
    let state = resp["state"].as_str().unwrap_or("?");
    let replayed = resp["replayed"].as_bool().unwrap_or(false);
    if replayed {
        println!(
            "Replayed existing execution request: {} (idempotency key already used)",
            &id[..8.min(id.len())]
        );
    } else {
        println!("Created execution request: {}", &id[..8.min(id.len())]);
    }
    println!("  state: {state}{}", execution::describe_state(state));
    println!("  id:    {id}");
    Ok(())
}

fn cmd_execution_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/executions")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No execution requests found.");
        return Ok(());
    }
    print_table_header(&["ID", "ITEM", "STATE", "CREATED"]);
    for r in rows {
        let state = r["state"].as_str().unwrap_or("?");
        let state_label = format!("{state}{}", execution::describe_state(state));
        print_table_row(&[
            short_id(r["request_id"].as_str()),
            short_id(r["item_id"].as_str()),
            &state_label,
            short_date(r["created_at"].as_str()),
        ]);
    }
    Ok(())
}

fn cmd_execution_get(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get(&format!("/executions/{id}"))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let request_id = resp["request_id"].as_str().unwrap_or(&id);
    let state = resp["state"].as_str().unwrap_or("?");
    println!("Execution request {request_id}");
    println!("  item:    {}", resp["item_id"].as_str().unwrap_or("?"));
    println!("  state:   {state}{}", execution::describe_state(state));
    println!("  created: {}", resp["created_at"].as_str().unwrap_or("?"));
    if let Some(cancel_at) = resp["cancellation_requested_at"].as_str() {
        println!("  cancellation requested at: {cancel_at}");
    }
    // needs_operator/lost are surfaced as visibly
    // distinct outcomes, not collapsed into the same quiet summary as
    // every other state — a single-record view has room to say what to do
    // about it, not just flag that it happened.
    match state {
        "needs_operator" => println!(
            "  \u{26a0} needs_operator: ambiguous outcome — safe retry could not be proven. \
             After confirming it's safe to recover, run \
             `tack execution reconcile {request_id} --recovery-key <key> --reason <text>`."
        ),
        "lost" => println!(
            "  \u{26a0} lost: no known running process was observed for this attempt. \
             Review before requeuing — a lease expiry alone never launches a second process."
        ),
        _ => {}
    }
    Ok(())
}

fn cmd_execution_cancel(client: &TackClient, id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post(&format!("/executions/{id}/cancel"), &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    // `cancelled` means nobody held the run, so it ended at once. Otherwise
    // the state is the literal "cancellation_requested" — not one of the
    // frozen `ExecutionState` values — because the runner holding the run
    // observes the request and reports the actual outcome; say that plainly
    // rather than implying the request is already terminal.
    if resp["state"] == "cancelled" {
        println!("Cancelled.");
        return Ok(());
    }
    println!(
        "Cancellation requested for {} — the request is not yet terminal; \
         `tack execution get {id}` will show the outcome once the runner reports it.",
        &id[..8.min(id.len())]
    );
    Ok(())
}

fn cmd_execution_reconcile(
    client: &TackClient,
    id: String,
    recovery_key: String,
    reason: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_requeue_body(&recovery_key, &reason);
    let resp = client.post(&format!("/executions/{id}/requeue"), &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let replayed = resp["replayed"].as_bool().unwrap_or(false);
    if replayed {
        println!(
            "Replayed existing recovery confirmation for {} (recovery key already used)",
            &id[..8.min(id.len())]
        );
    } else {
        println!(
            "Requeued {} from needs_operator — state: queued",
            &id[..8.min(id.len())]
        );
    }
    Ok(())
}

// ─── Fleet commands ───────────────────────────────────────────────────────────

fn cmd_fleet_create(
    client: &TackClient,
    name: String,
    concurrency_limit: Option<i64>,
    policy: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let body = execution::build_create_fleet_body(&name, concurrency_limit, policy.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    let resp = client.post("/runner-fleets", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["fleet_id"].as_str().unwrap_or("?");
    println!("Created fleet: {} ({})", name, &id[..8.min(id.len())]);
    println!("  id: {id}");
    Ok(())
}

fn cmd_fleet_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/runner-fleets")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No fleets found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME", "CONCURRENCY"]);
    for f in rows {
        let concurrency = f["concurrency_limit"]
            .as_i64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        print_table_row(&[
            short_id(f["fleet_id"].as_str()),
            f["name"].as_str().unwrap_or("?"),
            &concurrency,
        ]);
    }
    Ok(())
}

// ─── Runner commands ──────────────────────────────────────────────────────────

struct RunnerEnrollOpts {
    name: String,
    total_capacity: i64,
    available_capacity: i64,
    labels: Option<String>,
    capability_snapshot: Option<String>,
    protocol_version: Option<i64>,
    enrollment_lifetime_secs: Option<i64>,
}

fn cmd_runner_enroll(
    client: &TackClient,
    opts: RunnerEnrollOpts,
    out: Option<std::path::PathBuf>,
    as_json: bool,
) -> anyhow::Result<()> {
    let args = execution::EnrollRunnerArgs {
        name: &opts.name,
        total_capacity: opts.total_capacity,
        available_capacity: opts.available_capacity,
        labels: opts.labels.as_deref(),
        capability_snapshot: opts.capability_snapshot.as_deref(),
        protocol_version: opts.protocol_version,
        enrollment_lifetime_seconds: opts.enrollment_lifetime_secs,
    };
    let body = execution::build_enroll_runner_body(&args).map_err(|e| anyhow::anyhow!(e))?;
    // The raw enrollment token never flowed in as a CLI argument (it doesn't
    // exist until the server generates it in the response below), so there
    // is nothing here that could have appeared in `ps`/argv on the way in.
    let resp = client.post("/runners/enrollment", &body)?;

    if let Some(path) = &out {
        // Written atomically and owner-only — this file carries the same
        // one-time-secret weight as `~/.tackrc`'s bearer token.
        let contents = serde_json::to_vec_pretty(&resp)?;
        tack_cli::secure_fs::write_owner_only_atomic(path, &contents)
            .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", path.display()))?;
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    let runner_id = resp["runner_id"].as_str().unwrap_or("?");
    let token_id = resp["token_id"].as_str().unwrap_or("?");
    let expires_at = resp["expires_at"].as_str().unwrap_or("?");
    println!("Enrolled runner: {} ({})", opts.name, runner_id);
    println!("  token id:   {token_id}");
    println!("  expires at: {expires_at}");
    match &out {
        Some(path) => {
            println!(
                "  enrollment token: written to {} (owner-only; not printed here)",
                path.display()
            );
        }
        None => {
            let token = resp["enrollment_token"].as_str().unwrap_or("?");
            println!("  enrollment token: {token}");
            println!(
                "  (shown once — copy it into the runner's TACK_RUNNER_ENROLLMENT_TOKEN now; \
                 it cannot be retrieved again)"
            );
        }
    }
    Ok(())
}

fn cmd_runner_revoke(client: &TackClient, runner_id: String, as_json: bool) -> anyhow::Result<()> {
    let resp = client.post(&format!("/runners/{runner_id}/revoke"), &json!({}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!("Revoked runner: {}", &runner_id[..8.min(runner_id.len())]);
    Ok(())
}

fn cmd_runner_revoke_token(
    client: &TackClient,
    runner_id: String,
    token_id: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let resp = client.post(
        &format!("/runners/{runner_id}/enrollment-tokens/{token_id}/revoke"),
        &json!({}),
    )?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    println!(
        "Revoked enrollment token {} for runner {}",
        &token_id[..8.min(token_id.len())],
        &runner_id[..8.min(runner_id.len())]
    );
    Ok(())
}

// ─── Agent profile commands ────────────────────────────────────────────────────

fn cmd_agent_profile_create(
    client: &TackClient,
    name: String,
    instructions: String,
    tool_policy: Option<String>,
    limits: Option<String>,
    fleet: Option<String>,
    as_json: bool,
) -> anyhow::Result<()> {
    let mut body = execution::build_create_agent_profile_body(
        &name,
        &instructions,
        tool_policy.as_deref(),
        limits.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    if let Some(fleet) = fleet {
        body["fleet_id"] = json!(fleet);
    }
    let resp = client.post("/agent-profiles", &body)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    let id = resp["agent_profile_id"].as_str().unwrap_or("?");
    println!(
        "Created agent profile: {} ({})",
        name,
        &id[..8.min(id.len())]
    );
    println!("  id: {id}");
    Ok(())
}

fn cmd_agent_profile_list(client: &TackClient, as_json: bool) -> anyhow::Result<()> {
    let resp = client.get("/agent-profiles")?;
    let list_val = resp.get("data").cloned().unwrap_or(resp);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&list_val)?);
        return Ok(());
    }
    let empty = vec![];
    let rows = list_val.as_array().unwrap_or(&empty);
    if rows.is_empty() {
        println!("No agent profiles found.");
        return Ok(());
    }
    print_table_header(&["ID", "NAME"]);
    for p in rows {
        print_table_row(&[
            short_id(p["agent_profile_id"].as_str()),
            p["name"].as_str().unwrap_or("?"),
        ]);
    }
    Ok(())
}

pub(crate) fn run_execution(client: &TackClient, action: ExecutionAction) -> anyhow::Result<()> {
    match action {
        ExecutionAction::Create { args } => {
            let json = args.json;
            cmd_execution_create(
                client,
                ExecutionCreateOpts {
                    item_id: args.item_id,
                    idempotency_key: args.idempotency_key,
                    runner: args.runner,
                    fleet: args.fleet,
                    agent_profile: args.agent_profile,
                    harness: args.harness,
                    model: args.model,
                    permission_policy: args.permission_policy,
                    budgets: args.budgets,
                    environment: args.environment,
                    metadata: args.metadata,
                    timeout_seconds: args.timeout_seconds,
                    status_map_policy: args.status_map_policy,
                },
                json,
            )
        }
        ExecutionAction::List { json } => cmd_execution_list(client, json),
        ExecutionAction::Get { id, json } => cmd_execution_get(client, id, json),
        ExecutionAction::Cancel { id, json } => cmd_execution_cancel(client, id, json),
        ExecutionAction::Reconcile {
            id,
            recovery_key,
            reason,
            json,
        } => cmd_execution_reconcile(client, id, recovery_key, reason, json),
    }
}

pub(crate) fn run_fleet(client: &TackClient, action: FleetAction) -> anyhow::Result<()> {
    match action {
        FleetAction::Create {
            name,
            concurrency_limit,
            policy,
            json,
        } => cmd_fleet_create(client, name, concurrency_limit, policy, json),
        FleetAction::List { json } => cmd_fleet_list(client, json),
    }
}

pub(crate) fn run_runner(client: &TackClient, action: RunnerAction) -> anyhow::Result<()> {
    match action {
        RunnerAction::Enroll {
            name,
            total_capacity,
            available_capacity,
            labels,
            capability_snapshot,
            protocol_version,
            enrollment_lifetime_secs,
            out,
            json,
        } => cmd_runner_enroll(
            client,
            RunnerEnrollOpts {
                name,
                total_capacity,
                available_capacity,
                labels,
                capability_snapshot,
                protocol_version,
                enrollment_lifetime_secs,
            },
            out,
            json,
        ),
        RunnerAction::Revoke { runner_id, json } => cmd_runner_revoke(client, runner_id, json),
        RunnerAction::RevokeToken {
            runner_id,
            token_id,
            json,
        } => cmd_runner_revoke_token(client, runner_id, token_id, json),
        // Already handled above; unreachable but required for exhaustiveness.
        RunnerAction::Start { .. } => unreachable!(),
        RunnerAction::Doctor { .. } => unreachable!(),
        RunnerAction::Secret { .. } => unreachable!(),
    }
}

pub(crate) fn run_agent_profile(
    client: &TackClient,
    action: AgentProfileAction,
) -> anyhow::Result<()> {
    match action {
        AgentProfileAction::Create {
            name,
            instructions,
            tool_policy,
            limits,
            fleet,
            json,
        } => cmd_agent_profile_create(client, name, instructions, tool_policy, limits, fleet, json),
        AgentProfileAction::List { json } => cmd_agent_profile_list(client, json),
    }
}
