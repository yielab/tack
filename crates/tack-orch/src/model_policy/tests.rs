use super::*;

/// The three tiers as one table: the request's model wins; otherwise the
/// project's default model, but only for the project's default agent;
/// otherwise none.
#[test]
fn a_runs_model_is_the_requests_then_the_projects_for_its_default_agent_then_none() {
    struct Case {
        name: &'static str,
        sources: ModelPolicySources<'static>,
        model: Option<&'static str>,
        source: Option<ModelPolicyTier>,
    }
    let base = ModelPolicySources {
        request: None,
        project_default_model: Some("project-model"),
        project_default_agent: Some("codex"),
        run_agent: "codex",
    };
    let cases = [
        Case {
            name: "the request wins over the project default",
            sources: ModelPolicySources {
                request: Some("request-model"),
                ..base
            },
            model: Some("request-model"),
            source: Some(ModelPolicyTier::Request),
        },
        Case {
            name: "the project default applies to the project's default agent",
            sources: base,
            model: Some("project-model"),
            source: Some(ModelPolicyTier::ProjectDefault),
        },
        Case {
            name: "a run on another agent gets no project model",
            sources: ModelPolicySources {
                run_agent: "claude-code",
                ..base
            },
            model: None,
            source: None,
        },
        Case {
            name: "a project with no default agent gives no model",
            sources: ModelPolicySources {
                project_default_agent: None,
                ..base
            },
            model: None,
            source: None,
        },
        Case {
            name: "a project with no default model gives none",
            sources: ModelPolicySources {
                project_default_model: None,
                ..base
            },
            model: None,
            source: None,
        },
        Case {
            name: "the request wins on another agent too",
            sources: ModelPolicySources {
                request: Some("request-model"),
                run_agent: "claude-code",
                ..base
            },
            model: Some("request-model"),
            source: Some(ModelPolicyTier::Request),
        },
        Case {
            name: "an opaque string passes through unchanged",
            sources: ModelPolicySources {
                request: Some("プロバイダー::🚀 weird/model id:with colons"),
                ..base
            },
            model: Some("プロバイダー::🚀 weird/model id:with colons"),
            source: Some(ModelPolicyTier::Request),
        },
    ];
    for case in cases {
        let resolved = resolve_model_policy(&case.sources);
        assert_eq!(resolved.model.as_deref(), case.model, "{}", case.name);
        assert_eq!(resolved.source, case.source, "{}", case.name);
    }
}
