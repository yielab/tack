//! Deterministic model-selection precedence (`TODO.md` Part III, card
//! III-F3): request override → agent-profile default → project default →
//! fleet default → (nothing configured) auto-select.
//!
//! [`resolve_model_policy`] is pure: no I/O, no clock, no database handle —
//! see [`wiring`] for the live `tack-db`-backed caller that fetches each
//! tier's configured default and hands the result here, mirroring
//! `crate::scheduler`'s own pure-core/live-wiring split
//! (`select`/`batch` vs `wiring`).
//!
//! # Vocabulary discipline (III.0)
//!
//! Every tier's value is a [`crate::scheduler::types::ModelSelector`], which
//! itself carries [`crate::execution::RequestedModelProvider`]/
//! [`crate::execution::RequestedModelId`] — the *requested* namespace, never
//! conflated with the *actual* namespace
//! ([`crate::execution::ActualModelProvider`]/[`crate::execution::ActualModelId`])
//! that `crate::usage_provenance` compares against. A resolved value from
//! this module is always still a *request*, whichever tier supplied it —
//! intersecting it against a runner's declared capability is
//! [`crate::scheduler::select::select_runner`]'s job (unmodified by this
//! card; see [`wiring`]'s module doc comment for the integration proof).

pub mod wiring;

use crate::scheduler::types::ModelSelector;

/// The four precedence tiers, most specific first. This fixed order is the
/// card's own acceptance bar ("all presence combinations deterministic...
/// with the precedence order pinned") — see `mod.rs`'s test module for the
/// exhaustive 2^4 table proving every presence combination resolves to
/// exactly this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelPolicyTier {
    RequestOverride,
    AgentProfile,
    Project,
    Fleet,
}

impl ModelPolicyTier {
    /// The precedence walk order. `resolve_model_policy` iterates this
    /// exact slice; nothing else in this module may reorder it.
    pub const ORDER: [ModelPolicyTier; 4] = [
        ModelPolicyTier::RequestOverride,
        ModelPolicyTier::AgentProfile,
        ModelPolicyTier::Project,
        ModelPolicyTier::Fleet,
    ];
}

/// One `Option<ModelSelector>` per precedence tier.
///
/// `None` means "this tier expressed no opinion" — not "this tier
/// explicitly requests auto-select." That distinction is load-bearing:
/// `Some(ModelSelector::AutoSelect)` at a tier is a real, if unusual,
/// configuration (an operator explicitly pinning "always auto-select at
/// this level") that *stops* the walk at that tier rather than falling
/// through to a less-specific tier that might name a concrete model. Only
/// an *absent* tier (`None`) is skipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelPolicySources {
    pub request_override: Option<ModelSelector>,
    pub agent_profile_default: Option<ModelSelector>,
    /// No `projects` table storage exists for a default model policy today
    /// — see this card's handoff, "Schema/API/contract change requested."
    /// The type carries this tier so precedence is fully expressed and
    /// future-proof; [`wiring::resolve_request_model_policy`] always passes
    /// `None` here until such storage exists.
    pub project_default: Option<ModelSelector>,
    pub fleet_default: Option<ModelSelector>,
}

impl ModelPolicySources {
    fn get(&self, tier: ModelPolicyTier) -> &Option<ModelSelector> {
        match tier {
            ModelPolicyTier::RequestOverride => &self.request_override,
            ModelPolicyTier::AgentProfile => &self.agent_profile_default,
            ModelPolicyTier::Project => &self.project_default,
            ModelPolicyTier::Fleet => &self.fleet_default,
        }
    }
}

/// The outcome of walking [`ModelPolicyTier::ORDER`] against a
/// [`ModelPolicySources`] value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelPolicy {
    pub selector: ModelSelector,
    /// Which tier supplied `selector`. `None` only when every tier was
    /// absent — in that case `selector` is `ModelSelector::AutoSelect` by
    /// construction (the request-shape III.1.2 already allows: both
    /// `requested_model_provider`/`requested_model_id` nullable), not any
    /// tier's own opinion.
    pub source: Option<ModelPolicyTier>,
}

