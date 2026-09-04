//! Runner-side provider endpoints: where a harness sends requests and which
//! credential authenticates them, when that differs from the harness's own
//! ambient login.
//!
//! Every harness this crate drives already has a working, credential-free
//! mode: the harness's own subscription/login (Claude Max, a ChatGPT plan,
//! `codex login`, ...). That mode needs nothing from this module — it is
//! simply the absence of a configured entry for the request's provider, and
//! every function here treats it as `None`/a typed absence, never a second
//! implicit case to branch on.
//!
//! What this module adds is the other mode: `RunnerConfig::providers` names
//! an entry (`[provider.<name>]`), and [`resolve_endpoint`] tells a harness
//! adapter what to inject when a request's provider matches one — a base
//! URL, the name of the environment variable that must carry the
//! credential, and the resolved credential itself. A gateway (Vercel AI
//! Gateway) and a vendor's own direct API are the same shape here — a base
//! URL plus a bearer credential — so a second entry, gateway or direct, is
//! a new row in [`known_endpoint`], never a new mechanism or a branch
//! inside an adapter.
//!
//! One entry exists today: `vercel_ai_gateway` (ADR 0061 decision 4).

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tack_orch::execution::{ModelCombination, ModelId, ModelProvider, RunnerCapabilities};

use crate::Clock;
use crate::config::{ProviderConfig, VERCEL_AI_GATEWAY_CONFIG_KEY, VERCEL_AI_GATEWAY_PROVIDER};
use crate::secrets::{SecretStore, SecretValue};

/// The provider's model-list endpoint (ADR 0061 decision 3).
const CATALOG_URL: &str = "https://ai-gateway.vercel.sh/v1/models";
const CATALOG_TIMEOUT: Duration = Duration::from_secs(10);

/// Recorded in `ModelCombination::discovery` for a catalog-sourced entry,
/// distinct from `"reported"` — a vendor's published list, not something
/// this runner measured while actually running a task (ADR 0061 decision
/// 3).
pub const CATALOG_DISCOVERY: &str = "catalog_reported";

/// Harness kinds whose adapters apply a [`ProviderEndpoint`]. `opencode` is
/// deliberately absent: reaching a configured endpoint there needs a
/// project-local config file written into the workspace plus an npm
/// package the harness loads at startup — a materially different mechanism
/// this crate does not implement.
const CATALOG_ELIGIBLE_HARNESSES: [&str; 2] = ["claude-code", "codex"];

/// The wire shape a harness adapter already speaks. Selects which
/// [`known_endpoint`] row applies and how the adapter injects it —
/// environment variables for an Anthropic-Messages CLI, invocation flags
/// for an OpenAI-Responses one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    AnthropicMessages,
    OpenAiResponses,
}

/// What an adapter must inject to point a spawn at a configured provider
/// endpoint instead of the harness's own default: where to send requests,
/// which environment variable carries the credential, its resolved value,
/// and a display label for a harness that must declare the provider under
/// a name (codex's `-c model_providers.<key>.name`).
#[derive(Debug)]
pub struct ProviderEndpoint {
    pub base_url: String,
    pub credential_env_var: String,
    pub credential: SecretValue,
    pub display_name: String,
}

/// Fixed, non-configurable facts about one provider's endpoint for one
/// wire — vendor data, never user configuration. `enabled`/`secret`
/// ([`ProviderConfig`]) are the only two knobs a `[provider.<name>]` table
/// exposes; a base URL or a credential env-var name is not one of them.
struct KnownEndpoint {
    base_url: &'static str,
    credential_env_var: &'static str,
    display_name: &'static str,
}

fn known_endpoint(provider: &str, wire: Wire) -> Option<KnownEndpoint> {
    match (provider, wire) {
        (VERCEL_AI_GATEWAY_PROVIDER, Wire::AnthropicMessages) => Some(KnownEndpoint {
            // No `/v1` suffix: the CLI appends `/v1/messages` itself, and a
            // double suffix 404s.
            base_url: "https://ai-gateway.vercel.sh/claude-code",
            credential_env_var: "ANTHROPIC_AUTH_TOKEN",
            display_name: "Vercel AI Gateway",
        }),
        (VERCEL_AI_GATEWAY_PROVIDER, Wire::OpenAiResponses) => Some(KnownEndpoint {
            base_url: "https://ai-gateway.vercel.sh/codex/v1",
            credential_env_var: "AI_GATEWAY_API_KEY",
            display_name: "Vercel AI Gateway",
        }),
        _ => None,
    }
}

