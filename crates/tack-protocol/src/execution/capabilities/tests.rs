use super::*;

#[test]
fn additive_fields_round_trip() {
    let raw = r#"{
      "protocol_version":1,"runner_version":"0.1.0","reported_at":"2026-08-06T12:00:00Z",
      "labels":{},"concurrency":{"total":1,"available":1},"harnesses":[{
        "harness_kind":"codex","installed_version":"1","probe_error":null,
        "probed_at":"2026-08-06T12:00:00Z","requires_model":true,"requires_model_reason":"name one","models":{"source":"listed","ids":["a"],"reason":null},"sign_in":{"state":"signed_out","detail":"Not logged in","command":"codex login"},"docs":{"models":"https://example.test/m","sign_in":"https://example.test/s"},"future_harness_key":true
      }],"features":{
        "cancel":{"support":"supported","reason":null},"resume":{"support":"unsupported","reason":"no"},
        "decisions":{"support":"supported","reason":null},"artifacts":{"support":"supported","reason":null},
        "usage":{"support":"advisory","reason":"partial"}
      },"limits":{"event_payload_bytes_max":1,"artifact_content_bytes_max":2},"future_capability":{"nested":true}
    }"#;
    let parsed: RunnerCapabilities = serde_json::from_str(raw).expect("capabilities fixture");
    let harness = &parsed.harnesses[0];
    assert!(harness.requires_model);
    assert_eq!(
        harness.models.as_ref().map(|m| m.source),
        Some(ModelListSource::Listed)
    );
    assert_eq!(
        harness.sign_in.as_ref().map(|s| s.state),
        Some(SignInState::SignedOut)
    );
    assert_eq!(harness.default_model, None);
    let round_trip = serde_json::to_value(parsed).expect("serialize");
    assert_eq!(round_trip["future_capability"]["nested"], true);
    assert_eq!(round_trip["harnesses"][0]["future_harness_key"], true);
}

/// `RunnerCapabilities` cannot parse either embedded snapshot: both
/// fixtures omit `runner_version` (a sibling field in the enclosing
/// envelope, not nested under `capabilities`), which `RunnerCapabilities`
/// requires. `EmbeddedCapabilitySnapshot` is the type for this shape;
/// this test proves it parses enrollment's full example and refresh's
/// sparse one (`"harnesses": []`, `"features": {}`) unchanged, and that
/// unknown keys still survive a round trip.
/// Neither fixture parses as `RunnerCapabilities` (both omit
/// `runner_version`, a sibling field in the enclosing envelope) but both
/// parse as, and round-trip exactly through, `EmbeddedCapabilitySnapshot`.
/// Returns the parsed value so the caller can check fixture-specific shape.
fn assert_embedded_snapshot_round_trips(
    capabilities: &serde_json::Value,
) -> EmbeddedCapabilitySnapshot {
    assert!(
        serde_json::from_value::<RunnerCapabilities>(capabilities.clone()).is_err(),
        "an embedded snapshot omitting runner_version must not parse as RunnerCapabilities"
    );
    let parsed: EmbeddedCapabilitySnapshot =
        serde_json::from_value(capabilities.clone()).expect("embedded capabilities");
    assert_eq!(
        serde_json::to_value(&parsed).expect("serialize embedded capabilities"),
        *capabilities,
        "an embedded snapshot must round-trip exactly"
    );
    parsed
}

