//! One place every caller builds a live [`ControlPlane`] from a
//! `control_planes` row's `kind` — replacing four copy-pasted
//! `match row.kind.as_str()` sites that had drifted into four
//! near-identical, hand-maintained match arms:
//! `tack-api::orch_store::RepoControlPlaneStore::list_registered`,
//! `tack-api::dispatcher::build_control_plane`,
//! `tack-api::handlers::orch::build_control_plane_for_decision`, and
//! `tack-api::handlers::provisioning::resolve_control_plane`. Two of those
//! sites carried a comment arguing the duplication was deliberate ("if a
//! third caller ever needs this, that's the point to actually share it") —
//! there were already four, not two, so that argument had already lost;
//! this module is the sharing.
//!
//! Lives in `tack-orch`, not `tack-api`, because `crates/tack-orch/
//! Cargo.toml` forbids the reverse dependency (see this crate's own module
//! doc) and every one of those four callers already depends on `tack-orch`.
//!
//! **Callers keep their own failure behaviour.** This module only builds
//! the adapter and classifies *why* it failed ([`RegistryError::UnknownKind`]
//! vs. [`RegistryError::Construction`]); it deliberately does not decide
//! whether that failure aborts a caller's request or is logged and skipped.
//! `orch_store::RepoControlPlaneStore::list_registered` (a reconciler batch
//! loop covering every registered plane) and the three request-scoped HTTP
//! handlers (each acting on one plane a caller named explicitly) need
//! opposite answers to that question — folding the decision in here would
//! force one of them to be wrong. See TODO.md card G1.

use std::sync::Arc;

use crate::adapters::docket::DocketAdapter;
use crate::{ControlPlane, OrchError};

/// Why [`build`] could not hand back a live adapter. Two shapes, not one
/// generic error, because a caller's correct response to each differs: an
/// unrecognised `kind` is a configuration mistake made before any
/// construction was attempted; [`RegistryError::Construction`] is a known
/// `kind`'s own constructor failing (for `"docket"` today, only ever the
/// fallible `reqwest::Client` build inside `DocketAdapter::new` — see that
/// function's own doc comment for how rare that is in practice). Neither
/// variant means an HTTP call was made or attempted.
#[derive(Debug)]
pub enum RegistryError {
    /// `control_planes.kind` names a provider this build of Tack does not
    /// implement (or implements only as a not-yet-registered stub — see
    /// [`build`]'s doc comment on `"github-actions"`). Carries the raw
    /// string so a caller can name it in its own error message without
    /// re-deriving it from the original row.
    UnknownKind(String),
    /// A recognised `kind`'s own adapter constructor returned `Err`.
    Construction(OrchError),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::UnknownKind(kind) => {
                write!(f, "unsupported control-plane kind {kind:?}")
            }
            RegistryError::Construction(e) => {
                write!(f, "failed to construct control-plane adapter: {e}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// Build a live [`ControlPlane`] for one `control_planes` row.
///
/// `config`/`secrets` are the JSON blobs migrations 032/033 added to
/// `control_planes` — threaded through this signature so it never has to be
/// defined twice, but **not read by any `kind` registered below today**:
/// `"docket"` only ever needed `base_url` plus the single `token` column
/// that predates both migrations, so this function's one match arm ignores
/// them. They exist here for the provider that *does* need them — a GitHub
/// Actions plane's `{owner, repo, workflow_file, ref, api_base}` config and
/// its PAT/webhook-secret pair — once `adapters::github_actions` graduates
/// from the compile-only stub it is today.
///
/// `"github-actions"` is deliberately **not** a match arm below.
/// [`crate::adapters::github_actions::GithubActionsAdapter`] is a
/// compile-only stub — every method but `kind`/`capabilities` is
/// `unimplemented!()` — so registering it here would let an operator create
/// a control plane that panics the first time the reconciler polls it,
/// rather than the honest [`RegistryError::UnknownKind`] this function
/// returns for any other name it doesn't recognise.
pub fn build(
    kind: &str,
    base_url: &str,
    token: Option<String>,
    config: &serde_json::Value,
    secrets: Option<&serde_json::Value>,
) -> Result<Arc<dyn ControlPlane>, RegistryError> {
    match kind {
        "docket" => {
            // See this function's own doc comment: docket needs neither
            // parameter today. Named, not `_`-prefixed at the call site, so
            // the signature stays self-documenting for whoever adds the
            // next `kind` and actually reads one of these.
            let _ = (config, secrets);
            DocketAdapter::new(base_url, token)
                .map(|adapter| Arc::new(adapter) as Arc<dyn ControlPlane>)
                .map_err(RegistryError::Construction)
        }
        other => Err(RegistryError::UnknownKind(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docket_kind_builds_a_live_adapter() {
        let result = build(
            "docket",
            "http://127.0.0.1:7331",
            Some("tok".to_string()),
            &serde_json::json!({}),
            None,
        );
        assert!(result.is_ok(), "docket must always construct");
        assert_eq!(result.unwrap().kind(), "docket");
    }

    #[test]
    fn docket_kind_ignores_config_and_secrets() {
        // See `build`'s own doc comment — these are threaded through for a
        // future provider, not read by "docket" today. Passing a
        // non-trivial value for both must not change the outcome.
        let result = build(
            "docket",
            "http://127.0.0.1:7331",
            None,
            &serde_json::json!({"owner": "example", "repo": "demo"}),
            Some(&serde_json::json!({"pat": "should-be-ignored"})),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_kind_is_a_typed_error_not_a_panic() {
        let result = build(
            "some-future-thing",
            "http://127.0.0.1:9999",
            None,
            &serde_json::json!({}),
            None,
        );
        match result {
            Err(RegistryError::UnknownKind(kind)) => assert_eq!(kind, "some-future-thing"),
            Ok(_) => panic!("expected RegistryError::UnknownKind, got Ok"),
            Err(other) => panic!("expected RegistryError::UnknownKind, got {other}"),
        }
    }

    #[test]
    fn github_actions_is_not_registered() {
        // The stub exists so both adapters compile against the trait, not
        // so an operator can select it — see `build`'s doc comment on why
        // this is deliberate, not an oversight to fill in later in this
        // card.
        let result = build(
            "github-actions",
            "https://api.github.com",
            None,
            &serde_json::json!({}),
            None,
        );
        assert!(matches!(result, Err(RegistryError::UnknownKind(_))));
    }

    #[test]
    fn registry_error_display_names_the_problem() {
        let unknown = RegistryError::UnknownKind("mystery".to_string());
        assert!(unknown.to_string().contains("mystery"));

        let construction = RegistryError::Construction(OrchError::Http("boom".to_string()));
        assert!(construction.to_string().contains("boom"));
    }
}
