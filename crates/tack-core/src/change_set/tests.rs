use super::*;
use crate::hierarchy::Entry;
use crate::item_types::{Level, preset};
use crate::models::ProjectType;
use crate::principal::Scope;
use crate::workflow::simple_workflow;

fn board(items: &[(Uuid, &str)]) -> BoardView {
    let workflow = simple_workflow();
    let mut occupied: HashMap<String, usize> = HashMap::new();
    for (_, status) in items {
        *occupied.entry((*status).to_string()).or_default() += 1;
    }
    BoardView {
        workflow,
        item_types: preset(&ProjectType::Software),
        items: items
            .iter()
            .map(|(id, status)| {
                (
                    *id,
                    ExistingItem {
                        status: (*status).to_string(),
                    },
                )
            })
            .collect(),
        occupied,
        edges: Vec::new(),
        roles: vec![("Backend".into(), Uuid::new_v4())],
        profiles: ["p-code".to_string()].into(),
        profiles_by_team: [("code".to_string(), vec!["p-code".to_string()])].into(),
        forest: {
            let mut forest = Forest::new(Uuid::nil());
            for (id, _) in items {
                forest.insert(*id, entry(None, Level::Work, "Task"));
            }
            forest
        },
        anchor_type: None,
    }
}

fn entry(parent: Option<Uuid>, level: Level, name: &str) -> Entry {
    Entry {
        parent,
        title: name.to_owned(),
        type_name: name.to_owned(),
        level,
    }
}

fn create(title: &str) -> CreateOp {
    CreateOp {
        title: title.into(),
        description: None,
        acceptance: Vec::new(),
        depends_on: Vec::new(),
        item_type: None,
        role: None,
        team: None,
        profile_id: None,
        parent: None,
    }
}

fn all(operations: &[Operation]) -> Vec<usize> {
    (0..operations.len()).collect()
}

fn reasons(problems: &[Problem]) -> Vec<String> {
    problems.iter().map(Problem::to_string).collect()
}

#[test]
fn a_clean_set_resolves_every_create() {
    let existing = Uuid::new_v4();
    let operations = vec![
        Operation::Create(CreateOp {
            role: Some("backend".into()),
            team: Some("code".into()),
            item_type: Some("bug".into()),
            ..create("Fix it")
        }),
        Operation::Comment {
            item: ItemRef::New(0),
            text: "why".into(),
        },
        Operation::Depend {
            blocker: ItemRef::New(0),
            blocked: ItemRef::Existing(existing),
        },
    ];
    let board = board(&[(existing, "To Do")]);
    let resolved = validate(&operations, &all(&operations), &board, &HashMap::new()).unwrap();
    let first = &resolved[&0];
    assert_eq!(first.item_type.as_deref(), Some("bug"));
    assert_eq!(first.profile.as_deref(), Some("p-code"));
    assert!(first.role.is_some());
}

#[test]
fn a_create_names_what_the_project_lacks_and_is_still_valid() {
    let operations = vec![Operation::Create(CreateOp {
        role: Some("Ghost".into()),
        team: Some("nobody".into()),
        item_type: Some("saga".into()),
        profile_id: Some("p-none".into()),
        ..create("Odd")
    })];
    let resolved = validate(&operations, &[0], &board(&[]), &HashMap::new()).unwrap();
    let first = &resolved[&0];
    assert_eq!(first.unknown_role.as_deref(), Some("Ghost"));
    assert_eq!(first.unknown_type.as_deref(), Some("saga"));
    assert_eq!(first.unknown_profile.as_deref(), Some("p-none"));
    assert_eq!(first.unresolved_team.as_deref(), Some("nobody"));
    assert_eq!(first.item_type.as_deref(), Some("task"));
}

