//! Shared by every harness test module: a request built from the contract's
//! claim fixture, scratch directories, and locators for the fake harness.

use std::{
    collections::BTreeMap,
    path::Path,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use tack_orch::execution::{
    AttemptSnapshot, EnvironmentValue, ExecutionRequestSnapshot, HarnessKind,
};

use crate::client::{
    AttemptId, AttemptLease, AttemptState, ClaimRequestId, ClaimedWork, FencingToken, RunnerId,
    Timestamp, Workspace, WorkspaceId,
};
use crate::harness::{
    ExecutionSpec,
    local_process::{BinaryLocator, HarnessGrammar, LocalProcessHarness},
    process::{CapturedOutput, ProcessExit, ProcessLimits, ProcessResult},
};
use crate::secrets::SecretStore;

#[derive(Clone, Copy)]
pub struct FixedClock(pub SystemTime);

impl crate::Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

pub fn clock() -> FixedClock {
    let at = chrono::DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z").expect("timestamp");
    FixedClock(at.into())
}

/// Removes itself and everything under it when dropped, including when an
/// assertion panics first. Whatever holds a path into it holds the guard.
pub fn scratch(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// File-backed, never the platform keychain, so parallel tests share
/// nothing and CI needs no Secret Service.
pub fn secret_store(dir: &Path) -> SecretStore {
    SecretStore::file(dir.join("secrets.json"))
}

pub fn fake_harness() -> BinaryLocator {
    let (program, prefix_args) = crate::harness::fixtures::fake_harness_command();
    BinaryLocator::Fixed {
        program,
        prefix_args,
    }
}

/// A `/bin/sh` script written into `dir`, standing in for a harness binary.
pub fn script(dir: &Path, body: &str) -> BinaryLocator {
    let path = dir.join("shim.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write shim");
    BinaryLocator::Fixed {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![path.display().to_string()],
    }
}

/// The contract's own claimed request, retargeted at `kind` and `workspace`.
/// It asks for `openai` / `opaque/model-alpha`, tools `shell`/`filesystem`,
/// network denied.
pub fn spec(kind: &str, workspace: &Path) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = HarnessKind::new(kind);
    request.timeout_seconds = 30;
    let attempt: AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
    ExecutionSpec {
        work: ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-09T11:59:00Z"),
                expires_at: Timestamp::new("2026-08-09T12:59:00Z"),
            },
            request,
            attempt,
        },
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt"),
            id: WorkspaceId::new("ws_test"),
            path: workspace.to_path_buf(),
            base_revision: "revision".into(),
        },
    }
}

pub fn set_env(spec: &mut ExecutionSpec, pairs: &[(&str, &str)]) {
    for (name, value) in pairs {
        spec.work.request.environment.insert(
            (*name).to_owned(),
            EnvironmentValue {
                value: Some((*value).to_owned()),
                secret_reference: None,
                additional: BTreeMap::new(),
            },
        );
    }
}

pub fn set_secret_reference(spec: &mut ExecutionSpec, name: &str, reference: &str) {
    spec.work.request.environment.insert(
        name.to_owned(),
        EnvironmentValue {
            value: None,
            secret_reference: Some(reference.to_owned()),
            additional: BTreeMap::new(),
        },
    );
}

/// The Vercel gateway, enabled, its key stored under `secret_name`.
pub fn gateway(secret_name: &str) -> BTreeMap<String, crate::config::ProviderConfig> {
    BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: secret_name.to_owned(),
        },
    )])
}

pub fn finished(exit: ProcessExit, stdout: &str) -> ProcessResult {
    ProcessResult {
        exit,
        stdout: CapturedOutput {
            text: stdout.to_owned(),
            ..CapturedOutput::default()
        },
        stderr: CapturedOutput::default(),
    }
}

pub fn limits() -> ProcessLimits {
    ProcessLimits {
        termination_grace: Duration::from_millis(150),
        ..ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
    }
}

/// Builds a harness around `grammar`, for driving a real grammar through
/// the core against a shim script (see [`script`]).
pub fn harness_for<G: HarnessGrammar>(
    grammar: G,
    locator: BinaryLocator,
    state: &Path,
) -> LocalProcessHarness<G, FixedClock> {
    LocalProcessHarness::new(
        grammar,
        locator,
        clock(),
        limits(),
        state.join("staging"),
        secret_store(state),
    )
}
