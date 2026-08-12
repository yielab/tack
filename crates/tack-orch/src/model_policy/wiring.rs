//! Live wiring between the pure [`super::resolve_model_policy`] and real
//! `agent_profiles.limits` / `agent_fleets.default_policy` rows (`TODO.md`
//! Part III, card III-F3).
//!
//! # Where each tier's data actually lives today
//!
//! - **Request override**: `execution_requests.requested_model_provider`/
//!   `requested_model_id` (migration 044) — already read directly by the
//!   caller (there is nothing for this module to fetch).
//! - **Agent profile default**: [`parse_model_default_convention`] reads an
//!   optional `{"default_model": {"provider": ..., "model_id": ...}}` (or
//!   `{"default_model": "auto"}`) key out of `agent_profiles.limits`
//!   (migration 042) — a JSON blob already fully operator-settable via
//!   `POST /api/agent-profiles`' existing `limits` field
//!   (`crates/tack-api/src/handlers/runner_admin.rs`). No schema change.
//! - **Fleet default**: the same convention, read out of
//!   `agent_fleets.default_policy` (migration 039) — likewise already
//!   operator-settable via `POST /api/runner-fleets`.
//! - **Project default**: **no storage exists**. `projects` (migration 002)
//!   has `vocabulary`/`workflow` JSON columns, both semantically owned by
//!   the workflow engine, and no third general-purpose settings column.
//!   [`resolve_request_model_policy`] always passes `None` for this tier —
//!   see this card's handoff, "Schema/API/contract change requested," which
//!   requests either a real `projects.default_model_policy` column or an
//!   explicitly documented reuse of an existing column, decided by whoever
//!   owns the next migration batch (`crates/tack-db/src/migrations.rs` is
//!   not this card's file to edit — III.3).
//!
//! This mirrors `crate::scheduler::wiring`'s own established shape
//! (`priority_from_metadata` reading a documented, non-binding convention
//! out of `execution_requests.metadata` because no real `priority` column
//! exists) — a documented stopgap, not a second frozen contract, and named
//! as such per this card's own instruction not to build on a stopgap
//! silently.
//!
//! # Capability intersection before claim
//!
//! This module does not itself check a resolved model against any runner's
//! declared capability — that check already exists, untouched, in
//! `crate::scheduler::select::select_runner` (card III-E1) and is wired to
//! live data by `crate::scheduler::wiring::choose_request_for_runner` (card
//! III-E6). Once a [`ResolvedModelPolicy`](super::ResolvedModelPolicy)'s
//! selector is persisted as an `execution_requests` row's
//! `requested_model_provider`/`requested_model_id` (or left `NULL` for
//! `AutoSelect`), the existing, unmodified claim path enforces "unavailable
//! choice never leases" automatically — proven end-to-end, using only
//! already-existing repository methods, in
//! `crates/tack-orch/tests/model_policy_test.rs`.

use tack_db::Repository;

use super::{ModelPolicySources, ResolvedModelPolicy, resolve_model_policy};
use crate::execution::{RequestedModelId, RequestedModelProvider};
use crate::scheduler::types::ModelSelector;

/// The JSON key this module reads a tier's default model from. See this
/// module's doc comment for the exact shapes accepted.
pub const DEFAULT_MODEL_KEY: &str = "default_model";

/// Parses the `{"default_model": ...}` convention out of a raw JSON blob
/// (`agent_profiles.limits` or `agent_fleets.default_policy`). Never errors:
/// a missing key, malformed JSON, wrong type, or a partial provider/model_id
/// pair all read as "this tier expressed no opinion" (`None`) — matching
/// `crate::scheduler::wiring::priority_from_metadata`'s own established
/// posture for an unenforced convention over a real column.
pub fn parse_model_default_convention(raw_json: &str) -> Option<ModelSelector> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let default_model = value.get(DEFAULT_MODEL_KEY)?;
    if let Some(literal) = default_model.as_str() {
        return (literal == "auto").then_some(ModelSelector::AutoSelect);
    }
    let provider = default_model.get("provider")?.as_str()?;
    let model_id = default_model.get("model_id")?.as_str()?;
    Some(ModelSelector::Explicit {
        provider: RequestedModelProvider::new(provider),
        model_id: RequestedModelId::new(model_id),
    })
}

