use super::*;

fn file(text: &str) -> Outline {
    serde_saphyr::from_str(text).expect("a project file")
}

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn a_team_no_computer_reports_comes_with_the_command_that_makes_it() {
    let outline = file(
        "teams:\n  \
           - { name: fab-research, shape: respond, delivers: report, methods: [research-review, house-style] }\n  \
           - { name: fab-build, shape: build }\n",
    );
    let gaps = missing(&outline, &names(&["fab-build"]), &names(&[]));
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].kind, MissingKind::Team);
    assert_eq!(gaps[0].shape.as_deref(), Some("respond"));
    // A shipped recipe is made by the command; a method the team's owner writes is not.
    assert_eq!(
        gaps[0].fix,
        "docket init fab-research --blueprint respond --recipe research-review"
    );
}

#[test]
fn a_team_a_profile_names_is_missing_too_with_no_shape() {
    let outline = file(
        "profiles:\n  - { name: Support, tool_policy: { agent: docket, docket: { team: fab-support } } }\n",
    );
    let gaps = missing(&outline, &names(&[]), &names(&["docket"]));
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].name, "fab-support");
    assert_eq!(gaps[0].shape, None);
    assert_eq!(gaps[0].fix, "docket init fab-support");
}

#[test]
fn an_agent_no_computer_reports_is_missing_with_the_sentence_that_installs_it() {
    let outline = file(
        "profiles:\n  \
           - { name: A, tool_policy: { agent: codex } }\n  \
           - { name: B, tool_policy: { agent: claude-code } }\n  \
           - { name: C }\n\
         automation: { default_harness: opencode }\n",
    );
    let gaps = missing(&outline, &names(&[]), &names(&["claude-code"]));
    let found: Vec<(&str, MissingKind)> = gaps
        .iter()
        .map(|gap| (gap.name.as_str(), gap.kind))
        .collect();
    assert_eq!(
        found,
        [
            ("codex", MissingKind::Harness),
            ("opencode", MissingKind::Harness)
        ]
    );
    assert!(
        gaps[0].fix.contains("tack runner doctor"),
        "{}",
        gaps[0].fix
    );
}

#[test]
fn a_file_that_expects_nothing_misses_nothing() {
    let outline = file("profiles:\n  - { name: Software }\nitems: []\n");
    assert!(missing(&outline, &names(&[]), &names(&[])).is_empty());
}
