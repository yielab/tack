//! A run's model: the request's own, otherwise the project's default model
//! when the run's agent is the project's default agent, otherwise none (the
//! agent chooses).
//!
//! [`resolve_model_policy`] is pure (no I/O/clock/DB); `tack_app::model_policy_wiring`
//! is the live `tack-db`-backed caller. Whether a harness can run with no model
//! is `select_runner`'s job, not this module's.

/// Which input supplied the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPolicyTier {
    Request,
    ProjectDefault,
}

/// The facts the policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPolicySources<'a> {
    /// The model the request named, if it did.
    pub request: Option<&'a str>,
    /// The project's default model.
    pub project_default_model: Option<&'a str>,
    /// The project's default agent (harness kind).
    pub project_default_agent: Option<&'a str>,
    /// The agent (harness kind) this run uses.
    pub run_agent: &'a str,
}

/// The outcome of [`resolve_model_policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelPolicy {
    pub model: Option<String>,
    /// `None` only when no input named a model.
    pub source: Option<ModelPolicyTier>,
}

/// The request's model; else the project's default model when the run's
/// agent is the project's default agent; else none. The string is never
/// inspected or normalized.
pub fn resolve_model_policy(sources: &ModelPolicySources<'_>) -> ResolvedModelPolicy {
    if let Some(model) = sources.request {
        return ResolvedModelPolicy {
            model: Some(model.to_owned()),
            source: Some(ModelPolicyTier::Request),
        };
    }
    if let (Some(model), Some(agent)) =
        (sources.project_default_model, sources.project_default_agent)
        && agent == sources.run_agent
    {
        return ResolvedModelPolicy {
            model: Some(model.to_owned()),
            source: Some(ModelPolicyTier::ProjectDefault),
        };
    }
    ResolvedModelPolicy {
        model: None,
        source: None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