/// Walks [`ModelPolicyTier::ORDER`] and returns the first present tier's
/// value, or `AutoSelect` with `source: None` if every tier is absent. Pure,
/// deterministic, and total — every one of the 2^4 = 16 presence
/// combinations of `sources`' four fields produces exactly one outcome; see
/// this module's test suite for the exhaustive table.
pub fn resolve_model_policy(sources: &ModelPolicySources) -> ResolvedModelPolicy {
    for tier in ModelPolicyTier::ORDER {
        if let Some(selector) = sources.get(tier) {
            return ResolvedModelPolicy {
                selector: selector.clone(),
                source: Some(tier),
            };
        }
    }
    ResolvedModelPolicy {
        selector: ModelSelector::AutoSelect,
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{RequestedModelId, RequestedModelProvider};

    fn sentinel(tag: &str) -> ModelSelector {
        ModelSelector::Explicit {
            provider: RequestedModelProvider::new(format!("{tag}-provider")),
            model_id: RequestedModelId::new(format!("{tag}-model")),
        }
    }

    /// The card's own acceptance bar, verbatim: "all presence combinations
    /// deterministic... table test over the 2^4 presence combinations of
    /// request override / agent profile / project / fleet — including
    /// all-absent — with the precedence order pinned." Each tier gets a
    /// distinct sentinel value so a wrong winner is caught, not just "some
    /// value came back."
    #[test]
    fn every_presence_combination_resolves_to_the_pinned_precedence_order() {
        for mask in 0u8..16 {
            let request_present = mask & 0b0001 != 0;
            let profile_present = mask & 0b0010 != 0;
            let project_present = mask & 0b0100 != 0;
            let fleet_present = mask & 0b1000 != 0;

            let sources = ModelPolicySources {
                request_override: request_present.then(|| sentinel("request")),
                agent_profile_default: profile_present.then(|| sentinel("profile")),
                project_default: project_present.then(|| sentinel("project")),
                fleet_default: fleet_present.then(|| sentinel("fleet")),
            };

            let (expected_tier, expected_selector) = if request_present {
                (Some(ModelPolicyTier::RequestOverride), sentinel("request"))
            } else if profile_present {
                (Some(ModelPolicyTier::AgentProfile), sentinel("profile"))
            } else if project_present {
                (Some(ModelPolicyTier::Project), sentinel("project"))
            } else if fleet_present {
                (Some(ModelPolicyTier::Fleet), sentinel("fleet"))
            } else {
                (None, ModelSelector::AutoSelect)
            };

            let resolved = resolve_model_policy(&sources);
            assert_eq!(
                resolved.source, expected_tier,
                "mask {mask:#06b}: wrong winning tier"
            );
            assert_eq!(
                resolved.selector, expected_selector,
                "mask {mask:#06b}: wrong resolved selector"
            );
        }
    }

    #[test]
    fn all_tiers_absent_resolves_to_auto_select_with_no_source() {
        let resolved = resolve_model_policy(&ModelPolicySources::default());
        assert_eq!(resolved.selector, ModelSelector::AutoSelect);
        assert_eq!(resolved.source, None);
    }

    #[test]
    fn a_tier_explicitly_configured_as_auto_select_stops_the_walk_there() {
        // Distinct from "absent": the agent profile tier here has a real
        // opinion (auto-select), so a fleet default underneath it must
        // never be consulted, even though AutoSelect and "nothing
        // configured" both ultimately produce ModelSelector::AutoSelect.
        let sources = ModelPolicySources {
            request_override: None,
            agent_profile_default: Some(ModelSelector::AutoSelect),
            project_default: None,
            fleet_default: Some(sentinel("fleet")),
        };
        let resolved = resolve_model_policy(&sources);
        assert_eq!(resolved.selector, ModelSelector::AutoSelect);
        assert_eq!(resolved.source, Some(ModelPolicyTier::AgentProfile));
    }

    #[test]
    fn resolution_is_deterministic_across_repeated_calls() {
        let sources = ModelPolicySources {
            request_override: None,
            agent_profile_default: Some(sentinel("profile")),
            project_default: Some(sentinel("project")),
            fleet_default: Some(sentinel("fleet")),
        };
        let first = resolve_model_policy(&sources);
        for _ in 0..25 {
            assert_eq!(resolve_model_policy(&sources), first);
        }
    }

    /// "Nonsense id round-trips": an unrecognised model id/provider must
    /// flow through resolution byte-for-byte — never normalized, rejected,
    /// or coerced into a known value. Covers ASCII punctuation, unicode
    /// (including combining/emoji code points), and a very long string.
    #[test]
    fn nonsense_opaque_ids_round_trip_through_resolution_unmodified() {
        let long_model = "x".repeat(10_000);
        let cases: [(&str, &str); 3] = [
            (
                "totally-made-up-provider-9000",
                "totally-made-up-model-9000",
            ),
            (
                "プロバイダー::🚀",
                "weird/model id with spaces::and:colons 🧭",
            ),
            ("provider-x", long_model.as_str()),
        ];
        for (provider_raw, model_raw) in cases {
            let sources = ModelPolicySources {
                request_override: Some(ModelSelector::Explicit {
                    provider: RequestedModelProvider::new(provider_raw),
                    model_id: RequestedModelId::new(model_raw),
                }),
                agent_profile_default: None,
                project_default: None,
                fleet_default: None,
            };
            let resolved = resolve_model_policy(&sources);
            match resolved.selector {
                ModelSelector::Explicit { provider, model_id } => {
                    assert_eq!(provider.as_str(), provider_raw);
                    assert_eq!(model_id.as_str(), model_raw);
                }
                ModelSelector::AutoSelect => panic!("expected the explicit override to win"),
            }
        }
    }

    /// The same opaque values must also survive a JSON round trip
    /// unmodified (the `opaque_id!` macro's `#[serde(transparent)]`
    /// contract in `crate::execution::types`) — proven directly here rather
    /// than only assumed from that macro's own doc comment.
    #[test]
    fn nonsense_opaque_ids_round_trip_through_json_unmodified() {
        let very_long = "y".repeat(10_000);
        for raw in [
            "totally-made-up-model-9000",
            "プロバイダー::🚀 combining-\u{0301}",
            very_long.as_str(),
        ] {
            let id = RequestedModelId::new(raw);
            let json = serde_json::to_string(&id).expect("serialize opaque id");
            let round_tripped: RequestedModelId =
                serde_json::from_str(&json).expect("deserialize opaque id");
            assert_eq!(round_tripped.as_str(), raw);
            assert_eq!(id, round_tripped);
        }
    }
}