/// Fetches each tier's configured default (agent profile, fleet — project
/// has no storage today, see this module's doc comment) and resolves the
/// final [`ResolvedModelPolicy`] via [`resolve_model_policy`].
///
/// `agent_profile_id`/`fleet_id` are `None` whenever the request has no
/// agent profile, or its selector is `exact_runner` rather than `fleet`
/// (there is no fleet to read a default from in that case — the tier is
/// simply absent, exactly as if no default had been configured).
pub async fn resolve_request_model_policy(
    repo: &Repository,
    agent_profile_id: Option<&str>,
    fleet_id: Option<&str>,
    request_override: Option<ModelSelector>,
) -> Result<ResolvedModelPolicy, sqlx::Error> {
    let agent_profile_default = match agent_profile_id {
        Some(id) => repo
            .fetch_agent_profile_limits(id)
            .await?
            .as_deref()
            .and_then(parse_model_default_convention),
        None => None,
    };
    let fleet_default = match fleet_id {
        Some(id) => repo
            .fetch_fleet_default_policy(id)
            .await?
            .as_deref()
            .and_then(parse_model_default_convention),
        None => None,
    };
    let sources = ModelPolicySources {
        request_override,
        agent_profile_default,
        project_default: None,
        fleet_default,
    };
    Ok(resolve_model_policy(&sources))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_explicit_default_model() {
        let raw = r#"{"default_model":{"provider":"openai","model_id":"opaque/model-alpha"}}"#;
        let parsed = parse_model_default_convention(raw);
        assert_eq!(
            parsed,
            Some(ModelSelector::Explicit {
                provider: RequestedModelProvider::new("openai"),
                model_id: RequestedModelId::new("opaque/model-alpha"),
            })
        );
    }

    #[test]
    fn parses_an_explicit_auto_default() {
        let raw = r#"{"default_model":"auto"}"#;
        assert_eq!(
            parse_model_default_convention(raw),
            Some(ModelSelector::AutoSelect)
        );
    }

    #[test]
    fn an_unrecognised_literal_is_no_opinion_not_a_crash() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":"sometimes"}"#),
            None
        );
    }

    #[test]
    fn a_missing_key_is_no_opinion() {
        assert_eq!(parse_model_default_convention(r#"{}"#), None);
        assert_eq!(
            parse_model_default_convention(r#"{"other_field":true}"#),
            None
        );
    }

    #[test]
    fn malformed_json_is_no_opinion_not_a_panic() {
        assert_eq!(parse_model_default_convention("not json"), None);
        assert_eq!(parse_model_default_convention(""), None);
    }

    #[test]
    fn a_partial_provider_model_pair_is_no_opinion() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":{"provider":"openai"}}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(
                r#"{"default_model":{"model_id":"opaque/model-alpha"}}"#
            ),
            None
        );
    }

    #[test]
    fn wrong_value_types_are_no_opinion() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":5}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":null}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":{"provider":5,"model_id":"m"}}"#),
            None
        );
    }

    /// Nonsense/opaque values inside the convention JSON round-trip exactly
    /// as typed — the parser must not normalize or reject an id it does not
    /// recognise.
    #[test]
    fn nonsense_ids_inside_the_convention_round_trip_unmodified() {
        let raw = r#"{"default_model":{"provider":"totally-made-up-provider-9000","model_id":"totally-made-up-model-9000"}}"#;
        assert_eq!(
            parse_model_default_convention(raw),
            Some(ModelSelector::Explicit {
                provider: RequestedModelProvider::new("totally-made-up-provider-9000"),
                model_id: RequestedModelId::new("totally-made-up-model-9000"),
            })
        );
    }
}
