use super::*;

async fn test_pool() -> SqlitePool {
    let pool = crate::init_pool("sqlite::memory:")
        .await
        .expect("in-memory pool");
    crate::migrations::run_all(&pool).await.expect("migrations");
    pool
}

/// Seed the built-ins (twice, to exercise the idempotent re-seed path too) and return
/// every `ProjectType::Construction` template.
async fn seeded_construction_templates(pool: &SqlitePool) -> Vec<ProjectTemplate> {
    seed_builtin_templates(pool).await.expect("seed");
    seed_builtin_templates(pool).await.expect("re-seed");
    list_templates(pool, Some(ProjectType::Construction))
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
    let pool = test_pool().await;
    let construction = seeded_construction_templates(&pool).await;

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
    let pool = test_pool().await;
    let construction = seeded_construction_templates(&pool).await;
    let wood = find_template(&construction, "Wood Frame Build");

    assert_eq!(
        wood.vocabulary.get("task").map(String::as_str),
        Some("Work Order")
    );
    assert_eq!(
        wood.vocabulary.get("sprint").map(String::as_str),
        Some("Phase")
    );
}

#[tokio::test]
async fn wood_frame_has_stud_spacing_select_field() {
    let pool = test_pool().await;
    let construction = seeded_construction_templates(&pool).await;
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
    let pool = test_pool().await;
    let construction = seeded_construction_templates(&pool).await;
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
    let pool = test_pool().await;
    seed_builtin_templates(&pool).await.expect("seed");

    let construction = list_templates(&pool, Some(ProjectType::Construction))
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
