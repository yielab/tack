//! One place every caller builds a live [`ControlPlane`](crate::ControlPlane)
//! from a `control_planes` row's `kind`, replacing four drifted copy-pasted
//! match sites. Lives in `tack-orch`, not `tack-api` (which cannot depend on
//! it, and every caller already depends on `tack-orch`).

use std::sync::Arc;

use crate::adapters::docket::DocketAdapter;
use crate::{ControlPlane, OrchError};

/// Why [`build`] could not hand back a live adapter — two shapes because a
/// caller's response differs: an unrecognised `kind` is a config mistake
/// before construction; `Construction` is a known `kind`'s own ctor failing.
#[derive(Debug)]
pub enum RegistryError {
    /// `control_planes.kind` names an unimplemented (or stub-only) provider.
    UnknownKind(String),
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
/// `config`/`secrets` (migrations 032/033) are threaded through for a future
/// provider that needs them; no `kind` registered below reads them today.
/// `"github-actions"` is deliberately not a match arm: its adapter is a
/// compile-only stub whose methods `unimplemented!()`, so registering it
/// would panic the reconciler instead of returning `UnknownKind`.
pub fn build(
    kind: &str,
    base_url: &str,
    token: Option<String>,
    config: &serde_json::Value,
    secrets: Option<&serde_json::Value>,
) -> Result<Arc<dyn ControlPlane>, RegistryError> {
    match kind {
        "docket" => {
            // Docket needs neither parameter today — see this fn's doc.
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
        // so an operator can select it — see `build`'s doc comment.
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