#[test]
fn embedded_capability_snapshot_parses_full_and_sparse_fixtures() {
    let enrollment: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/enrollment.request.json"
    ))
    .expect("enrollment fixture JSON");
    let parsed_enrollment = assert_embedded_snapshot_round_trips(&enrollment["capabilities"]);
    assert_eq!(parsed_enrollment.harnesses.len(), 1);
    assert_eq!(parsed_enrollment.features["cancel"]["support"], "supported");

    let refresh: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/refresh.request.json"
    ))
    .expect("refresh fixture JSON");
    let parsed_refresh = assert_embedded_snapshot_round_trips(&refresh["capabilities"]);
    assert!(parsed_refresh.harnesses.is_empty());
    assert_eq!(parsed_refresh.features, serde_json::json!({}));

    let mut with_future_field = enrollment["capabilities"].clone();
    with_future_field["future_capability_field"] = serde_json::json!({"nested": true});
    let parsed_additive: EmbeddedCapabilitySnapshot =
        serde_json::from_value(with_future_field.clone()).expect("parse additive field");
    assert_eq!(
        serde_json::to_value(parsed_additive).expect("serialize additive field"),
        with_future_field,
        "unrecognised keys on an embedded snapshot must survive a round trip"
    );
}

#[test]
fn team_ceiling_fields_are_optional_and_round_trip() {
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.json"
    ))
    .expect("capabilities fixture JSON");
    let parsed: RunnerCapabilities = serde_json::from_value(old.clone()).expect("old snapshot");
    assert_eq!(parsed.harnesses[0].autonomy_ceiling, None);
    assert_eq!(parsed.harnesses[0].teams, None);
    assert_eq!(serde_json::to_value(&parsed).expect("serialize"), old);

    let with: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.with-teams.json"
    ))
    .expect("with-teams fixture JSON");
    let parsed: RunnerCapabilities = serde_json::from_value(with.clone()).expect("with teams");
    let harness = &parsed.harnesses[0];
    assert_eq!(harness.autonomy_ceiling.as_deref(), Some("L3"));
    let teams = harness.teams.as_ref().expect("teams reported");
    assert_eq!(teams[0].violations[0].fix, "run it lights-out");
    assert_eq!(serde_json::to_value(&parsed).expect("serialize"), with);
}

#[test]
fn an_agents_own_features_ride_beside_its_report_and_round_trip() {
    let mut report: HarnessCapability = serde_json::from_value(serde_json::json!({
        "harness_kind": "codex", "installed_version": "1", "probe_error": null,
        "probed_at": "2026-08-06T12:00:00Z"
    }))
    .expect("a report without features parses");
    assert_eq!(
        report.features(),
        None,
        "absent stays absent, never invented"
    );

    let declared = FeatureCapabilities {
        cancel: CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: Some("stops the process group".to_owned()),
            additional: BTreeMap::new(),
        },
        resume: CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some("a restart ends the run".to_owned()),
            additional: BTreeMap::new(),
        },
        decisions: CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: None,
            additional: BTreeMap::new(),
        },
        artifacts: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: None,
            additional: BTreeMap::new(),
        },
        usage: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some("partial".to_owned()),
            additional: BTreeMap::new(),
        },
        proposals: None,
        activity: None,
        converse: None,
        additional: BTreeMap::new(),
    };
    report.set_features(&declared);
    let wire = serde_json::to_value(&report).expect("serialize");
    assert_eq!(wire["features"]["cancel"]["support"], "supported");
    assert_eq!(
        wire["features"]["resume"]["reason"],
        "a restart ends the run"
    );
    let back: HarnessCapability = serde_json::from_value(wire).expect("parse back");
    assert_eq!(back.features(), Some(declared));
}

fn member(can_propose: Option<bool>, reads_web: Option<bool>) -> TeamMember {
    TeamMember {
        role: "researcher".to_owned(),
        model: None,
        can_propose,
        writes_project: None,
        reads_web,
        web_hosts: None,
    }
}

fn facts<'a>(network: Option<TeamNetwork>, members: Option<&'a [TeamMember]>) -> TeamFacts<'a> {
    TeamFacts {
        network,
        members,
        ..TeamFacts::default()
    }
}

