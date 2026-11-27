use super::*;
use crate::outline::{BundleRole, BundleTeam};

const MY_SAAS: &str = include_str!("../../../../../tack-api/tests/fixtures/bundles/my-saas.yaml");

const REMOVED: &str = "removed on the board since the last revision";
const CREATED: &str = "created on the board since the last revision";
const NO_REVISION: &str = "no revision yet: export once to make the folder authoritative";

fn yaml(text: &str) -> Outline {
    serde_saphyr::from_str(text).expect("the outline parses")
}

/// An outline whose only content is these roles, as `(name, colour)`.
fn roles(list: &[(&str, &str)]) -> Outline {
    Outline {
        roles: list
            .iter()
            .map(|(name, color)| BundleRole {
                name: (*name).into(),
                color: Some((*color).into()),
                icon: None,
                profile: None,
            })
            .collect(),
        ..Outline::default()
    }
}

/// `verb:key:fields:reason`, the way a row of the table writes a change.
fn show(change: &Change) -> String {
    let paths: Vec<&str> = change.fields.iter().map(|f| f.path.as_str()).collect();
    format!(
        "{}:{}:{}:{}",
        format!("{:?}", change.verb).to_lowercase(),
        change.key,
        paths.join(","),
        change.reason.as_deref().unwrap_or("")
    )
}

fn shown(merged: &Merged) -> Vec<String> {
    merged.changes.iter().map(show).collect()
}

fn conflicts(merged: &Merged) -> Vec<String> {
    merged
        .conflicts
        .iter()
        .map(|c| format!("{}.{}", c.key, c.path))
        .collect()
}

fn colours(merged: &Merged) -> Vec<String> {
    merged
        .outline
        .roles
        .iter()
        .map(|r| format!("{}={}", r.name, r.color.as_deref().unwrap_or("")))
        .collect()
}

type Roles<'a> = &'a [(&'a str, &'a str)];

struct Row<'a> {
    label: &'a str,
    base: Option<Roles<'a>>,
    board: Roles<'a>,
    file: Roles<'a>,
    prefer: Option<Prefer>,
    changes: &'a [&'a str],
    conflicts: &'a [&'a str],
    merged: &'a [&'a str],
}

