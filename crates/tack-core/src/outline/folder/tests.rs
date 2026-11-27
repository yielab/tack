use super::*;
use crate::outline::key::key_of;

const MY_SAAS: &str = include_str!("../../../../tack-api/tests/fixtures/bundles/my-saas.yaml");

fn outline(text: &str) -> Outline {
    serde_saphyr::from_str(text).expect("the outline parses")
}

fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
    list.iter()
        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
        .collect()
}

/// My SaaS as a folder carries it: every item has the key of its title, names its parent and
/// blockers by key, and has none of the board's keys; items in key order.
fn as_the_folder_has_it() -> Outline {
    let mut outline = outline(MY_SAAS);
    let keys: BTreeMap<String, String> = outline
        .items
        .iter()
        .filter_map(|item| Some((item.reference.clone()?, key_of(&item.title))))
        .collect();
    for item in &mut outline.items {
        item.key = Some(key_of(&item.title));
        item.parent = item.parent.as_ref().map(|p| keys[p].clone());
        item.depends_on = item.depends_on.iter().map(|d| keys[d].clone()).collect();
        item.reference = None;
        item.status = None;
        item.priority = None;
        item.assignee = None;
        item.sprint = None;
        item.due_date = None;
    }
    outline.items.sort_by(|a, b| a.key.cmp(&b.key));
    outline
}

#[test]
fn a_folder_written_and_read_is_the_same_plan() {
    let planned = as_the_folder_has_it();
    let read = read(&write(&planned)).expect("the folder reads");
    assert_eq!(
        serde_json::to_value(&read).expect("json"),
        serde_json::to_value(&planned).expect("json"),
    );
}

#[test]
fn write_never_writes_the_boards_keys() {
    let mut planned = outline(MY_SAAS);
    for item in &mut planned.items {
        item.key = Some(key_of(&item.title));
    }
    let written = write(&planned);
    assert!(
        planned.items.iter().any(|i| i.status.is_some()),
        "the fixture gives items a status",
    );
    for (path, text) in written.iter().filter(|(p, _)| p.starts_with("items/")) {
        let front = text.split("---\n").nth(1).expect("front matter");
        for board_key in [
            "status", "priority", "assignee", "sprint", "due_date", "ref", "key",
        ] {
            assert!(
                !front
                    .lines()
                    .any(|l| l.starts_with(&format!("{board_key}:"))),
                "{path} writes {board_key}",
            );
        }
    }
}

#[test]
fn an_item_with_no_description_writes_no_body() {
    let mut planned = Outline::default();
    planned.items.push(crate::outline::OutlineItem {
        key: Some("login".into()),
        title: "Login".into(),
        ..Default::default()
    });
    let written = write(&planned);
    let text = &written["items/login.md"];
    assert!(text.starts_with("---\n"));
    assert!(
        text.ends_with("---\n"),
        "nothing after the closing line: {text:?}"
    );
}

#[test]
fn a_file_the_folder_cannot_hold_is_refused_naming_it() {
    let good = "---\ntitle: A\n---\n";
    let rows: &[(&[(&str, &str)], &str)] = &[
        (&[("items/Login.md", good)], "items/Login.md"),
        (&[("items/login.md", "---\ntitle: A\n")], "items/login.md"),
        (&[("items/login.md", "title: A\n")], "items/login.md"),
        (&[("notes.txt", "hello")], "notes.txt"),
        (
            &[("items/login.md", "---\ntitle: [\n---\n")],
            "items/login.md",
        ),
        (&[("project.yaml", "bogus: 1\n")], "project.yaml"),
        (&[("items/login.md", good)], "project.yaml"),
        (
            &[("items/login.md", "---\nkey: other\ntitle: A\n---\n")],
            "items/login.md",
        ),
        (
            &[("items/login.md", "---\ntitle: A\ndescription: B\n---\n")],
            "items/login.md",
        ),
    ];
    for (list, named) in rows {
        let problems = read(&files(list)).expect_err("refused");
        assert!(
            problems.iter().any(|p| p.contains(named)),
            "{list:?} -> {problems:?}",
        );
    }
}
