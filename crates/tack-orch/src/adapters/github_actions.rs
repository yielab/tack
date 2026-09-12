//! `GithubActionsAdapter` — a **compile-only stub** for a second
//! [`ControlPlane`](crate::ControlPlane) implementor.
//!
//! Its only job is to make "both adapters compile against the trait" a fact
//! CI can check, ahead of wiring the real thing. Every method but
//! `kind` and
//! `capabilities` is `unimplemented!()`
//! — there is no HTTP client, no request/response handling, nothing that
//! could plausibly talk to a real GitHub Actions instance. Don't add any
//! until the real adapter is actually being wired.
//!
//! **Not registered.** `adapters::registry::build` has no `"github-actions"`
//! match arm — see that function's own doc comment for why: registering an
//! adapter whose methods all panic would let an operator create a control
//! plane that blows up the first time the reconciler polls it.
//!
//! `capabilities` is filled in for
//! real, though, and deliberately so: it's the one method this stub can
//! answer honestly without making a network call, and having it right now
//! lets `docs/plans/agnostic-control-plane.md` §II.1.3's `RunState`
//! normalization table and the capability-gated UI be designed and tested
//! against GitHub Actions' real shape before a single line of
//! HTTP-calling code exists. Every value below is checked against GitHub's
//! REST API documentation (`docs/plans/agnostic-control-plane.md` §II.1.4,
//! "Verified external facts") — not guessed from what a docket-shaped
//! provider would need.

use async_trait::async_trait;

use crate::{
    ApprovalState, Capabilities, ControlPlane, DecisionSupport, EventScope, FleetStatus, Health,
    MetricSample, ModelSelection, NewRemoteTask, OrchError, ProvisionPodParams, ProvisionedPod,
    Rated, RemoteApproval, RemoteRun, RemoteTask, Support, TracesPage, UsageSupport,
};

/// See the module doc — compile-only, not registered, not usable.
pub struct GithubActionsAdapter;

#[async_trait]
impl ControlPlane for GithubActionsAdapter {
    fn kind(&self) -> &'static str {
        "github-actions"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // `POST .../actions/workflows/{id}/dispatches` (§II.1.4).
            dispatch: true,
            // `POST .../runs/{id}/cancel` -> `202` (§II.1.4).
            cancel: true,
            pause: Rated::new(
                Support::Unsupported,
                "the GitHub REST API has no pause endpoint for a workflow run",
            ),
            resume: Rated::new(
                Support::Unsupported,
                "the GitHub REST API has no resume endpoint for a workflow run",
            ),
            event_scope: Rated::new(
                EventScope::Run,
                "events are derived from GET .../runs/{id}/jobs step data, one run at a \
                 time; GitHub has no project- or plane-wide event stream, and run logs are \
                 a 302 redirect that expires in 1 minute, not a stream",
            ),
            // `GET .../actions/runs/{id}/artifacts`; retention is bounded
            // (default 90 days) but the capability itself is real.
            artifacts: true,
            decisions: Rated::new(
                DecisionSupport::Poll,
                "pending deployment gates are read via GET .../runs/{id}/pending_deployments \
                 on the reconciler's poll cadence; there is no push/webhook path for a new \
                 gate opening",
            ),
            usage: Rated::new(
                UsageSupport::NotMeasured,
                "GitHub Actions reports runner minutes, not model/token usage; no usage \
                 metering exists for this provider",
            ),
            model_selection: Rated::new(
                ModelSelection::Honoured,
                "a dispatched workflow receives its inputs verbatim; this adapter does not \
                 intercept or reinterpret a model identifier passed as one",
            ),
            // A workflow file names the runtimes a dispatch can target.
            runtimes: true,
            plane_metrics: false,
            // No "create me a fresh execution environment" call exists — a
            // workflow runs against infrastructure that already exists.
            provisioning: false,
        }
    }

    async fn health(&self) -> Result<Health, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn status(&self) -> Result<FleetStatus, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn traces(&self, _project: &str, _since: Option<&str>) -> Result<TracesPage, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn enqueue_task(
        &self,
        _project: &str,
        _task: NewRemoteTask,
    ) -> Result<String, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn dispatch(
        &self,
        _project: &str,
        _vars: serde_json::Value,
    ) -> Result<String, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn decide_approval(
        &self,
        _token: &str,
        _grant: bool,
    ) -> Result<ApprovalState, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }

    async fn provision_pod(
        &self,
        _params: ProvisionPodParams,
    ) -> Result<ProvisionedPod, OrchError> {
        unimplemented!("adapters::github_actions is a compile-only stub — see the module doc")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_is_github_actions() {
        assert_eq!(GithubActionsAdapter.kind(), "github-actions");
    }

    #[test]
    fn github_actions_capabilities_are_declared() {
        let caps = GithubActionsAdapter.capabilities();
        assert_eq!(
            caps.pause.level,
            Support::Unsupported,
            "no pause endpoint exists anywhere in the GitHub REST API"
        );
        assert_eq!(caps.resume.level, Support::Unsupported);
        assert_eq!(
            caps.event_scope.level,
            EventScope::Run,
            "GitHub Actions events are derived per run, from job step data"
        );
        assert!(caps.cancel, "POST .../runs/{{id}}/cancel is real");
        assert!(!caps.plane_metrics, "no plane-wide scrape exists");
        assert_eq!(caps.usage.level, UsageSupport::NotMeasured);
        assert_eq!(caps.decisions.level, DecisionSupport::Poll);
        assert_eq!(caps.model_selection.level, ModelSelection::Honoured);
        assert!(caps.runtimes);
        assert!(!caps.provisioning);
    }

    #[tokio::test]
    #[should_panic(expected = "compile-only stub")]
    async fn every_other_method_is_unimplemented() {
        // One representative panic check, not thirteen — the point is that
        // this adapter cannot be mistaken for a working one if something
        // ever did call it directly; `adapters::registry::build` (tested in
        // its own module) is what actually keeps it unreachable in
        // practice.
        let adapter = GithubActionsAdapter;
        let _ = adapter.health().await;
    }
}