#[test]
fn a_cycle_a_missing_status_and_a_full_column_are_all_reasons_at_once() {
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mut board = board(&[(a, "To Do"), (b, "To Do"), (c, "Doing")]);
    board.edges.push((a, b));
    board.workflow.statuses.iter_mut().for_each(|status| {
        if status.name == "Doing" {
            status.wip_limit = Some(1);
        }
    });
    let operations = vec![
        // b blocking a closes the loop a -> b -> a.
        Operation::Depend {
            blocker: ItemRef::Existing(b),
            blocked: ItemRef::Existing(a),
        },
        Operation::Move {
            item: ItemRef::Existing(a),
            status: "nowhere".into(),
        },
        // in_progress already holds c and its limit is 1.
        Operation::Move {
            item: ItemRef::Existing(b),
            status: "Doing".into(),
        },
        Operation::WhoDoesIt {
            item: ItemRef::Existing(a),
            role: Some("Ghost".into()),
            team: None,
            profile_id: None,
        },
    ];
    let problems = validate(&operations, &all(&operations), &board, &HashMap::new()).unwrap_err();
    let text = reasons(&problems);
    assert_eq!(problems.len(), 4, "{text:#?}");
    assert!(text[0].starts_with("operation 0:") && text[0].contains("loop"));
    assert!(text[1].starts_with("operation 1:") && text[1].contains("nowhere"));
    assert!(text[2].starts_with("operation 2:") && text[2].contains("WIP"));
    assert!(text[3].starts_with("operation 3:") && text[3].contains("Ghost"));
    assert!(describe(&problems).contains("4 reasons"));
}

#[test]
fn moves_count_the_room_earlier_operations_take() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut board = board(&[(a, "To Do"), (b, "To Do")]);
    board.workflow.statuses.iter_mut().for_each(|status| {
        if status.name == "Doing" {
            status.wip_limit = Some(1);
        }
    });
    let operations = vec![
        Operation::Move {
            item: ItemRef::Existing(a),
            status: "Doing".into(),
        },
        Operation::Move {
            item: ItemRef::Existing(b),
            status: "Doing".into(),
        },
    ];
    let problems = validate(&operations, &[0, 1], &board, &HashMap::new()).unwrap_err();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].operation, Some(1));
}

#[test]
fn creates_that_loop_through_depends_on_are_refused() {
    let operations = vec![
        Operation::Create(CreateOp {
            depends_on: vec![ItemRef::New(1)],
            ..create("a")
        }),
        Operation::Create(CreateOp {
            depends_on: vec![ItemRef::New(0)],
            ..create("b")
        }),
    ];
    let problems = validate(&operations, &[0, 1], &board(&[]), &HashMap::new()).unwrap_err();
    assert!(reasons(&problems)[0].contains("loop"));
}

#[test]
fn a_parent_is_an_earlier_create_and_an_item_of_another_project_is_not_found() {
    let outsider = Uuid::new_v4();
    let operations = vec![
        Operation::Create(CreateOp {
            parent: Some(ItemRef::New(1)),
            ..create("child")
        }),
        Operation::Create(create("parent")),
        Operation::Priority {
            item: ItemRef::Existing(outsider),
            priority: Priority::High,
        },
    ];
    let problems = validate(&operations, &[0, 1, 2], &board(&[]), &HashMap::new()).unwrap_err();
    let text = reasons(&problems);
    assert!(text[0].contains("earlier operation"), "{text:?}");
    assert!(text[1].contains("not in this project"), "{text:?}");
}

#[test]
fn applying_part_of_a_set_needs_the_creates_it_names_to_be_applied() {
    let operations = vec![
        Operation::Create(create("a")),
        Operation::Comment {
            item: ItemRef::New(0),
            text: "x".into(),
        },
    ];
    let board = board(&[]);
    let problems = validate(&operations, &[1], &board, &HashMap::new()).unwrap_err();
    assert!(reasons(&problems)[0].contains("not applied"));
    let made = Uuid::new_v4();
    let done = HashMap::from([(0, made)]);
    // The create was applied before; its item exists in the store, so the view has it.
    let mut board = board;
    board.items.insert(
        made,
        ExistingItem {
            status: "To Do".into(),
        },
    );
    assert!(validate(&operations, &[1], &board, &done).is_ok());
}

#[test]
fn an_empty_set_and_a_blank_title_are_refused() {
    assert!(validate(&[], &[], &board(&[]), &HashMap::new()).is_err());
    let blank = vec![Operation::Create(create("  "))];
    assert!(validate(&blank, &[0], &board(&[]), &HashMap::new()).is_err());
}

fn agent(project: Uuid, capabilities: &[Capability]) -> Principal {
    Principal::agent(
        "cred",
        vec![Scope {
            all_projects: false,
            projects: vec![project],
            capabilities: capabilities.to_vec(),
        }],
    )
}

