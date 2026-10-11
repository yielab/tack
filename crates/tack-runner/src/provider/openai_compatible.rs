//! Any server speaking OpenAI Chat Completions as a [`super::Provider`]:
//! llama.cpp, LM Studio, vLLM, Ollama, or a hosted router. Unlike the
//! hosted providers, nothing about its endpoint is vendor data — the server
//! root comes from `[provider.openai_compatible].base_url` — and a
//! credential is optional, because a local server usually checks none.
//! Its catalog is the OpenAI model list (`GET /v1/models`), which publishes
//! an id per model and nothing this crate records as metadata.

use async_trait::async_trait;

use super::{CATALOG_TIMEOUT, CatalogEntry, CatalogFetchError, KnownEndpoint, Provider, Wire};
use crate::config::{OPENAI_COMPATIBLE_CONFIG_KEY, OPENAI_COMPATIBLE_PROVIDER, ProviderConfig};
use crate::secrets::SecretValue;

/// The variable the OpenAI client libraries docket and opencode build on
/// read a bearer credential from.
const CREDENTIAL_ENV_VAR: &str = "OPENAI_API_KEY";

/// The server root `base_url` names, with a trailing `/` or `/v1` removed,
/// so the `/v1` this provider appends is never doubled: `http://h:8081`,
/// `http://h:8081/` and `http://h:8081/v1` all name the same server.
fn server_root(base_url: &str) -> &str {
    let trimmed = base_url.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
}

/// `config`'s server root, or `None` when the entry names no usable
/// `base_url` — configuration loading rejects an enabled entry in that
/// state, so a `None` here is a `ProviderConfig` built by hand.
fn configured_root(config: &ProviderConfig) -> Option<&str> {
    config
        .base_url
        .as_deref()
        .map(server_root)
        .filter(|root| !root.is_empty())
}

pub(crate) struct OpenAiCompatible;

#[async_trait]
impl Provider for OpenAiCompatible {
    fn wire_name(&self) -> &'static str {
        OPENAI_COMPATIBLE_PROVIDER
    }

    fn config_key(&self) -> &'static str {
        OPENAI_COMPATIBLE_CONFIG_KEY
    }

    fn display_name(&self) -> &'static str {
        "OpenAI-compatible endpoint"
    }

    // Only the wire docket and opencode speak: codex needs the Responses
    // shape and claude-code the Anthropic Messages shape, and a Chat
    // Completions server answers neither.
    fn serves(&self, wire: Wire) -> bool {
        matches!(wire, Wire::OpenAiChatCompletions)
    }

    fn endpoint(&self, wire: Wire, config: &ProviderConfig) -> Option<KnownEndpoint> {
        if !self.serves(wire) {
            return None;
        }
        let root = configured_root(config)?;
        Some(KnownEndpoint {
            // Both harnesses append `/chat/completions` themselves.
            base_url: format!("{root}/v1"),
            credential_env_var: CREDENTIAL_ENV_VAR,
        })
    }

    // A local server has no account to authenticate: an empty `secret`
    // means no credential is resolved or injected, and the catalog request
    // carries no `Authorization` header.
    fn requires_credential(&self) -> bool {
        false
    }

    // A server behind this endpoint may serve whatever model it has loaded
    // regardless of the id requested (llama.cpp does), so a harness's own
    // init line — written before any request reaches it — is no evidence
    // of what served the run. Explicit rather than left to the trait
    // default, the same as the gateway module's own explicit `false`.
    fn confirms_served_model_from_init_line(&self) -> bool {
        false
    }

    async fn fetch_catalog(
        &self,
        config: &ProviderConfig,
        credential: Option<&SecretValue>,
    ) -> Result<Vec<CatalogEntry>, CatalogFetchError> {
        let Some(root) = configured_root(config) else {
            tracing::warn!("openai_compatible catalog fetch skipped: no base_url configured");
            return Err(CatalogFetchError::Transport);
        };
        let client = reqwest::Client::builder()
            .timeout(CATALOG_TIMEOUT)
            .user_agent(concat!("tack-runner/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| CatalogFetchError::Transport)?;
        let request = client.get(format!("{root}/v1/models"));
        let request = match credential {
            Some(secret) => request.bearer_auth(secret.expose()),
            None => request,
        };
        let response = request
            .send()
            .await
            .map_err(|_| CatalogFetchError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(CatalogFetchError::Status(status.as_u16()));
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| CatalogFetchError::Status(status.as_u16()))?;
        parse_catalog(&body).map_err(|_| CatalogFetchError::Status(status.as_u16()))
    }
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

/// Parses one OpenAI model-list body into the common [`CatalogEntry`]
/// shape. The list publishes an `id`, an `owned_by` and a `created` time
/// per model and nothing about its context window, price or modality, so
/// every entry's metadata is `None` — never inferred. A server's own extra
/// fields (llama.cpp's `meta`, with its training context) are tolerated
/// and not read: no two servers agree on them.
fn parse_catalog(body: &[u8]) -> Result<Vec<CatalogEntry>, serde_json::Error> {
    let parsed: CatalogResponse = serde_json::from_slice(body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|model| CatalogEntry {
            id: model.id,
            context_window: None,
            price: None,
            modality: None,
        })
        .collect())
}

#[cfg(test)]
#[path = "openai_compatible/tests.rs"]
mod tests;
