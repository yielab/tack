use super::*;
use crate::item_types::preset;
use crate::models::{CodeOrigin, ItemSource, WorkspaceMode};
use crate::workflow::{kanban_workflow, scrum_workflow};

const MY_SAAS: &str = include_str!("../../../../tack-api/tests/fixtures/bundles/my-saas.yaml");
const HARNESSES: [&str; 4] = ["claude-code", "codex", "opencode", "docket"];

fn profile_row(name: &str, builtin: bool) -> ProfileRow {
    ProfileRow {
        id: format!("ap_{name}"),
        name: name.into(),
        kind: if builtin { "planner" } else { "custom" }.into(),
        instructions: "x".into(),
        tool_policy: serde_json::json!({}),
        limits: serde_json::json!({}),
        fleet_id: None,
        delivers: None,
        egress_mode: "listed".into(),
        builtin,
    }
}

/// A board that owns what `Board` borrows.
struct Owned {
    workflow: WorkflowConfig,
    fields: Vec<CustomFieldDefinition>,
    profiles: Vec<ProfileRow>,
    roles: Vec<(Uuid, String)>,
    sprints: Vec<(Uuid, String)>,
    fleets: Vec<(String, String)>,
    level: Level,
    auto_start: bool,
    item_types: Vec<ItemTypeDef>,
    new_project: bool,
}

impl Owned {
    fn empty() -> Self {
        Self {
            workflow: scrum_workflow(),
            fields: Vec::new(),
            profiles: vec![profile_row("Planner", true)],
            roles: Vec::new(),
            sprints: Vec::new(),
            fleets: Vec::new(),
            level: Level::L3,
            auto_start: false,
            item_types: preset(&ProjectType::Web),
            new_project: false,
        }
    }

    fn check(&self, outline: &Outline) -> Result<Plan, OutlineError> {
        check(
            outline,
            &Board {
                workflow: &self.workflow,
                fields: &self.fields,
                profiles: &self.profiles,
                roles: &self.roles,
                sprints: &self.sprints,
                fleets: &self.fleets,
                harness_kinds: &HARNESSES,
                level: self.level,
                auto_start: self.auto_start,
                item_types: &self.item_types,
                new_project: self.new_project,
            },
        )
    }
}

fn yaml(text: &str) -> Outline {
    serde_saphyr::from_str(text).expect("the outline parses")
}

