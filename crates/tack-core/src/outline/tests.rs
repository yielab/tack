use super::*;
use crate::workflow::scrum_workflow;

fn item(title: &str) -> OutlineItem {
    OutlineItem {
        title: title.into(),
        ..OutlineItem::default()
    }
}

fn with_ref(reference: &str, title: &str) -> OutlineItem {
    OutlineItem {
        reference: Some(reference.into()),
        ..item(title)
    }
}

fn child(title: &str, parent: &str) -> OutlineItem {
    OutlineItem {
        parent: Some(parent.into()),
        ..item(title)
    }
}

fn titles(resolved: &ResolvedOutline) -> Vec<&str> {
    resolved
        .items
        .iter()
        .map(|p| p.input.title.as_str())
        .collect()
}

fn problems(items: &[OutlineItem]) -> Vec<String> {
    resolve(items, &scrum_workflow())
        .expect_err("outline must be refused")
        .problems
}

#[test]
fn children_written_after_parents_listed_later() {
    let items = [child("Wireframes", "design"), with_ref("design", "Design")];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    assert_eq!(titles(&resolved), ["Design", "Wireframes"]);
    assert_eq!(resolved.items[1].parent, Some(PlannedParent::Planned(0)));
}

#[test]
fn written_order_follows_the_file_where_parents_allow() {
    let items = [with_ref("e", "Epic"), child("Kid", "e"), item("Loose")];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    assert_eq!(titles(&resolved), ["Epic", "Kid", "Loose"]);
}

#[test]
fn parent_found_by_ref_title_or_existing_id() {
    let existing = Uuid::new_v4();
    let cases = [
        (
            "by ref",
            vec![with_ref("e1", "Epic"), child("T", "e1")],
            Some(PlannedParent::Planned(0)),
        ),
        (
            "by title",
            vec![item("Epic"), child("T", "Epic")],
            Some(PlannedParent::Planned(0)),
        ),
        (
            "existing id",
            vec![item("Epic"), child("T", &existing.to_string())],
            Some(PlannedParent::Existing(existing)),
        ),
    ];
    for (case, items, expected) in cases {
        let resolved = resolve(&items, &scrum_workflow()).unwrap();
        assert_eq!(resolved.items[1].parent, expected, "{case}");
    }
}

#[test]
fn ref_wins_over_a_title() {
    let items = [item("a"), with_ref("a", "Other"), child("T", "a")];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    let parent = resolved.items[2].parent.unwrap();
    assert_eq!(parent, PlannedParent::Planned(1));
}

#[test]
fn depends_on_becomes_blocker_blocked_pairs() {
    let items = [
        with_ref("a", "Research"),
        OutlineItem {
            depends_on: vec!["a".into(), "Research".into()],
            ..item("Build")
        },
    ];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    assert_eq!(resolved.blocks, [(0, 1)], "a repeated blocker is one edge");
}

#[test]
fn refused_outlines_name_the_problem() {
    let cases: Vec<(&str, Vec<OutlineItem>, &str)> = vec![
        ("no title", vec![item("  ")], "item 1 has no title"),
        (
            "duplicate ref",
            vec![with_ref("x", "A"), with_ref("x", "B")],
            "reuses ref \"x\"",
        ),
        (
            "unknown parent",
            vec![child("T", "nowhere")],
            "no ref or title in the outline",
        ),
        (
            "own parent",
            vec![OutlineItem {
                parent: Some("T".into()),
                ..item("T")
            }],
            "is its own parent",
        ),
        (
            "ambiguous title",
            vec![item("Same"), item("Same"), child("T", "Same")],
            "title of 2 items",
        ),
        (
            "parent loop",
            vec![child("A", "B"), child("B", "A")],
            "form a loop",
        ),
        (
            "dependency loop",
            vec![
                OutlineItem {
                    depends_on: vec!["B".into()],
                    ..item("A")
                },
                OutlineItem {
                    depends_on: vec!["A".into()],
                    ..item("B")
                },
            ],
            "depends_on forms a loop",
        ),
        (
            "self dependency",
            vec![OutlineItem {
                depends_on: vec!["A".into()],
                ..item("A")
            }],
            "depends on itself",
        ),
        (
            "bad due date",
            vec![OutlineItem {
                due_date: Some("soon".into()),
                ..item("A")
            }],
            "is not a date",
        ),
        (
            "negative estimate",
            vec![OutlineItem {
                estimate: Some(-1.0),
                ..item("A")
            }],
            "non-negative",
        ),
    ];
    for (case, items, expected) in cases {
        let found = problems(&items);
        assert!(
            found.iter().any(|p| p.contains(expected)),
            "{case}: expected a problem containing {expected:?}, got {found:?}"
        );
    }
}