#[test]
fn a_team_with_no_network_and_no_member_that_reads_the_web_cannot_serve_a_run_that_needs_it() {
    let web = TeamNeeds::of("researcher", true, None);
    let closed = Some(TeamNetwork::Closed);
    let none_read = [member(None, Some(false)), member(None, Some(false))];
    assert_eq!(
        team_misfits(facts(closed, Some(&none_read)), &web),
        [TeamMisfit::NoWeb]
    );
    // One member that reads the web, a network that is not closed, or a run that does not
    // need the web is enough.
    let one_reads = [member(None, Some(false)), member(None, Some(true))];
    assert!(team_misfits(facts(closed, Some(&one_reads)), &web).is_empty());
    assert!(team_misfits(facts(Some(TeamNetwork::Open), Some(&none_read)), &web).is_empty());
    assert!(team_misfits(facts(Some(TeamNetwork::Limited), Some(&none_read)), &web).is_empty());
    assert!(
        team_misfits(
            facts(closed, Some(&none_read)),
            &TeamNeeds::of("researcher", false, None)
        )
        .is_empty()
    );
}

#[test]
fn only_what_the_harness_said_is_a_misfit() {
    let needs = TeamNeeds {
        web: true,
        proposals: true,
        ..TeamNeeds::default()
    };
    let unsaid = [member(None, None)];
    assert_eq!(
        team_misfits(
            facts(None, Some(&[member(Some(false), Some(false))])),
            &needs
        ),
        [TeamMisfit::NoProposals],
        "a network the harness did not report is not a closed one"
    );
    let closed = Some(TeamNetwork::Closed);
    assert!(team_misfits(facts(closed, Some(&unsaid)), &needs).is_empty());
    assert!(team_misfits(facts(closed, None), &needs).is_empty());
    assert!(team_misfits(facts(closed, Some(&[])), &needs).is_empty());
}

#[test]
fn a_planner_needs_a_member_that_can_propose_and_a_researcher_does_not() {
    let none_can = [member(Some(false), None), member(Some(false), None)];
    let one_can = [member(Some(false), None), member(Some(true), None)];
    let planner = TeamNeeds::of("planner", false, None);
    assert_eq!(
        team_misfits(facts(None, Some(&none_can)), &planner),
        [TeamMisfit::NoProposals]
    );
    assert!(team_misfits(facts(None, Some(&one_can)), &planner).is_empty());
    assert!(
        team_misfits(
            facts(None, Some(&none_can)),
            &TeamNeeds::of("researcher", false, None)
        )
        .is_empty()
    );
    assert_eq!(
        serde_json::to_value([TeamMisfit::NoWeb, TeamMisfit::NoProposals]).expect("serialize"),
        serde_json::json!(["no_web", "no_proposals"])
    );
}

fn method(name: &str, missing_roles: &[&str]) -> TeamMethod {
    TeamMethod {
        name: name.to_owned(),
        source: "recipe".to_owned(),
        default: false,
        missing_roles: missing_roles
            .iter()
            .map(|role| (*role).to_owned())
            .collect(),
        error: None,
    }
}

fn need(kind: &str, name: &str, satisfied: bool) -> TeamNeed {
    TeamNeed {
        kind: kind.to_owned(),
        name: name.to_owned(),
        fix: "do it".to_owned(),
        satisfied,
        blocking: None,
    }
}

#[test]
fn a_run_that_changes_the_repository_does_not_fit_a_team_that_delivers_a_report() {
    let code = TeamNeeds::of("implementer", false, None);
    assert!(code.repository);
    let team = TeamFacts {
        delivers: Some(TeamDelivers::Report),
        ..TeamFacts::default()
    };
    assert_eq!(team_misfits(team, &code), [TeamMisfit::NoRepository]);
    // A Planner and a Researcher change nothing in the repository.
    for kind in ["planner", "researcher"] {
        assert!(team_misfits(team, &TeamNeeds::of(kind, false, None)).is_empty());
    }
    let branch = TeamFacts {
        delivers: Some(TeamDelivers::Branch),
        ..TeamFacts::default()
    };
    assert!(team_misfits(branch, &code).is_empty());
}

