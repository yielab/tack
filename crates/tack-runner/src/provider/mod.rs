//! Runner-side provider endpoints: where a harness sends requests and which
//! credential authenticates them, when that differs from the harness's own
//! ambient login.
//!
//! Every harness this crate drives already has a working, credential-free
//! mode (the harness's own subscription/login) needing nothing from this
//! module — it is simply the absence of a configured entry, treated as
//! `None`/a typed absence, never a second implicit case to branch on.
//!
//! What this module adds is the other mode: `RunnerConfig::providers` names
//! an entry (`[provider.<name>]`), and [`resolve_endpoint`] tells a harness
//! adapter what to inject — a base URL, the credential env var name, and the
//! resolved credential. A gateway and a vendor's own direct API are the same
//! shape here, so a second provider is a new [`Provider`] impl registered in
//! [`registry`], never a new mechanism. Each provider parses its own catalog
//! body/auth/pricing shape into the common [`CatalogEntry`] shape.
//!
//! Two providers exist today: [`vercel_ai_gateway`] and [`anthropic`].

mod anthropic;
mod vercel_ai_gateway;

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    ModelCombination, ModelId, ModelMetadata, ModelProvider, RunnerCapabilities,
};

use crate::Clock;
use crate::config::ProviderConfig;
use crate::secrets::{SecretStore, SecretValue};

const CATALOG_TIMEOUT: Duration = Duration::from_secs(10);

/// Recorded in `ModelCombination::discovery` for a catalog-sourced entry,
/// distinct from `"reported"` — a vendor's published list, not something
/// measured while actually running a task (ADR 0061 decision 3).
pub const CATALOG_DISCOVERY: &str = "catalog_reported";

/// The wire shape a harness adapter already speaks. Selects which
/// [`Provider::endpoint`] applies and how the adapter injects it —
/// environment variables for an Anthropic-Messages CLI, invocation flags
/// for an OpenAI-Responses one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    AnthropicMessages,
    OpenAiResponses,
}

/// Which [`Wire`] a harness kind speaks, read from its descriptor, so
/// [`attach_catalog`] never records a combination for a harness a provider
/// can't reach (a provider need not serve every wire).
fn wire_for_harness(harness_kind: &str) -> Option<Wire> {
    crate::harness::descriptor(harness_kind).map(|descriptor| descriptor.wire)
}

/// What an adapter must inject to point a spawn at a configured provider:
/// where to send requests, which env var carries the credential, its
/// resolved value, and a display label (codex's `-c model_providers.<key>.name`).
#[derive(Debug)]
pub struct ProviderEndpoint {
    pub base_url: String,
    pub credential_env_var: String,
    pub credential: SecretValue,
    pub display_name: String,
}

/// Fixed, non-configurable facts about one provider's endpoint for one
/// wire — vendor data, never user configuration (`[provider.<name>]` only
/// exposes `enabled`/`secret`).
pub struct KnownEndpoint {
    pub base_url: &'static str,
    pub credential_env_var: &'static str,
}

/// One entry from a provider's own model catalog, parsed into the shape
/// every provider fills regardless of its vendor's body shape (ADR 0063
/// decision 5). An unpublished field is `None`, never a default or zero
/// (decision 7). `price`/`modality` keep the vendor's raw shape rather than
/// a normalized one — vendor catalogs disagree too much on both to avoid
/// silently falsifying most of them.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub context_window: Option<u64>,
    pub price: Option<serde_json::Value>,
    pub modality: Option<serde_json::Value>,
}

/// Why `requested_provider` named a known endpoint but it could not resolve
/// into a working [`ProviderEndpoint`]. Every variant names a fact, never a
/// secret value — safe in a `HarnessError::Rejected` reason or a log line.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider {0:?} has no enabled [provider.<name>] entry")]
    NotConfigured(String),
    #[error("provider {0:?} secret: {1}")]
    Secret(String, crate::secrets::SecretError),
}