/// Why `requested_provider` named a known endpoint but it could not be
/// resolved into a working [`ProviderEndpoint`]. Every variant names a
/// fact, never a secret value — safe inside a `HarnessError::Rejected`
/// reason or a log line.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider {0:?} has no enabled [provider.<name>] entry")]
    NotConfigured(String),
    #[error("provider {0:?} secret: {1}")]
    Secret(String, crate::secrets::SecretError),
}

/// Resolves what `requested_provider` needs injected for `wire`, or `None`
/// when `requested_provider` names no known endpoint for that wire at all —
/// the harness's own subscription/login mode, where an adapter must inject
/// nothing.
pub fn resolve_endpoint(
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    requested_provider: &str,
    wire: Wire,
) -> Result<Option<ProviderEndpoint>, ProviderError> {
    let Some(known) = known_endpoint(requested_provider, wire) else {
        return Ok(None);
    };
    let config_key = match requested_provider {
        VERCEL_AI_GATEWAY_PROVIDER => VERCEL_AI_GATEWAY_CONFIG_KEY,
        other => other,
    };
    let config = providers
        .get(config_key)
        .filter(|config| config.enabled)
        .ok_or_else(|| ProviderError::NotConfigured(requested_provider.to_owned()))?;
    let credential = secrets
        .resolve(&config.secret)
        .map_err(|error| ProviderError::Secret(requested_provider.to_owned(), error))?;
    tracing::debug!(
        provider = requested_provider,
        secret = %config.secret,
        "provider endpoint resolved for spawn injection"
    );
    Ok(Some(ProviderEndpoint {
        base_url: known.base_url.to_owned(),
        credential_env_var: known.credential_env_var.to_owned(),
        credential,
        display_name: known.display_name.to_owned(),
    }))
}

/// What asking a configured provider for its model catalog produced — a
/// typed absence for every non-catalog outcome, matching
/// `bootstrap::build_adapter_registry`'s own posture toward a harness that
/// cannot be discovered: never a stale or placeholder list.
#[derive(Debug, Clone)]
pub enum CatalogStatus {
    /// No enabled `[provider.<name>]` entry exists for the one provider
    /// this runner knows how to ask.
    NotConfigured,
    /// The entry is enabled but its secret does not resolve.
    SecretUnresolved,
    /// The catalog endpoint answered a non-success status, or could not be
    /// reached at all (`status: None`).
    Unreachable { status: Option<u16> },
    /// The catalog request succeeded.
    Configured {
        model_count: usize,
        checked_at: DateTime<Utc>,
    },
}

/// Fetches the configured provider's model catalog and, on success, records
/// one [`ModelCombination`] per catalog-eligible harness already present in
/// `capabilities.harnesses` — never inventing an entry for a harness this
/// machine did not probe. Called from both `bootstrap::build_runtime` and
/// `bootstrap::probe`, so a real enrollment/refresh snapshot and a
/// `tack runner doctor --json` run share this one code path rather than two
/// that could quietly diverge.
pub async fn attach_catalog<C: Clock>(
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> CatalogStatus {
    let Some(config) = providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .filter(|config| config.enabled)
    else {
        return CatalogStatus::NotConfigured;
    };
    let secret = match secrets.resolve(&config.secret) {
        Ok(secret) => secret,
        Err(_) => return CatalogStatus::SecretUnresolved,
    };
    tracing::debug!(secret = %config.secret, "provider secret resolved for catalog fetch");
    let model_ids = match fetch_catalog_ids(&secret).await {
        Ok(ids) => ids,
        Err(CatalogFetchError::Status(status)) => {
            tracing::warn!(status, "provider catalog fetch rejected");
            return CatalogStatus::Unreachable {
                status: Some(status),
            };
        }
        Err(CatalogFetchError::Transport) => {
            tracing::warn!("provider catalog fetch failed: transport error");
            return CatalogStatus::Unreachable { status: None };
        }
    };
    tracing::debug!(model_count = model_ids.len(), "provider catalog fetched");

    let checked_at = DateTime::<Utc>::from(clock.now());
    for harness in capabilities.harnesses.iter_mut() {
        if CATALOG_ELIGIBLE_HARNESSES.contains(&harness.harness_kind.as_str()) {
            harness.model_combinations.push(ModelCombination {
                model_provider: ModelProvider::new(VERCEL_AI_GATEWAY_PROVIDER),
                model_ids: model_ids.iter().cloned().map(ModelId::new).collect(),
                discovery: CATALOG_DISCOVERY.to_owned(),
                additional: Default::default(),
            });
        }
    }
    CatalogStatus::Configured {
        model_count: model_ids.len(),
        checked_at,
    }
}

enum CatalogFetchError {
    Transport,
    Status(u16),
}

#[derive(serde::Deserialize)]
struct CatalogModel {
    id: String,
}

#[derive(serde::Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    data: Vec<CatalogModel>,
}

