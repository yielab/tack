//! `GithubActionsAdapter` — a **compile-only stub** for a second
//! [`ControlPlane`] implementor (TODO.md card G1, Wave B).
//!
//! Its only job this wave is to make "both adapters compile against the
//! trait" a fact CI can check today, rather than a discovery Wave D makes
//! when it starts wiring the real thing. Every method but
//! [`kind`](GithubActionsAdapter::kind) and
//! [`capabilities`](GithubActionsAdapter::capabilities) is `unimplemented!()`
//! — there is no HTTP client, no request/response handling, nothing that
//! could plausibly talk to a real GitHub Actions instance. Don't add any;
//! that work belongs to Wave D (see `docs/plans/agnostic-control-plane.md`
//! §4 Phase 6, card N2), which also owns this file from that point on (see
//! TODO.md's file-ownership map).
//!
//! **Not registered.** `adapters::registry::build` has no `"github-actions"`
//! match arm — see that function's own doc comment for why: registering an
//! adapter whose methods all panic would let an operator create a control
//! plane that blows up the first time the reconciler polls it.
//!
//! [`capabilities`](GithubActionsAdapter::capabilities) is filled in for
//! real, though, and deliberately so: it's the one method this stub can
//! answer honestly without making a network call, and having it right now
//! lets `docs/plans/agnostic-control-plane.md` §II.1.3's `RunState`
//! normalization table and the capability-gated UI (card G1's frontend
//! follow-up) be designed and tested against GitHub Actions' real shape
//! before a single line of HTTP-calling code exists. Every value below is
//! checked against GitHub's REST API documentation (`docs/plans/
//! agnostic-control-plane.md` §II.1.4, "Verified external facts") — not
//! guessed from what a docket-shaped provider would need.

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
            // `POST .../actions/workflows/{id}/dispatches` — a real,
            // documented endpoint (§II.1.4).
            dispatch: true,
            // `POST .../runs/{id}/cancel` -> `202` (§II.1.4).
            cancel: true,
            // No pause/suspend/hold endpoint exists anywhere in the GitHub
            // REST API for a workflow run — not a gap in this adapter, a
            // gap in the provider (§II.1.4: "no endpoint exists").
            pause: Rated::new(
                Support::Unsupported,
                "the GitHub REST API has no pause endpoint for a workflow run",
            ),
            resume: Rated::new(
                Support::Unsupported,
                "the GitHub REST API has no resume endpoint for a workflow run",
            ),
            // Derived from `GET .../runs/{id}/jobs` step data, one run at a
            // time — run logs (`GET .../runs/{id}/logs`) are a 302 whose
            // link expires in 1 minute, so they cannot be treated as an
            // event stream (§II.1.4). Unlike docket, GitHub has no
            // project-wide or plane-wide event feed at all.
            event_scope: Rated::new(
                EventScope::Run,
                "events are derived from GET .../runs/{id}/jobs step data, one run at a \
                 time; GitHub has no project- or plane-wide event stream, and run logs are \
                 a 302 redirect that expires in 1 minute, not a stream",
            ),
            // `GET .../actions/runs/{id}/artifacts` is a real, documented
            // endpoint; retention is bounded (default 90 days) but the
            // capability itself is real.
            artifacts: true,
            // `GET`/`POST .../runs/{id}/pending_deployments` is GitHub
            // Actions' decision store (§II.1.4) — read on the reconciler's
            // poll cadence, same as docket's approvals; GitHub has no push
            // mechanism for a newly pending deployment gate either.
            decisions: Rated::new(
                DecisionSupport::Poll,
                "pending deployment gates are read via GET .../runs/{id}/pending_deployments \
                 on the reconciler's poll cadence; there is no push/webhook path for a new \
                 gate opening",
            ),
            // GitHub Actions bills and reports runner minutes, never model
            // or token usage — there is no usage figure to report here at
            // all (TODO.md §II.0 rule 7: two meters are never one number).
            usage: Rated::new(
                UsageSupport::NotMeasured,
                "GitHub Actions reports runner minutes, not model/token usage; no usage \
                 metering exists for this provider",
            ),
            // A dispatched workflow receives its inputs (including a model
            // identifier, if the workflow defines one) verbatim — this
            // adapter does not intercept or reinterpret them, and GitHub
            // itself has no routing layer to override them.
            model_selection: Rated::new(
                ModelSelection::Honoured,
                "a dispatched workflow receives its inputs verbatim; this adapter does not \
                 intercept or reinterpret a model identifier passed as one",
            ),
            // A workflow file names the runtimes (self-hosted labels,
            // GitHub-hosted OS images) a dispatch can target.
            runtimes: true,
            // No plane-wide scrape exists — GitHub Actions has nothing
            // resembling docket's `/metrics` (§II.1.4).
            plane_metrics: false,
            // No provisioning primitive exists — a workflow runs against
            // whatever runner infrastructure already exists (GitHub-hosted,
            // or a self-hosted runner registered out of band); there is no
            // "create me a fresh execution environment" call.
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