#[test]
fn every_problem_reported_at_once() {
    let items = [item(""), child("T", "nowhere")];
    assert_eq!(problems(&items).len(), 2);
}

#[test]
fn too_many_items_refused_before_reading_them() {
    let items = vec![item("x"); MAX_OUTLINE_ITEMS + 1];
    let found = problems(&items);
    assert_eq!(found.len(), 1);
    assert!(found[0].contains("at most"));
}

#[test]
fn status_matched_in_any_case_unknown_falls_back_with_warning() {
    let items = [
        OutlineItem {
            status: Some("in progress".into()),
            ..item("A")
        },
        OutlineItem {
            status: Some("Someday".into()),
            ..item("B")
        },
        item("C"),
    ];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    let statuses: Vec<&str> = resolved.items.iter().map(|p| p.status.as_str()).collect();
    assert_eq!(statuses, ["In Progress", "Backlog", "Backlog"]);
    assert_eq!(resolved.warnings.len(), 1);
    assert!(resolved.warnings[0].contains("\"Someday\""));
}

#[test]
fn unknown_priority_is_medium_with_warning() {
    let items = [OutlineItem {
        priority: Some("asap".into()),
        ..item("A")
    }];
    let resolved = resolve(&items, &scrum_workflow()).unwrap();
    assert_eq!(resolved.items[0].input.priority, None);
    assert_eq!(resolved.warnings.len(), 1);
}

#[test]
fn fields_parsed_leniently() {
    let items = [OutlineItem {
        item_type: Some("Epic".into()),
        priority: Some("URGENT".into()),
        due_date: Some("2026-11-01".into()),
        tags: vec![" ux ".into(), "".into()],
        description: Some("  ".into()),
        ..item("  Design  ")
    }];
    let input = &resolve(&items, &scrum_workflow()).unwrap().items[0].input;
    assert_eq!(input.title, "Design");
    assert_eq!(input.item_type, Some(ItemType::Epic));
    assert_eq!(input.priority, Some(Priority::Critical));
    assert_eq!(
        input.due_date.unwrap().to_rfc3339(),
        "2026-11-01T00:00:00+00:00"
    );
    assert_eq!(input.tags.as_deref(), Some(&["ux".to_string()][..]));
    assert_eq!(input.description, None);
}

#[test]
fn project_type_parsed_in_any_case() {
    assert_eq!(parse_project_type(" Software "), Ok(ProjectType::Software));
    assert!(
        parse_project_type("spaceship")
            .unwrap_err()
            .contains("software, web")
    );
}

#[test]
fn outline_reads_numbers_as_refs_and_one_dependency_as_a_list() {
    let outline: Outline = serde_json::from_value(serde_json::json!({
        "items": [
            { "ref": 1, "title": "A" },
            { "id": 2, "title": "B", "parent": 1, "depends_on": 1 }
        ]
    }))
    .unwrap();
    let b = &outline.items[1];
    assert_eq!(b.reference.as_deref(), Some("2"));
    assert_eq!(b.parent.as_deref(), Some("1"));
    assert_eq!(b.depends_on, ["1"]);
}

#[test]
fn outline_refuses_unknown_fields() {
    let err = serde_json::from_value::<Outline>(serde_json::json!({
        "items": [{ "title": "A", "children": [] }]
    }))
    .unwrap_err();
    assert!(err.to_string().contains("children"), "{err}");
}