#[derive(Clone, Copy)]
enum Expect {
    Problem(&'static str),
    Warning(&'static str),
}

#[test]
fn every_rule_of_the_check_speaks_in_its_own_section() {
    use Expect::{Problem, Warning};
    let rows: Vec<(&str, Expect)> = vec![
        (
            "project: { vocabulary: { nonsense: x } }",
            Problem("project.vocabulary: \"nonsense\""),
        ),
        (
            "project: { workflow: { preset: kanban, statuses: [{ name: A, category: todo }] } }",
            Problem("either a preset or statuses"),
        ),
        (
            "project: { workflow: { transitions: [] } }",
            Problem("give a preset"),
        ),
        (
            "project: { workflow: { preset: waterfall } }",
            Problem("preset \"waterfall\" is not one of scrum"),
        ),
        (
            "project: { workflow: { statuses: [{ name: A, category: later }] } }",
            Problem("status \"A\" has category \"later\""),
        ),
        (
            "fields: [{ name: F, type: colour }]",
            Problem("fields: \"F\" has type \"colour\""),
        ),
        (
            "fields: [{ name: F, type: select }]",
            Problem("fields: \"F\" is a select field and needs options"),
        ),
        (
            "fields: [{ name: F, type: text }, { name: F, type: text }]",
            Problem("fields: \"F\" is named twice"),
        ),
        (
            "profiles: [{ name: P, kind: wizard }]",
            Problem("profiles: \"P\" has kind \"wizard\""),
        ),
        (
            "profiles: [{ name: P, tool_policy: { agent: gpt } }]",
            Problem("profiles: \"P\": tool_policy.agent must be one of"),
        ),
        (
            "profiles: [{ name: P, tool_policy: { docket: { team: Bad_Team } } }]",
            Problem("profiles: \"P\": tool_policy.docket.team"),
        ),
        (
            "profiles: [{ name: P, tool_policy: { network: \"yes\" } }]",
            Problem("profiles: \"P\": tool_policy.network must be true or false"),
        ),
        (
            "profiles: [{ name: P, egress_mode: wide }]",
            Problem("profiles: \"P\": egress mode \"wide\""),
        ),
        (
            "profiles: [{ name: P, tool_policy: { allowed_hosts: [\"https://docs.rs\"] } }]",
            Problem("tool_policy.allowed_hosts"),
        ),
        (
            "profiles: [{ name: P, limits: { budgets: { tokens: 0 } } }]",
            Problem("profiles: \"P\": limits.budgets.tokens must be a positive whole number"),
        ),
        (
            "profiles: [{ name: P, limits: { budgets: { max_turns: 2 } } }]",
            Problem("profiles: \"P\": limits.budgets.max_turns is not a limit"),
        ),
        (
            "profiles: [{ name: P, fleet: Nowhere }]",
            Warning("profiles: \"P\": no fleet is named \"Nowhere\""),
        ),
        (
            "profiles: [{ name: Planner }]",
            Warning("profiles: \"Planner\" is built in"),
        ),
        (
            "teams: [{ name: Bad_Team }]",
            Problem("teams: \"Bad_Team\": the name must match"),
        ),
        (
            "teams: [{ name: fab, shape: department }]",
            Problem("teams: \"fab\" has shape \"department\""),
        ),
        (
            "teams: [{ name: fab, delivers: pdf }]",
            Problem("teams: \"fab\" delivers \"pdf\""),
        ),
        (
            "teams: [{ name: fab, methods: [Not_A_Method] }]",
            Problem("teams: \"fab\": method \"Not_A_Method\""),
        ),
        (
            "teams: [{ name: fab }, { name: fab }]",
            Problem("teams: \"fab\" appears more than once"),
        ),
        (
            "roles: [{ name: R, profile: Ghost }]",
            Problem("roles: \"R\" names profile \"Ghost\""),
        ),
        (
            "sprints: [{ name: S, status: active }]",
            Problem("sprints: \"S\" has status \"active\""),
        ),
        (
            "sprints: [{ name: S, start: tomorrow }]",
            Problem("sprints: \"S\": start \"tomorrow\" is not a date"),
        ),
        (
            "automation: { code_origin: moon }",
            Problem("automation.code_origin \"moon\""),
        ),
        (
            "automation: { workspace_mode: swim }",
            Problem("automation.workspace_mode \"swim\""),
        ),
        (
            "automation: { code_origin: folder, repository: relative/dir }",
            Problem("automation.repository must be an absolute path"),
        ),
        (
            "automation: { code_origin: url, repository: /tmp/x }",
            Problem("automation.repository must be a URL"),
        ),
        (
            "automation: { prepare: \"npm ci\\nrm -rf /\" }",
            Problem("automation.prepare must be a single line"),
        ),
        (
            "automation: { default_harness: gpt }",
            Problem("automation.default_harness \"gpt\""),
        ),
        (
            "automation: { default_profile: Ghost }",
            Problem("automation.default_profile \"Ghost\""),
        ),
        (
            "automation: { on_finish_status: Done }",
            Warning("automation.on_finish_status is ignored: the status is set by a person"),
        ),
        (
            "automation: { on_start_status: Nowhere }",
            Warning("automation.on_start_status is ignored: the status is set by a person"),
        ),
        (
            "automation: { auto_start: { status: Nowhere } }",
            Problem("automation.auto_start.status \"Nowhere\""),
        ),
        (
            "automation: { autonomy: { level: L9 } }",
            Problem("autonomy.level \"L9\" is not L3, L4 or L5"),
        ),
        (
            "automation: { auto_start: { max_open: 0 } }",
            Problem("automation.auto_start.max_open must be at least 1"),
        ),
        (
            "automation: { autonomy: { level: L4 } }",
            Warning("autonomy.level asked: L4; applied: L3. Raise it from Settings → Automation"),
        ),
        (
            "automation: { autonomy: { domains: [{ name: docs, level: L5 }] } }",
            Warning("autonomy.domains[docs].level asked: L5; applied: L3"),
        ),
        (
            "automation: { auto_start: { enabled: true } }",
            Warning(
                "auto_start asked: true; applied: false. Turn it on from Settings → Automation",
            ),
        ),
        (
            "items: [{ title: A, roles: [Ghost] }]",
            Problem("item 1 (\"A\"): role \"Ghost\""),
        ),
        (
            "items: [{ title: A, sprint: Ghost }]",
            Problem("item 1 (\"A\"): sprint \"Ghost\""),
        ),
        (
            "items: [{ title: A, fields: { Ghost: 1 } }]",
            Problem("item 1 (\"A\"): field \"Ghost\""),
        ),
        (
            "fields: [{ name: Edad, type: select, options: [\"3-5\"] }]\nitems: [{ title: La luna, fields: { Edad: \"12\" } }]",
            Problem("item 1 (\"La luna\"): field 'Edad': '12' is not a valid option"),
        ),
        (
            "fields: [{ name: Edad, type: text, required: true }]\nitems: [{ title: A }]",
            Warning("item 1 (\"A\"): required field \"Edad\" has no value"),
        ),
    ];
    let board = Owned::empty();
    for (text, expect) in rows {
        let result = board.check(&yaml(text));
        match (expect, result) {
            (Problem(wanted), Err(e)) => assert!(
                e.problems.iter().any(|p| p.contains(wanted)),
                "{text}: wanted a problem with \"{wanted}\", got {:?}",
                e.problems
            ),
            (Problem(wanted), Ok(_)) => {
                panic!("{text}: wanted a problem with \"{wanted}\", got a plan")
            }
            (Warning(wanted), Ok(plan)) => assert!(
                plan.warnings.iter().any(|w| w.contains(wanted)),
                "{text}: wanted a warning with \"{wanted}\", got {:?}",
                plan.warnings
            ),
            (Warning(wanted), Err(e)) => panic!(
                "{text}: wanted a warning with \"{wanted}\", got problems {:?}",
                e.problems
            ),
        }
    }
}

#[test]
fn the_my_saas_file_checks_clean_but_for_its_three_autonomy_warnings() {
    let outline = yaml(MY_SAAS);
    let mut board = Owned::empty();
    board.workflow = workflow_of(outline.project.as_ref(), &ProjectType::Web).expect("workflow");
    let plan = board.check(&outline).expect("the fixture checks clean");
    assert_eq!(plan.warnings.len(), 3, "{:?}", plan.warnings);
    assert!(plan.warnings[0].starts_with("autonomy.level asked: L4; applied: L3."));
    assert!(
        plan.warnings[1].starts_with("autonomy.domains[content].level asked: L4; applied: L3.")
    );
    assert!(plan.warnings[2].starts_with("auto_start asked: true; applied: false."));
    let automation = plan.automation.expect("automation");
    assert_eq!(automation.auto_start.map(|a| a.enabled), Some(Some(false)));
    assert_eq!(
        automation.autonomy.and_then(|a| a.level).as_deref(),
        Some("L3")
    );
    let names: Vec<&str> = plan.sections.iter().map(|s| s.section.as_str()).collect();
    assert_eq!(
        names,
        [
            "types",
            "fields",
            "profiles",
            "roles",
            "sprints",
            "automation",
            "items"
        ]
    );
    assert_eq!(plan.items.items.len(), 20);
}

fn type_of(plan: &Plan, index: usize) -> Option<&str> {
    plan.items.items[index].input.item_type.as_deref()
}

#[test]
fn an_items_word_is_a_key_or_a_name_of_the_catalog_in_any_case() {
    let outline = yaml(
        "items:\n  - {title: A, type: BUG}\n  - {title: B, type: Feature}\n  - {title: C}\n  \
         - {title: D, type: epic}\n",
    );
    let plan = Owned::empty().check(&outline).expect("checks clean");
    assert_eq!(type_of(&plan, 0), Some("bug"));
    assert_eq!(type_of(&plan, 1), Some("feature"));
    assert_eq!(
        type_of(&plan, 2),
        None,
        "no word: the store gives the default"
    );
    assert_eq!(type_of(&plan, 3), Some("epic"));
    assert!(plan.types.is_none() && plan.new_types.is_empty());
}

#[test]
fn a_word_the_catalog_lacks_is_added_as_work_and_listed() {
    let outline = yaml("items:\n  - {title: A, type: Spike}\n  - {title: B, type: spike}\n");
    let plan = Owned::empty().check(&outline).expect("checks clean");
    assert_eq!(plan.new_types, ["Spike"]);
    assert_eq!(type_of(&plan, 0), Some("spike"));
    assert_eq!(type_of(&plan, 1), Some("spike"));
    let types = plan.types.expect("the catalog changes");
    let spike = types.iter().find(|t| t.key == "spike").unwrap();
    assert_eq!(spike.level, item_types::Level::Work);
    assert_eq!(spike.name, "Spike");
    assert!(!spike.default_work);
    assert_eq!(types.iter().filter(|t| t.default_work).count(), 1);
}

#[test]
fn a_new_project_takes_the_files_types_whole_and_an_existing_one_gains_the_missing() {
    let file = "types:\n  - {key: building, name: Building, level: group, color: accent}\n  \
                - {key: order, name: Work Order, level: work, color: neutral, default: true}\n\
                items:\n  - {title: A, type: building}\n";
    let mut board = Owned::empty();
    board.new_project = true;
    board.item_types = Vec::new();
    let plan = board.check(&yaml(file)).expect("checks clean");
    let keys: Vec<&str> = plan
        .types
        .as_ref()
        .unwrap()
        .iter()
        .map(|t| t.key.as_str())
        .collect();
    assert_eq!(keys, ["building", "order"]);
    assert_eq!(type_of(&plan, 0), Some("building"));

    let renamed = "types:\n  - {key: epic, name: Saga, level: group, color: danger}\n  \
                   - {key: order, name: Order, level: work, color: neutral}\n";
    let plan = Owned::empty().check(&yaml(renamed)).expect("checks clean");
    let types = plan.types.unwrap();
    let epic = types.iter().find(|t| t.key == "epic").unwrap();
    assert_eq!(
        (epic.name.as_str(), epic.color),
        ("Saga", TypeColor::Danger)
    );
    assert!(types.iter().any(|t| t.key == "order" && !t.default_work));
    assert_eq!(types.iter().filter(|t| t.default_work).count(), 1);
}

#[test]
fn the_files_types_are_checked_like_a_catalog() {
    let mut board = Owned::empty();
    board.new_project = true;
    let no_default = "types:\n  - {key: a, name: A, level: work, color: neutral}\n";
    assert!(board.check(&yaml(no_default)).is_err());
    let group_default =
        "types:\n  - {key: a, name: A, level: group, color: neutral, default: true}\n";
    assert!(board.check(&yaml(group_default)).is_err());
    let twice = "types:\n  - {key: a, name: A, level: work, color: neutral, default: true}\n  \
                 - {key: a, name: B, level: work, color: neutral}\n";
    let err = board.check(&yaml(twice)).unwrap_err();
    assert!(
        err.problems.iter().any(|p| p.contains("used twice")),
        "{err}"
    );
    let bad_key =
        "types:\n  - {key: Bad Key, name: A, level: work, color: neutral, default: true}\n";
    assert!(board.check(&yaml(bad_key)).is_err());
}

#[test]
fn the_export_writes_the_catalog_and_the_file_reads_it_back() {
    let project = synthetic_project();
    let parts = Parts {
        project: &project,
        fields: &[],
        profiles: &[],
        roles: &[],
        sprints: &[],
        autonomy: None,
        auto_start: BundleAutoStart::default(),
        items: &[],
        item_roles: &[],
        item_fields: &[],
        dependencies: &[],
        fleets: &[],
        reported_teams: &[],
    };
    let written = to_bundle(&parts);
    let text = serde_json::to_string(&written).unwrap();
    let back: Outline = serde_json::from_str(&text).unwrap();
    let types: Vec<ItemTypeDef> = back
        .types
        .iter()
        .enumerate()
        .map(|(i, t)| ItemTypeDef {
            key: t.key.clone(),
            name: t.name.clone(),
            level: t.level,
            color: t.color,
            order: i as u32,
            default_work: t.default,
        })
        .collect();
    assert_eq!(types, project.item_types);
}

#[test]
fn an_auto_start_block_without_the_switch_leaves_the_switch_alone() {
    let outline = yaml("automation:\n  auto_start:\n    status: To Do\n    max_open: 2\n");
    let board = Owned::empty();
    let plan = board.check(&outline).expect("checks clean");
    let start = plan
        .automation
        .and_then(|a| a.auto_start)
        .expect("auto_start");
    assert_eq!(start.enabled, None);
    assert_eq!(start.max_open, Some(2));
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
}

fn synthetic_project() -> Project {
    let now = Utc::now();
    Project {
        id: Uuid::new_v4(),
        workspace_id: Uuid::new_v4(),
        name: "My SaaS".into(),
        description: None,
        project_type: ProjectType::Web,
        vocabulary: vocabulary_for_type(&ProjectType::Web),
        item_types: preset(&ProjectType::Web),
        workflow: scrum_workflow(),
        default_model: None,
        github_token_ref: None,
        created_at: now,
        updated_at: now,
        archived: false,
        code_origin: CodeOrigin::None,
        repository: None,
        default_branch: None,
        workspace_mode: WorkspaceMode::LocalBranch,
        push_after_run: false,
        default_harness: None,
        default_profile_id: None,
        definition_of_done: None,
        auto_start_enabled: false,
        auto_start_status: None,
        auto_start_max_open: 3,
        prepare_command: None,
        auto_start_disabled_reason: None,
        agents_on_board: Default::default(),
    }
}

fn synthetic_item(project: &Project, title: &str, parent: Option<Uuid>) -> Item {
    let now = Utc::now();
    Item {
        id: Uuid::new_v4(),
        project_id: project.id,
        parent_id: parent,
        title: title.into(),
        description: None,
        item_type: "task".into(),
        status: "To Do".into(),
        priority: Priority::Medium,
        estimate: None,
        estimate_unit: EstimateUnit::StoryPoints,
        tags: Vec::new(),
        sort_order: 0,
        sprint_id: None,
        assignee: None,
        due_date: None,
        recurrence: None,
        recurrence_of: None,
        started_at: None,
        completed_at: None,
        source: ItemSource::Manual,
        source_artifact_id: None,
        run_settings: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn the_export_numbers_items_parents_first_and_leaves_ids_and_dates_out() {
    let project = synthetic_project();
    // the child comes first in the slice; its parent must still be "1"
    let parent_id = Uuid::new_v4();
    let mut parent = synthetic_item(&project, "Colección", None);
    parent.id = parent_id;
    let child = synthetic_item(&project, "Historia", Some(parent_id));
    let items = vec![child.clone(), parent.clone()];
    let now = Utc::now();
    let role = Role {
        id: Uuid::new_v4(),
        project_id: project.id,
        name: "Contenido".into(),
        color: "#f59e0b".into(),
        icon: None,
        agent_profile_id: None,
        created_at: now,
    };
    let field = CustomFieldDefinition {
        id: Uuid::new_v4(),
        project_id: Some(project.id),
        name: "Dialecto".into(),
        field_type: CustomFieldType::Text,
        description: None,
        required: false,
        default_value: None,
        options: None,
        validation: None,
        created_at: now,
        updated_at: now,
    };
    let value = CustomFieldValue {
        id: Uuid::new_v4(),
        item_id: child.id,
        field_id: field.id,
        value: serde_json::json!("rioplatense"),
        created_at: now,
        updated_at: now,
    };
    let blocks = Dependency {
        id: Uuid::new_v4(),
        source_item_id: parent.id,
        target_item_id: child.id,
        dependency_type: DependencyType::Blocks,
        created_by: None,
        created_at: now,
    };
    let (roles, fields, item_roles, values, dependencies) = (
        [role.clone()],
        [field],
        [(child.id, role.id)],
        [value],
        [blocks],
    );
    let outline = to_bundle(&Parts {
        project: &project,
        fields: &fields,
        profiles: &[],
        roles: &roles,
        sprints: &[],
        autonomy: None,
        auto_start: BundleAutoStart::default(),
        items: &items,
        item_roles: &item_roles,
        item_fields: &values,
        dependencies: &dependencies,
        fleets: &[],
        reported_teams: &[],
    });
    assert_eq!(outline.items[0].reference.as_deref(), Some("1"));
    assert_eq!(outline.items[0].title, "Colección");
    assert_eq!(outline.items[1].reference.as_deref(), Some("2"));
    assert_eq!(outline.items[1].parent.as_deref(), Some("1"));
    assert_eq!(outline.items[1].depends_on, ["1"]);
    assert_eq!(outline.items[1].roles, ["Contenido"]);
    assert_eq!(
        outline.items[1].fields["Dialecto"],
        serde_json::json!("rioplatense")
    );

    let text = serde_saphyr::to_string(&outline).expect("yaml");
    for line in text.lines() {
        let key = line
            .trim_start_matches([' ', '-'])
            .split(':')
            .next()
            .unwrap_or("");
        assert!(key != "id" && !key.ends_with("_at"), "{line}");
    }
    assert_eq!(text, serde_saphyr::to_string(&outline).expect("yaml again"));

    // the export is a file the check accepts on an empty board
    let mut board = Owned::empty();
    board.workflow = project.workflow.clone();
    board.check(&outline).expect("the export checks clean");
}

#[test]
fn a_preset_is_the_preset_and_statuses_are_a_custom_workflow() {
    let preset = yaml("project: { workflow: { preset: Kanban } }");
    let workflow = workflow_of(preset.project.as_ref(), &ProjectType::Web).expect("preset");
    assert_eq!(
        serde_json::to_value(&workflow).unwrap(),
        serde_json::to_value(kanban_workflow()).unwrap()
    );
    let statuses = yaml(
        "project: { workflow: { statuses: [{ name: A, category: todo }, { name: B, category: in_progress }, { name: C, category: done }] } }",
    );
    let workflow = workflow_of(statuses.project.as_ref(), &ProjectType::Web).expect("statuses");
    assert_eq!(workflow.workflow_type, WorkflowType::Custom);
    assert_eq!(workflow.status_names(), ["A", "B", "C"]);
}

#[test]
fn the_install_setting_round_trips_through_the_file() {
    let mut board = Owned::empty();
    for stored in [None, Some(""), Some("make deps")] {
        let mut project = synthetic_project();
        project.prepare_command = stored.map(str::to_owned);
        board.workflow = project.workflow.clone();
        let outline = to_bundle(&Parts {
            project: &project,
            fields: &[],
            profiles: &[],
            roles: &[],
            sprints: &[],
            autonomy: None,
            auto_start: BundleAutoStart::default(),
            items: &[],
            item_roles: &[],
            item_fields: &[],
            dependencies: &[],
            fleets: &[],
            reported_teams: &[],
        });
        let text = serde_saphyr::to_string(&outline).expect("yaml");
        let read: Outline = serde_saphyr::from_str(&text).expect("the export parses");
        let plan = board.check(&read).expect("the export checks clean");
        let prepare = plan.automation.and_then(|a| a.prepare);
        assert_eq!(prepare.as_deref(), stored, "{text}");
    }
}

#[test]
fn teams_are_described_and_an_import_creates_nothing_from_them() {
    let outline = yaml(
        "teams:\n  - { name: fab-research, shape: respond, delivers: report, methods: [research-review] }\n",
    );
    assert_eq!(outline.teams.len(), 1);
    assert_eq!(outline.teams[0].shape.as_deref(), Some("respond"));
    let plan = Owned::empty()
        .check(&outline)
        .expect("a file with teams checks clean");
    assert!(
        plan.sections.is_empty(),
        "no section creates or updates anything for teams"
    );
    // The bundle still refuses a key it does not know, inside a team too.
    let unknown: Result<Outline, _> = serde_saphyr::from_str("teams: [{ name: fab, colour: red }]");
    assert!(unknown.is_err());
}

#[test]
fn the_export_writes_the_teams_its_profiles_name_with_what_the_computers_report() {
    let mut board = Owned::empty();
    let project = synthetic_project();
    board.workflow = project.workflow.clone();
    let named = |name: &str, team: Option<&str>| {
        let mut row = profile_row(name, false);
        row.id = format!("ap_{name}");
        row.tool_policy = match team {
            Some(team) => serde_json::json!({"agent": "docket", "docket": {"team": team}}),
            None => serde_json::json!({}),
        };
        row
    };
    let profiles = [
        named("Research", Some("fab-research")),
        named("Support", Some("fab-support")),
        named("Plain", None),
    ];
    let roles: Vec<Role> = profiles
        .iter()
        .map(|profile| Role {
            id: Uuid::new_v4(),
            project_id: project.id,
            name: profile.name.clone(),
            color: "#f59e0b".into(),
            icon: None,
            agent_profile_id: Some(profile.id.clone()),
            created_at: Utc::now(),
        })
        .collect();
    let reported = [BundleTeam {
        name: "fab-research".to_owned(),
        shape: Some("respond".to_owned()),
        delivers: Some("report".to_owned()),
        methods: vec!["research-review".to_owned()],
    }];
    let outline = to_bundle(&Parts {
        project: &project,
        fields: &[],
        profiles: &profiles,
        roles: &roles,
        sprints: &[],
        autonomy: None,
        auto_start: BundleAutoStart::default(),
        items: &[],
        item_roles: &[],
        item_fields: &[],
        dependencies: &[],
        fleets: &[],
        reported_teams: &reported,
    });
    let names: Vec<&str> = outline
        .teams
        .iter()
        .map(|team| team.name.as_str())
        .collect();
    assert_eq!(names, ["fab-research", "fab-support"]);
    assert_eq!(outline.teams[0].methods, ["research-review"]);
    assert_eq!(
        outline.teams[1].shape, None,
        "a team nobody reports is written by name alone"
    );
    // And the export reads back and checks clean.
    let text = serde_saphyr::to_string(&outline).expect("yaml");
    let read: Outline = serde_saphyr::from_str(&text).expect("the export parses");
    assert_eq!(read.teams, outline.teams);
    board.check(&read).expect("the export checks clean");
}