#[test]
fn every_rule_of_the_three_ways_has_its_row() {
    use Prefer::{Board, File};
    let rows = [
        // A field, on both sides.
        Row {
            label: "board and file agree",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "blue")],
            prefer: None,
            changes: &[],
            conflicts: &[],
            merged: &["Dev=blue"],
        },
        Row {
            label: "only the file changed",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "red")],
            file: &[("Dev", "blue")],
            prefer: None,
            changes: &["update:dev:color:"],
            conflicts: &[],
            merged: &["Dev=blue"],
        },
        Row {
            label: "only the board changed",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "red")],
            prefer: None,
            changes: &[],
            conflicts: &[],
            merged: &["Dev=blue"],
        },
        Row {
            label: "both changed",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "green")],
            prefer: None,
            changes: &[],
            conflicts: &["dev.color"],
            merged: &["Dev=blue"],
        },
        Row {
            label: "both changed, prefer the file",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "green")],
            prefer: Some(File),
            changes: &["update:dev:color:"],
            conflicts: &["dev.color"],
            merged: &["Dev=green"],
        },
        Row {
            label: "both changed, prefer the board",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "green")],
            prefer: Some(Board),
            changes: &[],
            conflicts: &["dev.color"],
            merged: &["Dev=blue"],
        },
        Row {
            label: "no base, they agree",
            base: None,
            board: &[("Dev", "blue")],
            file: &[("Dev", "blue")],
            prefer: None,
            changes: &[],
            conflicts: &[],
            merged: &["Dev=blue"],
        },
        Row {
            label: "no base, they differ",
            base: None,
            board: &[("Dev", "blue")],
            file: &[("Dev", "green")],
            prefer: None,
            changes: &[],
            conflicts: &["dev.color"],
            merged: &["Dev=blue"],
        },
        Row {
            label: "no base entry, they differ",
            base: Some(&[]),
            board: &[("Dev", "blue")],
            file: &[("Dev", "green")],
            prefer: Some(File),
            changes: &["update:dev:color:"],
            conflicts: &["dev.color"],
            merged: &["Dev=green"],
        },
        // An entry in the file only.
        Row {
            label: "in the file, not in the base",
            base: Some(&[]),
            board: &[],
            file: &[("Dev", "red")],
            prefer: None,
            changes: &["create:dev::"],
            conflicts: &[],
            merged: &["Dev=red"],
        },
        Row {
            label: "in the file, no base at all",
            base: None,
            board: &[],
            file: &[("Dev", "red")],
            prefer: None,
            changes: &["create:dev::"],
            conflicts: &[],
            merged: &["Dev=red"],
        },
        Row {
            label: "in the file as the base had it, gone from the board",
            base: Some(&[("Dev", "red")]),
            board: &[],
            file: &[("Dev", "red")],
            prefer: None,
            changes: &[&format!("keep:dev::{REMOVED}")],
            conflicts: &[],
            merged: &[],
        },
        Row {
            label: "in the file changed, gone from the board",
            base: Some(&[("Dev", "red")]),
            board: &[],
            file: &[("Dev", "blue")],
            prefer: None,
            changes: &[],
            conflicts: &["dev."],
            merged: &[],
        },
        Row {
            label: "same, prefer the file",
            base: Some(&[("Dev", "red")]),
            board: &[],
            file: &[("Dev", "blue")],
            prefer: Some(File),
            changes: &["create:dev::"],
            conflicts: &["dev."],
            merged: &["Dev=blue"],
        },
        Row {
            label: "same, prefer the board",
            base: Some(&[("Dev", "red")]),
            board: &[],
            file: &[("Dev", "blue")],
            prefer: Some(Board),
            changes: &[&format!("keep:dev::{REMOVED}")],
            conflicts: &["dev."],
            merged: &[],
        },
        // An entry on the board only.
        Row {
            label: "on the board as the base had it, gone from the file",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "red")],
            file: &[],
            prefer: None,
            changes: &["remove:dev::"],
            conflicts: &[],
            merged: &[],
        },
        Row {
            label: "on the board changed, gone from the file",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[],
            prefer: None,
            changes: &[],
            conflicts: &["dev."],
            merged: &["Dev=blue"],
        },
        Row {
            label: "same, prefer the file",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[],
            prefer: Some(File),
            changes: &["remove:dev::"],
            conflicts: &["dev."],
            merged: &[],
        },
        Row {
            label: "same, prefer the board",
            base: Some(&[("Dev", "red")]),
            board: &[("Dev", "blue")],
            file: &[],
            prefer: Some(Board),
            changes: &[],
            conflicts: &["dev."],
            merged: &["Dev=blue"],
        },
        Row {
            label: "on the board, not in the base",
            base: Some(&[]),
            board: &[("Dev", "red")],
            file: &[],
            prefer: None,
            changes: &[&format!("keep:dev::{CREATED}")],
            conflicts: &[],
            merged: &["Dev=red"],
        },
        Row {
            label: "on the board, no base at all",
            base: None,
            board: &[("Dev", "red")],
            file: &[],
            prefer: None,
            changes: &[&format!("keep:dev::{NO_REVISION}")],
            conflicts: &[],
            merged: &["Dev=red"],
        },
        Row {
            label: "the file's order, then the board's by name",
            base: None,
            board: &[("Zed", "z"), ("Amy", "a")],
            file: &[("Bob", "b")],
            prefer: None,
            changes: &[
                "create:bob::",
                &format!("keep:amy::{NO_REVISION}"),
                &format!("keep:zed::{NO_REVISION}"),
            ],
            conflicts: &[],
            merged: &["Bob=b", "Amy=a", "Zed=z"],
        },
    ];
    for row in rows {
        let (board, file) = (roles(row.board), roles(row.file));
        let base = row.base.map(roles);
        let merged = merge(
            &Three {
                base: base.as_ref(),
                board: &board,
                file: &file,
            },
            row.prefer,
        );
        assert_eq!(shown(&merged), row.changes, "{}: changes", row.label);
        assert_eq!(
            conflicts(&merged),
            row.conflicts,
            "{}: conflicts",
            row.label
        );
        assert_eq!(colours(&merged), row.merged, "{}: merged", row.label);
    }
}