/// Why a catalog request did not produce a parsed body: a status the
/// vendor returned, or no response at all.
#[derive(Debug)]
pub enum CatalogFetchError {
    Transport,
    Status(u16),
}

/// One provider this runner knows how to talk to in key+endpoint mode (ADR
/// 0063 decisions 1, 2, 4). Every vendor difference lives inside one impl;
/// [`resolve_endpoint`]/[`attach_catalog`] call only these methods, never a
/// vendor's name. Adding a provider is one more [`registry`] entry.
#[async_trait]
pub trait Provider: Send + Sync {
    /// The value recorded as `ModelProvider`/`requested_model_provider`.
    fn wire_name(&self) -> &'static str;

    /// The `[provider.<name>]` table name and `RunnerConfig::providers` key.
    fn config_key(&self) -> &'static str;

    /// A human-readable label for `tack runner doctor`.
    fn display_name(&self) -> &'static str;

    /// This provider's endpoint for `wire`, or `None` if it doesn't serve it.
    fn endpoint(&self, wire: Wire) -> Option<KnownEndpoint>;

    /// Whether a harness's own init/result line states which model actually
    /// served the request, not just which was requested. The init line is
    /// emitted before any network call, so it can only echo what was
    /// configured — a gateway can substitute a model (routing, fallback), a
    /// vendor's direct API cannot. Defaults to `false` so an override-less
    /// provider is never credited with an unproven capability.
    fn confirms_served_model_from_init_line(&self) -> bool {
        false
    }

    /// Fetches and parses this provider's own model catalog, using
    /// whatever auth placement and body shape its vendor requires.
    async fn fetch_catalog(
        &self,
        secret: &SecretValue,
    ) -> Result<Vec<CatalogEntry>, CatalogFetchError>;
}

/// Every provider this build knows about, in the fixed order `tack runner
/// doctor` displays them. Nothing outside this module constructs a
/// [`Provider`]; a caller only ever walks this list.
pub fn registry() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(vercel_ai_gateway::VercelAiGateway),
        Box::new(anthropic::Anthropic),
    ]
}

/// The harness kinds this provider's endpoints reach, in
/// [`crate::harness::DESCRIPTORS`] order — for `tack runner doctor`.
pub fn reaches(provider: &dyn Provider) -> Vec<&'static str> {
    crate::harness::DESCRIPTORS
        .into_iter()
        .filter(|descriptor| provider.endpoint(descriptor.wire).is_some())
        .map(|descriptor| descriptor.kind)
        .collect()
}

/// Whether a harness adapter must record the model for `requested_provider`
/// as `requested_not_confirmed` rather than `harness_reported` — true only
/// when `requested_provider` names a registered provider whose
/// [`Provider::confirms_served_model_from_init_line`] is `false`. A name
/// matching no registered provider always records `harness_reported`
/// unconditionally, unaffected by this function.
pub fn requires_unconfirmed_model_recording(requested_provider: &str) -> bool {
    registry()
        .into_iter()
        .find(|provider| provider.wire_name() == requested_provider)
        .is_some_and(|provider| !provider.confirms_served_model_from_init_line())
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
    let Some(provider) = registry()
        .into_iter()
        .find(|candidate| candidate.wire_name() == requested_provider)
    else {
        return Ok(None);
    };
    let Some(known) = provider.endpoint(wire) else {
        return Ok(None);
    };
    let config = providers
        .get(provider.config_key())
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
        display_name: provider.display_name().to_owned(),
    }))
}

/// What asking one configured provider for its model catalog produced — a
/// typed absence for every non-catalog outcome, matching
/// `crate::harness::discover`'s own posture toward a harness that
/// cannot be discovered: never a stale or placeholder list.
#[derive(Debug, Clone)]
pub enum CatalogStatus {
    /// No enabled `[provider.<name>]` entry exists for this provider.
    NotConfigured,
    /// The entry is enabled but its secret does not resolve.
    SecretUnresolved,
    /// The catalog endpoint answered a non-success status, or could not be
    /// reached at all (`status: None`).
    Unreachable { status: Option<u16> },
    /// The catalog request succeeded. `priced_model_count` and
    /// `context_window_model_count` count only entries that published that
    /// field (ADR 0063 decision 7) — never inferred for the rest.
    Configured {
        model_count: usize,
        priced_model_count: usize,
        context_window_model_count: usize,
        checked_at: DateTime<Utc>,
    },
}