#[test]
fn a_method_the_team_lacks_or_cannot_run_in_full_is_a_misfit_with_its_reason() {
    let methods = [
        method("tdd", &[]),
        method("secure-build", &["security-vetter"]),
    ];
    let team = TeamFacts {
        methods: Some(&methods),
        ..TeamFacts::default()
    };
    let naming = |name| TeamNeeds::of("implementer", false, Some(name));
    assert_eq!(
        team_misfits(team, &naming("review")),
        [TeamMisfit::UnknownMethod {
            name: "review".to_owned()
        }]
    );
    assert_eq!(
        team_misfits(team, &naming("secure-build")),
        [TeamMisfit::StepsSkipped {
            method: "secure-build".to_owned(),
            roles: vec!["security-vetter".to_owned()],
        }]
    );
    assert!(team_misfits(team, &naming("tdd")).is_empty());
    // A method whose file does not load is listed with no missing role and is still refused.
    let broken = [TeamMethod {
        error: Some("YAML parse error".to_owned()),
        ..method("capture-broken", &[])
    }];
    let team = TeamFacts {
        methods: Some(&broken),
        ..TeamFacts::default()
    };
    assert_eq!(
        team_misfits(team, &naming("capture-broken")),
        [TeamMisfit::MethodBroken {
            method: "capture-broken".to_owned(),
            error: "YAML parse error".to_owned(),
        }]
    );
    assert_eq!(
        serde_json::to_value(TeamMisfit::UnknownMethod {
            name: "review".to_owned()
        })
        .expect("serialize"),
        serde_json::json!({"unknown_method": {"name": "review"}})
    );
}

#[test]
fn each_need_the_team_says_is_unmet_is_one_misfit() {
    let needs = [
        need("service", "smtp", false),
        need("secret", "api key", true),
        need("service", "calendar", false),
        need(NEED_MODEL, "a model", false),
    ];
    let team = TeamFacts {
        needs: Some(&needs),
        ..TeamFacts::default()
    };
    assert_eq!(
        team_misfits(team, &TeamNeeds::default()),
        [
            TeamMisfit::NeedsUnmet {
                name: "smtp".to_owned()
            },
            TeamMisfit::NeedsUnmet {
                name: "calendar".to_owned()
            },
        ],
        "a model is met by the run naming one, which is checked per run"
    );
}

#[test]
fn only_a_blocking_need_refuses_and_one_that_does_not_say_blocks() {
    let degrading = TeamNeed {
        blocking: Some(false),
        ..need("service", "pdf reader", false)
    };
    let blocking = TeamNeed {
        blocking: Some(true),
        ..need("service", "fetch hosts", false)
    };
    let silent = need("service", "smtp", false);
    let needs = [degrading, blocking, silent];
    let team = TeamFacts {
        needs: Some(&needs),
        ..TeamFacts::default()
    };
    assert_eq!(
        team_misfits(team, &TeamNeeds::default()),
        [
            TeamMisfit::NeedsUnmet {
                name: "fetch hosts".to_owned()
            },
            TeamMisfit::NeedsUnmet {
                name: "smtp".to_owned()
            },
        ],
        "an unmet need that docket says only degrades the work refuses nothing"
    );
}

#[test]
fn an_absent_contract_is_never_a_misfit() {
    let everything = TeamNeeds::of("implementer", true, Some("tdd"));
    let nothing_said = TeamFacts::default();
    assert!(team_misfits(nothing_said, &everything).is_empty());
    // A reported but empty list says nothing either, as an empty member list does.
    let empty = TeamFacts {
        methods: Some(&[]),
        needs: Some(&[]),
        ..TeamFacts::default()
    };
    assert!(team_misfits(empty, &everything).is_empty());
}

fn local(sign_in: Option<(SignInState, Option<&str>)>) -> HarnessCapability {
    serde_json::from_value(serde_json::json!({
        "harness_kind": "codex",
        "installed_version": "1",
        "probe_error": null,
        "probed_at": "2026-08-06T12:00:00Z",
        "sign_in": sign_in.map(|(state, command)| serde_json::json!({
            "state": state, "detail": "d", "command": command,
        })),
    }))
    .expect("capability")
}

