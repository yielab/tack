//! Codex's own claims: its command line and how it reads an exit. The
//! lifecycle around them is proved in `local_process/tests.rs`.

use super::*;
use crate::harness::test_support::{finished, gateway, scratch, secret_store, spec};
use tack_orch::execution::Approvals;

fn endpoint(state: &std::path::Path) -> crate::provider::ProviderEndpoint {
    let secrets = secret_store(state);
    secrets
        .set("key", "a-resolvable-value")
        .expect("seed store");
    let provider = crate::config::VERCEL_AI_GATEWAY_PROVIDER;
    crate::provider::resolve_endpoint(&gateway("key"), &secrets, provider, DESCRIPTOR.wire)
        .expect("resolve")
        .expect("the gateway serves this wire")
}

#[test]
fn a_direct_request_passes_only_the_model() {
    let state = scratch("codex-direct");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let invocation = CodexGrammar.invocation(&run).expect("invocation");
    assert_eq!(
        invocation.args,
        // The fixture request's own tools are `shell`/`filesystem`; `shell`
        // is write-capable, so this lands on `workspace-write`
        // (`the_permission_policy_maps_onto_the_sandbox_flag` covers the
        // rest of the mapping).
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--model",
            "opaque/model-alpha"
        ]
    );
    assert!(invocation.env.is_empty());
}

/// The overrides are global flags: Codex ignores one placed after `exec`.
#[test]
fn a_configured_endpoint_is_named_before_the_subcommand() {
    let state = scratch("codex-endpoint");
    let request = spec(DESCRIPTOR.kind, state.path());
    let endpoint = endpoint(state.path());
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: state.path(),
    };
    let args = CodexGrammar.invocation(&run).expect("invocation").args;

    let exec = args.iter().position(|arg| arg == "exec").expect("exec");
    let overrides: Vec<&String> = args[..exec].iter().filter(|arg| *arg != "-c").collect();
    assert_eq!(overrides.len(), 5, "{args:?}");
    assert_eq!(overrides[0], "model_provider=tack_provider");
    let base_url = format!(
        "model_providers.tack_provider.base_url={:?}",
        endpoint.base_url
    );
    assert!(overrides.contains(&&base_url), "{args:?}");
    assert!(
        overrides
            .contains(&&"model_providers.tack_provider.env_key=\"AI_GATEWAY_API_KEY\"".to_owned())
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains(endpoint.credential.expose()))
    );
}

/// The exit decides, never the output: `exec --json`'s shape is unverified,
/// so output that is not JSON at all still succeeds on exit 0.
#[test]
fn the_exit_status_alone_decides_the_verdict() {
    let rows = [
        (ProcessExit::Exited(0), "fake-harness-ok", true, "completed"),
        (
            ProcessExit::Exited(0),
            "{\"incomplete\": \u{1} not-json }}}",
            true,
            "completed",
        ),
        (ProcessExit::Exited(17), "", false, "exit_code"),
        (ProcessExit::TimedOut, "", false, "timed_out"),
        #[cfg(unix)]
        (ProcessExit::Signaled(9), "", false, "signaled"),
    ];
    let state = scratch("codex-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    for (exit, stdout, succeeded, code) in rows {
        let report = CodexGrammar.report(&run, &finished(exit, stdout));
        assert_eq!(report.succeeded, succeeded, "{exit:?}");
        assert_eq!(report.terminal_reason["code"], code, "{exit:?}");
        assert_eq!(report.terminal_reason["stdout"]["text_preview"], stdout);
        assert_eq!(
            (report.observed_model, report.tokens_in, report.cost_usd),
            (None, None, None)
        );
    }
    let failed = CodexGrammar.report(&run, &finished(ProcessExit::Exited(17), ""));
    assert!(
        failed.terminal_reason["message"]
            .as_str()
            .expect("message")
            .contains("17")
    );
}

#[test]
fn codex_declares_what_it_does_not_enforce() {
    let declared = CodexGrammar.capabilities();
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert_eq!(declared.usage.support, CapabilitySupport::Advisory);
    assert_eq!(declared.decisions.support, CapabilitySupport::Unsupported);
    assert_eq!(
        declared.additional["permission_policy"]["support"],
        "advisory"
    );
}

/// One row per policy value the mapping cares about: the tool list alone
/// selects the sandbox mode; `approvals` and `network` never change the
/// args (each is compared against the same expected args as the first row,
/// which fixes `tools: []`, `network: false` and `approvals: None`).
#[test]
fn the_permission_policy_maps_onto_the_sandbox_flag() {
    let state = scratch("codex-permission-policy");
    let rows: [(&[&str], bool, Option<Approvals>, &str); 6] = [
        (&[], false, None, "read-only"),
        (&["bash"], false, None, "workspace-write"),
        (&["Edit"], false, None, "workspace-write"),
        (&["read"], false, None, "read-only"),
        (&[], false, Some(Approvals::Auto), "read-only"),
        (&[], true, None, "read-only"),
    ];
    for (tools, network, approvals, sandbox) in rows {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.permission_policy.tools =
            tools.iter().map(|tool| (*tool).to_owned()).collect();
        request.work.request.permission_policy.network = network;
        request.work.request.permission_policy.approvals = approvals;
        let run = RunContext {
            spec: &request,
            endpoint: None,
            scratch: state.path(),
        };
        let args = CodexGrammar.invocation(&run).expect("invocation").args;
        assert_eq!(
            args,
            [
                "exec",
                "--json",
                "--sandbox",
                sandbox,
                "--model",
                "opaque/model-alpha"
            ],
            "{tools:?} network={network} approvals={approvals:?}"
        );
    }
}

/// `(transcript, succeeded, tokens_in, tokens_out)`. Numbers for the real
/// capture are re-measured straight off its own `turn.completed` line
/// (`grep turn.completed exec-tool-call.jsonl`), not trusted from a brief.
/// `no_turn_completed` is the same capture with that line stripped,
/// standing in for a transcript that never reaches one.
#[test]
fn usage_is_read_from_the_turn_completed_line() {
    let exec_tool_call = include_str!("../fixtures/codex/0.149.1/exec-tool-call.jsonl");
    let no_turn_completed: String = exec_tool_call
        .lines()
        .filter(|line| !line.contains("turn.completed"))
        .collect::<Vec<_>>()
        .join("\n");
    let state = scratch("codex-usage");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let rows = [
        (exec_tool_call, true, Some(57), Some(10)),
        (no_turn_completed.as_str(), true, None, None),
    ];
    for (transcript, succeeded, tokens_in, tokens_out) in rows {
        let report = CodexGrammar.report(&run, &finished(ProcessExit::Exited(0), transcript));
        assert_eq!(report.succeeded, succeeded, "{transcript:?}");
        assert_eq!(report.tokens_in, tokens_in, "{transcript:?}");
        assert_eq!(report.tokens_out, tokens_out, "{transcript:?}");
        assert_eq!(report.observed_model, None, "{transcript:?}");
        assert_eq!(report.cost_usd, None, "{transcript:?}");
    }
}