#[test]
fn a_diff_reads_from_the_first_outline_to_the_second() {
    let (from, to) = (
        roles(&[("Dev", "red"), ("Ops", "grey")]),
        roles(&[("Dev", "blue"), ("QA", "gold")]),
    );
    let seen: Vec<String> = diff(&from, &to).iter().map(show).collect();
    assert_eq!(seen, ["update:dev:color:", "create:qa::", "remove:ops::"]);
    assert!(diff(&from, &from).is_empty());
}

#[test]
fn a_file_that_renames_edits_and_drops_keeps_what_the_board_changed() {
    let base = yaml(MY_SAAS);
    let mut board = base.clone();
    board.roles[0].color = Some("#000000".into());
    board.roles.push(BundleRole {
        name: "Ops".into(),
        color: None,
        icon: None,
        profile: None,
    });
    board.teams.push(BundleTeam {
        name: "saas-dev".into(),
        shape: None,
        delivers: None,
        methods: Vec::new(),
    });
    let mut file = base.clone();
    file.roles[0].icon = Some("pen".into());
    file.sprints.truncate(1);
    file.automation.as_mut().expect("automation").default_branch = Some("trunk".into());
    file.project.as_mut().expect("project").project_type = Some("software".into());

    let merged = merge(
        &Three {
            base: Some(&base),
            board: &board,
            file: &file,
        },
        None,
    );
    assert_eq!(
        shown(&merged),
        [
            "keep:project:type:the board's".to_string(),
            "update:contenido:icon:".to_string(),
            format!("keep:ops::{CREATED}"),
            "remove:semana-43::".to_string(),
            "update:automation:default_branch:".to_string(),
        ]
    );
    assert!(merged.conflicts.is_empty());
    let out = &merged.outline;
    assert_eq!(out.roles[0].color.as_deref(), Some("#000000"));
    assert_eq!(out.roles[0].icon.as_deref(), Some("pen"));
    assert_eq!(out.roles.last().map(|r| r.name.as_str()), Some("Ops"));
    assert_eq!(out.sprints.len(), 1);
    assert_eq!(
        out.automation
            .as_ref()
            .and_then(|a| a.default_branch.as_deref()),
        Some("trunk")
    );
    let project = out.project.as_ref().expect("project");
    assert_eq!(project.project_type.as_deref(), Some("web"));
    assert_eq!(project.workflow.as_ref().map(|w| w.statuses.len()), Some(5));
    assert_eq!(out.teams.len(), 1);
    assert_eq!(out.types.len(), 5);
}

