//! Deterministic model-selection precedence: request override →
//! agent-profile default → project default → fleet default → (nothing
//! configured) auto-select.
//!
//! [`resolve_model_policy`] is pure (no I/O/clock/DB) — see [`wiring`] for
//! the live `tack-db`-backed caller. Every tier's value is a
//! [`crate::scheduler::types::ModelSelector`] in the *requested* namespace,
//! never the *actual* one `crate::usage_provenance` compares against;
//! intersecting against a runner's capability is `select_runner`'s job.

pub mod wiring;

use crate::scheduler::types::ModelSelector;

/// The four precedence tiers, most specific first (see the test module for
/// the exhaustive 2^4 table proving this order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelPolicyTier {
    RequestOverride,
    AgentProfile,
    Project,
    Fleet,
}

impl ModelPolicyTier {
    /// The precedence walk order; only `resolve_model_policy` iterates it.
    pub const ORDER: [ModelPolicyTier; 4] = [
        ModelPolicyTier::RequestOverride,
        ModelPolicyTier::AgentProfile,
        ModelPolicyTier::Project,
        ModelPolicyTier::Fleet,
    ];
}

/// One `Option<ModelSelector>` per precedence tier. `None` means "no
/// opinion", not "requests auto-select": `Some(AutoSelect)` *stops* the
/// walk at that tier rather than falling through to a less-specific one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelPolicySources {
    pub request_override: Option<ModelSelector>,
    pub agent_profile_default: Option<ModelSelector>,
    /// Read from `projects.default_model`.
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
    /// Which tier supplied `selector`; `None` only when every tier was
    /// absent, in which case `selector` is `AutoSelect` by construction.
    pub source: Option<ModelPolicyTier>,
}

/// Walks [`ModelPolicyTier::ORDER`] and returns the first present tier's
/// value, or `AutoSelect` with `source: None` if every tier is absent. Pure,
/// deterministic and total over all 16 presence combinations (see the test
/// suite's exhaustive table).
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
#[path = "tests.rs"]
mod tests;
