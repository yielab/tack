use super::*;

async fn test_repo() -> Repository {
    let pool = crate::init_pool("sqlite::memory:")
        .await
        .expect("in-memory pool");
    crate::migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

/// Seed the built-ins (twice, to exercise the idempotent re-seed path too) and return
/// every `ProjectType::Construction` template.
async fn seeded_construction_templates(repo: &Repository) -> Vec<ProjectTemplate> {
    seed_builtin_templates(repo).await.expect("seed");
    seed_builtin_templates(repo).await.expect("re-seed");
    list_templates(repo, Some(ProjectType::Construction))
        .await
        .expect("list")
}

fn find_template<'a>(templates: &'a [ProjectTemplate], name: &str) -> &'a ProjectTemplate {
    templates
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("missing template {name}"))
}

#[tokio::test]
async fn construction_seed_creates_four_templates_once_each() {
    let repo = test_repo().await;
    let construction = seeded_construction_templates(&repo).await;

    for name in [
        "Construction Project",
        "Wood Frame Build",
        "Steel Frame Build",
        "SIP Panel Build",
    ] {
        let t = find_template(&construction, name);
        assert_eq!(t.project_type, ProjectType::Construction);
        assert!(t.is_builtin);
        assert_eq!(
            construction.iter().filter(|c| c.name == name).count(),
            1,
            "{name} duplicated by the re-seed"
        );
    }
}

#[tokio::test]
async fn wood_frame_keeps_construction_vocabulary() {
    let repo = test_repo().await;
    let construction = seeded_construction_templates(&repo).await;
    let wood = find_template(&construction, "Wood Frame Build");

    let default = wood.item_types.iter().find(|t| t.default_work).unwrap();
    assert_eq!(default.name, "Work Order");
    assert_eq!(
        wood.vocabulary.get("sprint").map(String::as_str),
        Some("Phase")
    );
}

#[tokio::test]
async fn wood_frame_has_stud_spacing_select_field() {
    let repo = test_repo().await;
    let construction = seeded_construction_templates(&repo).await;
    let wood = find_template(&construction, "Wood Frame Build");

    assert_eq!(wood.custom_fields.len(), 4);
    let stud = wood
        .custom_fields
        .iter()
        .find(|f| f.name == "Stud Spacing")
        .expect("stud spacing field");
    assert_eq!(stud.field_type, CustomFieldType::Select);
    assert_eq!(
        stud.options.as_deref(),
        Some(["16\" o.c.".to_string(), "24\" o.c.".to_string()].as_slice())
    );
}

#[tokio::test]
async fn sip_panel_template_has_panel_count_number_field() {
    let repo = test_repo().await;
    let construction = seeded_construction_templates(&repo).await;
    let sip = find_template(&construction, "SIP Panel Build");

    let panel_count = sip
        .custom_fields
        .iter()
        .find(|f| f.name == "Panel Count")
        .expect("panel count field");
    assert_eq!(panel_count.field_type, CustomFieldType::Number);
}

#[tokio::test]
async fn seeded_vertical_workflows_enforce_transitions() {
    let repo = test_repo().await;
    seed_builtin_templates(&repo).await.expect("seed");

    let construction = list_templates(&repo, Some(ProjectType::Construction))
        .await
        .expect("list");

    let steel = construction
        .iter()
        .find(|t| t.name == "Steel Frame Build")
        .unwrap();

    // Linear step is allowed.
    assert!(
        steel
            .workflow
            .validate_transition("Erection", "Decking/MEP")
            .is_ok()
    );
    // Rework loop back from Inspect is allowed.
    assert!(
        steel
            .workflow
            .validate_transition("Inspect", "Fireproofing")
            .is_ok()
    );
    // Illegal skip is rejected.
    assert!(
        steel
            .workflow
            .validate_transition("Permit", "Handover")
            .is_err()
    );
}

#[tokio::test]
async fn the_three_bundle_templates_are_listed_once_with_roles_profiles_and_level_three() {
    let repo = test_repo().await;
    seed_builtin_templates(&repo).await.expect("seed");
    seed_builtin_templates(&repo).await.expect("re-seed");
    let all = list_templates(&repo, None).await.expect("list");

    for (name, roles, profiles) in [
        ("Web product with content", 6, 6),
        ("Library or CLI", 4, 4),
        ("Static content site", 4, 4),
    ] {
        assert_eq!(all.iter().filter(|t| t.name == name).count(), 1, "{name}");
        let t = find_template(&all, name);
        assert_eq!(t.roles.len(), roles, "{name} roles");
        assert_eq!(t.profiles.len(), profiles, "{name} profiles");
        let level = t.automation.as_ref().and_then(|a| a.autonomy.as_ref());
        assert_eq!(level.and_then(|a| a.level.as_deref()), Some("L3"), "{name}");
        assert_eq!(level.and_then(|a| a.merge_on_done), Some(false), "{name}");
        for role in &t.roles {
            let profile = role.profile.as_deref().expect("a role names its profile");
            assert!(
                t.profiles.iter().any(|p| p.name == profile),
                "{name}: {profile}"
            );
        }
    }
}

#[tokio::test]
async fn digital_business_has_six_departments_that_name_no_agent_and_the_teams_it_expects() {
    let repo = test_repo().await;
    seed_builtin_templates(&repo).await.expect("seed");
    seed_builtin_templates(&repo).await.expect("re-seed");
    let all = list_templates(&repo, None).await.expect("list");
    assert_eq!(
        all.iter().filter(|t| t.name == "Digital business").count(),
        1
    );
    let t = find_template(&all, "Digital business");
    assert_eq!(t.roles.len(), 6);
    assert_eq!(t.profiles.len(), 6);
    for role in &t.roles {
        let profile = role.profile.as_deref().expect("a role names its profile");
        assert!(t.profiles.iter().any(|p| p.name == profile), "{profile}");
    }
    // None names an agent or a team, so the template works on whichever agent the runner has.
    for profile in &t.profiles {
        assert!(
            profile.tool_policy.get("agent").is_none(),
            "{}",
            profile.name
        );
        assert!(
            profile.tool_policy.get("docket").is_none(),
            "{}",
            profile.name
        );
    }
    let network: Vec<&str> = t
        .profiles
        .iter()
        .filter(|p| p.tool_policy["network"] == true)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        network,
        ["Support analyst", "Growth analyst", "Insight analyst"]
    );
    let shapes: Vec<&str> = t
        .teams
        .iter()
        .filter_map(|team| team.shape.as_deref())
        .collect();
    assert_eq!(shapes, ["build", "respond", "operate", "routine"]);
    // The other built-ins expect no team.
    assert!(find_template(&all, "Library or CLI").teams.is_empty());
}

#[tokio::test]
async fn a_saved_template_keeps_its_teams() {
    let repo = test_repo().await;
    let made = create_template(
        &repo,
        CreateProjectTemplate {
            item_types: None,
            name: "Mine".to_owned(),
            description: None,
            project_type: ProjectType::Custom,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
            teams: vec![BundleTeam {
                name: "fab-research".to_owned(),
                shape: Some("respond".to_owned()),
                delivers: None,
                methods: vec!["research-review".to_owned()],
            }],
        },
    )
    .await
    .expect("create");
    let read = get_template(&repo, made.id).await.expect("read back");
    assert_eq!(read.teams, made.teams);
    assert_eq!(read.teams[0].methods, ["research-review"]);
}