fn mixed() -> Vec<Operation> {
    vec![
        Operation::Create(create("a")),
        Operation::Create(create("b")),
        Operation::Priority {
            item: ItemRef::Existing(Uuid::new_v4()),
            priority: Priority::High,
        },
        Operation::Comment {
            item: ItemRef::New(0),
            text: "about a".into(),
        },
    ]
}

fn rung(level: RungLevel, per_set: u32, per_day: u32) -> AgentsOnBoard {
    AgentsOnBoard {
        level,
        creates_per_change_set: per_set,
        creates_per_day: per_day,
    }
}

#[test]
fn at_propose_everything_waits() {
    let verdicts = decide(
        &rung(RungLevel::Propose, 5, 5),
        RungLevel::Create,
        &mixed(),
        0,
    )
    .unwrap();
    assert!(verdicts.iter().all(|v| *v == Verdict::Wait));
}

#[test]
fn at_act_creates_wait_and_the_rest_applies_unless_it_needs_a_create() {
    let verdicts = decide(&rung(RungLevel::Act, 5, 5), RungLevel::Create, &mixed(), 0).unwrap();
    assert_eq!(
        verdicts,
        [Verdict::Wait, Verdict::Wait, Verdict::Apply, Verdict::Wait]
    );
}

#[test]
fn at_create_creates_apply_inside_the_caps_and_wait_beyond_them() {
    let at = |per_set, per_day, today| {
        decide(
            &rung(RungLevel::Create, per_set, per_day),
            RungLevel::Create,
            &mixed(),
            today,
        )
        .unwrap()
    };
    assert_eq!(
        at(2, 10, 0),
        [
            Verdict::Apply,
            Verdict::Apply,
            Verdict::Apply,
            Verdict::Apply
        ]
    );
    // One create per set: the second waits, the comment on the first still applies.
    assert_eq!(
        at(1, 10, 0),
        [
            Verdict::Apply,
            Verdict::Wait,
            Verdict::Apply,
            Verdict::Apply
        ]
    );
    // The day's room is used up by earlier sets.
    assert_eq!(
        at(5, 3, 2),
        [
            Verdict::Apply,
            Verdict::Wait,
            Verdict::Apply,
            Verdict::Apply
        ]
    );
    // No caps, no creates at once: Create without caps is Act.
    assert_eq!(
        at(0, 0, 0),
        [Verdict::Wait, Verdict::Wait, Verdict::Apply, Verdict::Wait]
    );
}

#[test]
fn a_dependent_waits_with_the_create_it_needs() {
    let operations = vec![
        Operation::Create(create("a")),
        Operation::Create(CreateOp {
            parent: Some(ItemRef::New(0)),
            ..create("b")
        }),
    ];
    let verdicts = decide(
        &rung(RungLevel::Create, 1, 10),
        RungLevel::Create,
        &operations,
        0,
    )
    .unwrap();
    assert_eq!(verdicts, [Verdict::Apply, Verdict::Wait]);
}

#[test]
fn the_stricter_of_the_rung_and_the_credential_wins() {
    let project = Uuid::new_v4();
    let create_rung = rung(RungLevel::Create, 5, 5);
    let proposer = agent(project, &[Capability::Read, Capability::Propose]);
    assert_eq!(ceiling_of(&proposer, project), RungLevel::Propose);
    let verdicts = decide(&create_rung, ceiling_of(&proposer, project), &mixed(), 0).unwrap();
    assert!(verdicts.iter().all(|v| *v == Verdict::Wait));
    let actor = agent(project, &[Capability::Act]);
    assert_eq!(ceiling_of(&actor, project), RungLevel::Create);
    let verdicts = decide(
        &rung(RungLevel::Propose, 5, 5),
        ceiling_of(&actor, project),
        &mixed(),
        0,
    )
    .unwrap();
    assert!(verdicts.iter().all(|v| *v == Verdict::Wait));
    let reader = agent(project, &[Capability::Read]);
    assert!(decide(&create_rung, ceiling_of(&reader, project), &mixed(), 0).is_err());
}

#[test]
fn a_project_at_read_accepts_no_proposal() {
    let error = decide(&rung(RungLevel::Read, 0, 0), RungLevel::Create, &mixed(), 0).unwrap_err();
    assert!(error.contains("read and talk only"));
}