#[test]
fn an_item_is_compared_by_its_plan_and_never_by_the_boards_keys() {
    let base = yaml(MY_SAAS);
    let item = |outline: &mut Outline, title: &str| {
        outline
            .items
            .iter()
            .position(|i| i.title == title)
            .expect("the item is in the fixture")
    };
    let mut board = base.clone();
    let s1 = item(&mut board, "Guía para crear tu primera factura");
    board.items[s1].description = Some("Escrito en el tablero.".into());
    let mut file = base.clone();
    let d1 = item(&mut file, "Vista previa de la factura en PDF");
    file.items[d1].description = Some("Con una vista previa.".into());
    file.items[d1].fields.insert("Plan".into(), "team".into());
    let d2 = item(&mut file, "Selector de plan en el onboarding");
    file.items[d2].status = Some("Hecho".into());
    let r2 = item(&mut file, "Comparativa de proveedores de firma electrónica");
    file.items.remove(r2);

    let merged = merge(
        &Three {
            base: Some(&base),
            board: &board,
            file: &file,
        },
        None,
    );
    assert_eq!(
        shown(&merged),
        [
            "update:vista-previa-de-la-factura-en-pdf:description,fields.Plan:".to_string(),
            "keep:selector-de-plan-en-el-onboarding:status:the board's".to_string(),
            "remove:comparativa-de-proveedores-de-firma-electrónica::".to_string(),
        ]
    );
    assert!(merged.conflicts.is_empty());
    let out = &merged.outline.items;
    assert_eq!(out.len(), base.items.len() - 1);
    let find = |title: &str| out.iter().find(|i| i.title == title).expect("kept");
    assert_eq!(
        find("Guía para crear tu primera factura")
            .description
            .as_deref(),
        Some("Escrito en el tablero.")
    );
    assert_eq!(
        find("Selector de plan en el onboarding").status.as_deref(),
        Some("Por hacer")
    );
    assert_eq!(
        find("Vista previa de la factura en PDF").fields["Plan"],
        "team"
    );

    // The brief of ADR 0025 is not in the outline yet; its paths are dotted like any object's.
    let (mut fields, mut found) = (Vec::new(), Vec::new());
    let value = |risk: &str, done: &str| serde_json::json!({ "brief": { "risk": risk, "definition_of_done": done } });
    let (base, board, file) = (value("low", "ok"), value("low", "ok"), value("high", "ok"));
    let merged = diff_values(
        Some(&base),
        &board,
        &file,
        "",
        None,
        &mut fields,
        &mut found,
    );
    assert_eq!(merged, file);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].path, "brief.risk");
    assert!(found.is_empty());
}

#[test]
fn two_budgets_changed_on_both_sides_are_a_conflict_the_preference_settles() {
    let with_budget = |tokens: i64| {
        let mut outline = yaml(MY_SAAS);
        let budgets = outline
            .automation
            .as_mut()
            .and_then(|a| a.autonomy.as_mut())
            .and_then(|a| a.budgets.as_mut())
            .expect("budgets");
        budgets.tokens_per_day = Some(tokens);
        outline
    };
    let tokens = |m: &Merged| {
        m.outline
            .automation
            .as_ref()
            .and_then(|a| a.autonomy.as_ref())
            .and_then(|a| a.budgets.as_ref())
            .and_then(|b| b.tokens_per_day)
    };
    let (base, board, file) = (
        with_budget(2_000_000),
        with_budget(3_000_000),
        with_budget(4_000_000),
    );
    let three = Three {
        base: Some(&base),
        board: &board,
        file: &file,
    };
    let path = "autonomy.budgets.tokens_per_day";

    let open = merge(&three, None);
    assert_eq!(conflicts(&open), [format!("automation.{path}")]);
    assert_eq!(open.conflicts[0].board, 3_000_000);
    assert_eq!(open.conflicts[0].file, 4_000_000);
    assert_eq!(open.conflicts[0].base, 2_000_000);
    assert_eq!(tokens(&open), Some(3_000_000));

    let theirs = merge(&three, Some(Prefer::File));
    assert_eq!(conflicts(&theirs), [format!("automation.{path}")]);
    assert_eq!(shown(&theirs), [format!("update:automation:{path}:")]);
    assert_eq!(tokens(&theirs), Some(4_000_000));

    let ours = merge(&three, Some(Prefer::Board));
    assert_eq!(conflicts(&ours), [format!("automation.{path}")]);
    assert!(ours.changes.is_empty());
    assert_eq!(tokens(&ours), Some(3_000_000));
}