async fn fetch_catalog_ids(secret: &SecretValue) -> Result<Vec<String>, CatalogFetchError> {
    let client = reqwest::Client::builder()
        .timeout(CATALOG_TIMEOUT)
        .user_agent(concat!("tack-runner/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| CatalogFetchError::Transport)?;
    let response = client
        .get(CATALOG_URL)
        .bearer_auth(secret.expose())
        .send()
        .await
        .map_err(|_| CatalogFetchError::Transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(CatalogFetchError::Status(status.as_u16()));
    }
    let parsed: CatalogResponse = response
        .json()
        .await
        .map_err(|_| CatalogFetchError::Status(status.as_u16()))?;
    Ok(parsed.data.into_iter().map(|model| model.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn providers(enabled: bool, secret: &str) -> BTreeMap<String, ProviderConfig> {
        BTreeMap::from([(
            VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
            ProviderConfig {
                enabled,
                secret: secret.to_owned(),
            },
        )])
    }

    #[test]
    fn a_direct_vendor_provider_resolves_to_no_endpoint_at_all() {
        let secrets = SecretStore::file(std::env::temp_dir().join("tack-provider-test-direct"));
        let result = resolve_endpoint(
            &providers(true, "demo"),
            &secrets,
            "anthropic",
            Wire::AnthropicMessages,
        );
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn a_disabled_provider_is_a_typed_not_configured_error() {
        let secrets = SecretStore::file(std::env::temp_dir().join("tack-provider-test-disabled"));
        let result = resolve_endpoint(
            &providers(false, "demo"),
            &secrets,
            VERCEL_AI_GATEWAY_PROVIDER,
            Wire::AnthropicMessages,
        );
        assert!(
            matches!(result, Err(ProviderError::NotConfigured(name)) if name == VERCEL_AI_GATEWAY_PROVIDER)
        );
    }

    #[test]
    fn an_enabled_provider_with_no_such_secret_is_a_typed_secret_error() {
        let secrets =
            SecretStore::file(std::env::temp_dir().join("tack-provider-test-missing-secret"));
        let result = resolve_endpoint(
            &providers(true, "does-not-exist"),
            &secrets,
            VERCEL_AI_GATEWAY_PROVIDER,
            Wire::AnthropicMessages,
        );
        assert!(
            matches!(result, Err(ProviderError::Secret(name, _)) if name == VERCEL_AI_GATEWAY_PROVIDER)
        );
    }

    #[test]
    fn a_configured_provider_resolves_the_wire_specific_endpoint() {
        let path = std::env::temp_dir().join("tack-provider-test-resolved");
        let secrets = SecretStore::file(path.clone());
        secrets
            .set("demo-secret", "the-real-key")
            .expect("seed secret");

        let claude = resolve_endpoint(
            &providers(true, "demo-secret"),
            &secrets,
            VERCEL_AI_GATEWAY_PROVIDER,
            Wire::AnthropicMessages,
        )
        .expect("resolves")
        .expect("endpoint present");
        assert_eq!(claude.base_url, "https://ai-gateway.vercel.sh/claude-code");
        assert_eq!(claude.credential_env_var, "ANTHROPIC_AUTH_TOKEN");
        assert_eq!(claude.credential.expose(), "the-real-key");

        let codex = resolve_endpoint(
            &providers(true, "demo-secret"),
            &secrets,
            VERCEL_AI_GATEWAY_PROVIDER,
            Wire::OpenAiResponses,
        )
        .expect("resolves")
        .expect("endpoint present");
        assert_eq!(codex.base_url, "https://ai-gateway.vercel.sh/codex/v1");
        assert_eq!(codex.credential_env_var, "AI_GATEWAY_API_KEY");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn credential_is_never_visible_through_debug() {
        let path = std::env::temp_dir().join("tack-provider-test-debug-redaction");
        let store = SecretStore::file(path.clone());
        store
            .set("demo-secret", "must-never-be-printed")
            .expect("seed secret");
        let endpoint = resolve_endpoint(
            &providers(true, "demo-secret"),
            &store,
            VERCEL_AI_GATEWAY_PROVIDER,
            Wire::AnthropicMessages,
        )
        .expect("resolves")
        .expect("endpoint present");

        assert!(!format!("{endpoint:?}").contains("must-never-be-printed"));
        let _ = std::fs::remove_file(&path);
    }
}