#[test]
fn a_local_agent_is_a_team_of_one_that_delivers_a_branch() {
    let signed_out = local(Some((SignInState::SignedOut, Some("codex login"))));
    let team = TeamReport::of_agent("codex", &signed_out);
    assert!(team.is_of_agent("codex"));
    assert_eq!(team.delivers, Some(TeamDelivers::Branch));
    let methods = team.methods.as_deref().expect("one method");
    assert_eq!(methods.len(), 1);
    assert!(methods[0].default && methods[0].missing_roles.is_empty());
    let needs = team.needs.as_deref().expect("a sign-in need");
    assert_eq!(needs.len(), 1);
    assert!(!needs[0].satisfied);
    assert_eq!(needs[0].kind, NEED_SIGN_IN);
    assert!(needs[0].fix.contains("codex login"), "{}", needs[0].fix);
    assert_eq!(team.members.as_deref().map(<[_]>::len), Some(1));
    // Not said by a local agent: its supervision, where it works, its network.
    assert!(team.profile.is_none() && team.works_in.is_none() && team.network.is_none());
    assert_eq!(
        team.misfits(&TeamNeeds::of("implementer", false, None)),
        [TeamMisfit::NeedsUnmet {
            name: "signed in".to_owned()
        }]
    );
    // Signed in, the same need is met and nothing is refused.
    let signed_in = TeamReport::of_agent("codex", &local(Some((SignInState::SignedIn, None))));
    assert_eq!(
        signed_in.needs.as_deref().map(|n| n[0].satisfied),
        Some(true)
    );
    assert!(
        signed_in
            .misfits(&TeamNeeds::of("implementer", true, None))
            .is_empty()
    );
}

#[test]
fn a_local_agent_whose_sign_in_was_not_read_reports_no_needs() {
    for sign_in in [None, Some((SignInState::Unknown, None))] {
        let team = TeamReport::of_agent("codex", &local(sign_in));
        assert!(team.needs.is_none());
        assert!(
            team.misfits(&TeamNeeds::of("implementer", false, None))
                .is_empty()
        );
    }
}

#[test]
fn a_harness_that_wants_a_model_named_says_so_as_a_need_no_team_level_refusal_counts() {
    let mut capability = local(Some((SignInState::SignedIn, None)));
    capability.requires_model = true;
    capability.requires_model_reason = Some("name a model".to_owned());
    let team = TeamReport::of_agent("codex", &capability);
    let needs = team.needs.as_deref().expect("needs");
    assert_eq!(needs.len(), 2);
    assert!(needs.iter().any(|n| n.kind == NEED_MODEL && !n.satisfied));
    assert!(
        team.misfits(&TeamNeeds::of("implementer", false, None))
            .is_empty()
    );
    // A default model the agent names meets it.
    capability.default_model = Some("gpt".to_owned());
    assert_eq!(
        TeamReport::of_agent("codex", &capability)
            .needs
            .map(|n| n.len()),
        Some(1)
    );
}

#[test]
fn the_team_of_one_round_trips_and_a_docket_team_is_not_one() {
    let team = TeamReport::of_agent("codex", &local(Some((SignInState::SignedIn, None))));
    let wire = serde_json::to_value(&team).expect("serialize");
    assert!(wire.get("profile").is_none(), "absent, not null: {wire}");
    assert_eq!(wire["delivers"], "branch");
    let back: TeamReport = serde_json::from_value(wire).expect("parse back");
    assert_eq!(back, team);
    // A report from before the contract has none of the three keys.
    let old: TeamReport =
        serde_json::from_value(serde_json::json!({"name": "saas", "profile": "attended"}))
            .expect("an older report");
    assert!(old.delivers.is_none() && old.methods.is_none() && old.needs.is_none());
    assert!(!old.is_of_agent("saas"));
}