// ─── CSV ──────────────────────────────────────────────────────────────

#[test]
fn csv_quoted_cells_may_hold_commas_quotes_and_newlines() {
    let csv = "Title,Description\n\"Plan, then build\",\"line one\nline \"\"two\"\"\"\nNext,\n";
    let read = items_from_csv(csv).unwrap();
    assert_eq!(read.items.len(), 2);
    assert_eq!(read.items[0].title, "Plan, then build");
    assert_eq!(
        read.items[0].description.as_deref(),
        Some("line one\nline \"two\"")
    );
    assert_eq!(read.items[1].origin.as_deref(), Some("row 4"));
}

#[test]
fn csv_columns_map_to_outline_fields() {
    let csv = "\u{feff}ref,title,parent,depends_on,tags,due,estimate,blocked\n\
               e,Epic,,,,,,\n\
               t,Task,e,x;y,ux | api,2026-01-02,2.5,\n";
    let read = items_from_csv(csv).unwrap();
    let task = &read.items[1];
    assert_eq!(task.reference.as_deref(), Some("t"));
    assert_eq!(task.parent.as_deref(), Some("e"));
    assert_eq!(task.depends_on, ["x", "y"]);
    assert_eq!(task.tags, ["ux", "api"]);
    assert_eq!(task.due_date.as_deref(), Some("2026-01-02"));
    assert_eq!(task.estimate, Some(2.5));
    assert_eq!(read.warnings, ["columns not read: \"blocked\""]);
}

#[test]
fn csv_rows_without_title_skipped_and_counted() {
    let read = items_from_csv("title,status\nA,\n,Done\n\n").unwrap();
    assert_eq!(read.items.len(), 1);
    assert_eq!(read.skipped, 1);
}

#[test]
fn csv_refused_without_title_column_or_with_bad_number() {
    let cases = [
        ("name,status\nA,Done\n", "title column"),
        ("", "no header row"),
        ("title,estimate\nA,lots\n", "row 2: estimate \"lots\""),
    ];
    for (csv, expected) in cases {
        let err = items_from_csv(csv).unwrap_err().to_string();
        assert!(err.contains(expected), "{csv:?}: {err}");
    }
}

#[test]
fn csv_export_reimports_with_hierarchy() {
    use crate::models::{EstimateUnit, Item};
    let now = Utc::now();
    let base = |title: &str, parent: Option<Uuid>| Item {
        recurrence: None,
        recurrence_of: None,
        id: Uuid::new_v4(),
        project_id: Uuid::nil(),
        parent_id: parent,
        title: title.into(),
        description: Some("has, a comma\nand a line".into()),
        item_type: ItemType::Task,
        status: "To Do".into(),
        priority: Priority::High,
        estimate: None,
        estimate_unit: EstimateUnit::default(),
        tags: vec!["a".into(), "b".into()],
        sort_order: 0,
        sprint_id: None,
        assignee: None,
        due_date: parse_due_date("2026-03-04"),
        started_at: None,
        completed_at: None,
        source: crate::models::ItemSource::Manual,
        source_artifact_id: None,
        run_settings: None,
        needs_review: false,
        merge_note: None,
        created_at: now,
        updated_at: now,
    };
    let parent = base("Epic, big", None);
    let kid = base("Kid", Some(parent.id));
    let read = items_from_csv(&items_to_csv(&[kid, parent])).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    let resolved = resolve(&read.items, &scrum_workflow()).unwrap();
    assert_eq!(titles(&resolved), ["Epic, big", "Kid"]);
    assert_eq!(resolved.items[1].parent, Some(PlannedParent::Planned(0)));
    let epic = &resolved.items[0];
    assert_eq!(epic.status, "To Do");
    assert_eq!(
        epic.input.description.as_deref(),
        Some("has, a comma\nand a line")
    );
    assert_eq!(epic.input.tags.as_deref().map(<[String]>::len), Some(2));
    assert_eq!(epic.input.due_date, parse_due_date("2026-03-04"));
}