#[test]
fn the_default_rung_is_propose_with_no_creates() {
    let default = AgentsOnBoard::default();
    assert_eq!(default.level, RungLevel::Propose);
    assert_eq!(
        (default.creates_per_change_set, default.creates_per_day),
        (0, 0)
    );
}

#[test]
fn a_status_follows_its_operations() {
    let op = |state| ChangeOp {
        operation: Operation::Create(create("x")),
        state,
        result: None,
    };
    use OpState::*;
    assert_eq!(
        status_of(&[op(Waiting), op(Waiting)]),
        ChangeSetStatus::Waiting
    );
    assert_eq!(
        status_of(&[op(Applied), op(Applied)]),
        ChangeSetStatus::Applied
    );
    assert_eq!(
        status_of(&[op(Rejected), op(Rejected)]),
        ChangeSetStatus::Rejected
    );
    assert_eq!(
        status_of(&[op(Applied), op(Waiting)]),
        ChangeSetStatus::Partly
    );
    assert_eq!(
        status_of(&[op(Applied), op(Rejected)]),
        ChangeSetStatus::Partly
    );
}

#[test]
fn operations_and_refs_read_as_json() {
    let text = r#"[
        {"op":"create","title":"T","parent":"7e4c6f8e-1c64-4d61-9a0e-5c1c2f3a9b10"},
        {"op":"move","item":0,"status":"Done"},
        {"op":"due","item":0,"due":null}
    ]"#;
    let operations: Vec<Operation> = serde_json::from_str(text).unwrap();
    assert!(
        matches!(&operations[0], Operation::Create(c) if matches!(c.parent, Some(ItemRef::Existing(_))))
    );
    assert_eq!(operations[1].needs(), vec![0]);
    assert!(matches!(operations[2], Operation::Due { due: None, .. }));
}

#[test]
fn a_set_that_would_break_the_tree_is_refused_with_every_reason() {
    let (epic, story, task) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mut board = board(&[(epic, "To Do"), (story, "To Do"), (task, "To Do")]);
    board.forest.insert(epic, entry(None, Level::Group, "Epic"));
    board
        .forest
        .insert(story, entry(Some(epic), Level::Work, "Story"));
    board
        .forest
        .insert(task, entry(Some(story), Level::Work, "Task"));
    let under = |item: Uuid, parent: Option<Uuid>| Operation::Parent {
        item: ItemRef::Existing(item),
        parent: parent.map(ItemRef::Existing),
    };

    // The epic under its own grandchild, and the epic under a task.
    let operations = vec![
        under(epic, Some(task)),
        under(story, None),
        under(epic, Some(story)),
    ];
    let problems = validate(&operations, &all(&operations), &board, &HashMap::new()).unwrap_err();
    let reasons = reasons(&problems);
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(reasons[0].contains("inside itself"), "{reasons:?}");
    // The story went to the top first, so the epic under it is now a plain move that fits.
    assert!(
        reasons[1].contains("An epic cannot go under a story"),
        "{reasons:?}"
    );
}

#[test]
fn a_parent_moves_with_the_operations_before_it() {
    let (epic, task) = (Uuid::new_v4(), Uuid::new_v4());
    let mut board = board(&[(epic, "To Do"), (task, "To Do")]);
    board.forest.insert(epic, entry(None, Level::Group, "Epic"));
    let operations = vec![
        Operation::Parent {
            item: ItemRef::Existing(task),
            parent: Some(ItemRef::Existing(epic)),
        },
        Operation::Parent {
            item: ItemRef::Existing(task),
            parent: None,
        },
    ];
    assert!(validate(&operations, &all(&operations), &board, &HashMap::new()).is_ok());
}

#[test]
fn a_create_under_work_cannot_be_a_group() {
    let task = Uuid::new_v4();
    let board = board(&[(task, "To Do")]);
    let mut make = create("Checkout");
    make.item_type = Some("epic".into());
    make.parent = Some(ItemRef::Existing(task));
    let operations = vec![Operation::Create(make)];
    let problems = validate(&operations, &[0], &board, &HashMap::new()).unwrap_err();
    assert!(reasons(&problems)[0].contains("An epic cannot go under a task"));
}