/// Fetches every enabled provider's catalog and, on success, records one
/// [`ModelCombination`] per catalog-eligible harness the endpoint actually
/// reaches — never inventing an entry for an unprobed harness. Returns one
/// [`CatalogStatus`] per provider keyed by `config_key`; one provider's
/// secret failure must not suppress another's catalog.
pub async fn attach_catalog<C: Clock>(
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> BTreeMap<String, CatalogStatus> {
    attach_catalog_to(registry(), capabilities, providers, secrets, clock).await
}

/// The body of [`attach_catalog`], parameterized over the provider list so
/// tests can exercise the orchestration guarantee (one provider's failure
/// never suppresses another's) against fakes, with no network involved.
async fn attach_catalog_to<C: Clock>(
    providers_registry: Vec<Box<dyn Provider>>,
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> BTreeMap<String, CatalogStatus> {
    let mut statuses = BTreeMap::new();
    for provider in providers_registry {
        let status =
            attach_one_catalog(provider.as_ref(), capabilities, providers, secrets, clock).await;
        statuses.insert(provider.config_key().to_owned(), status);
    }
    statuses
}

async fn attach_one_catalog<C: Clock>(
    provider: &dyn Provider,
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> CatalogStatus {
    let Some(config) = providers
        .get(provider.config_key())
        .filter(|config| config.enabled)
    else {
        return CatalogStatus::NotConfigured;
    };
    let secret = match secrets.resolve(&config.secret) {
        Ok(secret) => secret,
        Err(_) => return CatalogStatus::SecretUnresolved,
    };
    tracing::debug!(secret = %config.secret, "provider secret resolved for catalog fetch");
    let entries = match provider.fetch_catalog(&secret).await {
        Ok(entries) => entries,
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
    tracing::debug!(model_count = entries.len(), "provider catalog fetched");

    let checked_at = DateTime::<Utc>::from(clock.now());
    let model_ids: Vec<ModelId> = entries
        .iter()
        .map(|entry| ModelId::new(entry.id.clone()))
        .collect();
    // Only models the catalog actually said something about get an entry —
    // a model absent from this map is one the provider published nothing
    // for, never a claim of zero (ADR 0063 decision 7).
    let model_metadata: BTreeMap<ModelId, ModelMetadata> = entries
        .iter()
        .filter(|entry| {
            entry.context_window.is_some() || entry.price.is_some() || entry.modality.is_some()
        })
        .map(|entry| {
            (
                ModelId::new(entry.id.clone()),
                ModelMetadata {
                    context_window: entry.context_window,
                    price: entry.price.clone(),
                    modality: entry.modality.clone(),
                    additional: Default::default(),
                },
            )
        })
        .collect();
    for harness in capabilities.harnesses.iter_mut() {
        let Some(wire) = wire_for_harness(harness.harness_kind.as_str()) else {
            continue;
        };
        if provider.endpoint(wire).is_none() {
            continue;
        }
        harness.model_combinations.push(ModelCombination {
            model_provider: ModelProvider::new(provider.wire_name()),
            model_ids: model_ids.clone(),
            discovery: CATALOG_DISCOVERY.to_owned(),
            model_metadata: model_metadata.clone(),
            additional: Default::default(),
        });
    }
    CatalogStatus::Configured {
        model_count: entries.len(),
        priced_model_count: entries.iter().filter(|entry| entry.price.is_some()).count(),
        context_window_model_count: entries
            .iter()
            .filter(|entry| entry.context_window.is_some())
            .count(),
        checked_at,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
