use super::*;

fn placement(level: Level, name: &str, height: usize, project: Uuid) -> Placement {
    Placement {
        item: Uuid::new_v4(),
        title: "Checkout".into(),
        type_name: name.into(),
        level,
        project,
        subtree_height: height,
    }
}

fn target(level: Level, name: &str, ancestors: Vec<Uuid>, project: Uuid) -> Target {
    Target {
        id: Uuid::new_v4(),
        type_name: name.into(),
        level,
        project,
        ancestors,
    }
}

#[test]
fn a_group_is_a_group_by_its_type_or_by_having_children() {
    assert_eq!(node(Level::Group, false), Node::Group);
    assert_eq!(node(Level::Group, true), Node::Group);
    assert_eq!(node(Level::Work, true), Node::Group);
    assert_eq!(node(Level::Work, false), Node::Work);
}

#[test]
fn each_pair_of_levels_is_decided_by_the_rule() {
    let project = Uuid::new_v4();
    let group = target(Level::Group, "Epic", vec![], project);
    let work = target(Level::Work, "Task", vec![], project);
    let epic = placement(Level::Group, "Epic", 0, project);
    let task = placement(Level::Work, "Task", 0, project);

    assert!(admit_parent(&epic, Some(&group)).is_ok());
    assert!(admit_parent(&task, Some(&group)).is_ok());
    assert!(admit_parent(&task, Some(&work)).is_ok());
    let refused = admit_parent(&epic, Some(&work)).unwrap_err();
    assert!(matches!(refused, CoreError::GroupUnderWork { .. }));
    assert_eq!(refused.to_string(), "An epic cannot go under a task.");
}

#[test]
fn the_top_is_always_admitted() {
    let project = Uuid::new_v4();
    assert!(admit_parent(&placement(Level::Group, "Epic", 4, project), None).is_ok());
}

#[test]
fn an_item_cannot_go_under_itself_or_under_what_it_holds() {
    let project = Uuid::new_v4();
    let item = placement(Level::Work, "Task", 3, project);

    let mut itself = target(Level::Work, "Task", vec![], project);
    itself.id = item.item;
    let refused = admit_parent(&item, Some(&itself)).unwrap_err();
    assert!(matches!(refused, CoreError::ParentLoop { .. }));
    assert_eq!(
        refused.to_string(),
        "That would put Checkout inside itself."
    );

    // A loop of three: the item holds a, a holds b, and b is the asked parent.
    let (a, middle) = (Uuid::new_v4(), Uuid::new_v4());
    let inside = target(Level::Work, "Task", vec![middle, a, item.item], project);
    assert!(matches!(
        admit_parent(&item, Some(&inside)),
        Err(CoreError::ParentLoop { .. })
    ));
}

#[test]
fn five_levels_are_allowed_and_six_are_not() {
    let project = Uuid::new_v4();
    let above = |n: usize| (0..n).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
    let leaf = placement(Level::Work, "Task", 0, project);

    // A parent at the fourth level puts the item at the fifth.
    let fourth = target(Level::Work, "Task", above(3), project);
    assert_eq!(fourth.depth(), 4);
    assert!(admit_parent(&leaf, Some(&fourth)).is_ok());

    let fifth = target(Level::Work, "Task", above(4), project);
    let refused = admit_parent(&leaf, Some(&fifth)).unwrap_err();
    assert!(matches!(refused, CoreError::TreeTooDeep));
    assert_eq!(refused.to_string(), "Five levels is the most.");

    // The subtree counts: an item with two levels under it needs a parent at the second level or higher.
    let holder = placement(Level::Work, "Task", 2, project);
    assert!(
        admit_parent(
            &holder,
            Some(&target(Level::Work, "Task", above(1), project))
        )
        .is_ok()
    );
    assert!(matches!(
        admit_parent(
            &holder,
            Some(&target(Level::Work, "Task", above(2), project))
        ),
        Err(CoreError::TreeTooDeep)
    ));
}

#[test]
fn a_parent_in_another_project_is_refused() {
    let item = placement(Level::Work, "Task", 0, Uuid::new_v4());
    let other = target(Level::Group, "Epic", vec![], Uuid::new_v4());
    assert!(matches!(
        admit_parent(&item, Some(&other)),
        Err(CoreError::ParentInOtherProject)
    ));
}

#[test]
fn only_work_goes_in_a_sprint() {
    assert!(admit_sprint(Node::Work, "Task").is_ok());
    let refused = admit_sprint(Node::Group, "Epic").unwrap_err();
    assert!(matches!(refused, CoreError::GroupInSprint { .. }));
    assert_eq!(
        refused.to_string(),
        "An epic does not go in a sprint: its items do."
    );
}

#[test]
fn work_with_children_counts_as_a_group_for_the_sprint() {
    assert!(admit_sprint(node(Level::Work, true), "Task").is_err());
    assert!(admit_sprint(node(Level::Work, false), "Task").is_ok());
}

#[test]
fn sentences_take_the_article_of_the_projects_own_word() {
    let refused = CoreError::GroupUnderWork {
        group: "Building".into(),
        parent: "Work Order".into(),
    };
    assert_eq!(
        refused.to_string(),
        "A building cannot go under a work order."
    );
    let refused = CoreError::GroupUnderWork {
        group: "Course".into(),
        parent: "Assignment".into(),
    };
    assert_eq!(
        refused.to_string(),
        "A course cannot go under an assignment."
    );
}

fn entry(parent: Option<Uuid>, level: Level, name: &str) -> Entry {
    Entry {
        parent,
        title: name.to_owned(),
        type_name: name.to_owned(),
        level,
    }
}

#[test]
fn a_forest_reads_height_and_ancestors_from_its_parents() {
    let project = Uuid::new_v4();
    let (epic, story, task, other) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let mut forest = Forest::new(project);
    forest.insert(epic, entry(None, Level::Group, "Epic"));
    forest.insert(story, entry(Some(epic), Level::Work, "Story"));
    forest.insert(task, entry(Some(story), Level::Work, "Task"));
    forest.insert(other, entry(None, Level::Work, "Task"));

    assert_eq!(forest.placement(epic).unwrap().subtree_height, 2);
    assert_eq!(forest.placement(task).unwrap().subtree_height, 0);
    assert_eq!(forest.target(task).unwrap().ancestors, vec![story, epic]);
    assert_eq!(forest.target(task).unwrap().depth(), 3);

    // The epic under its own grandchild is the loop the rule names.
    let held = forest.target(task).unwrap();
    assert!(matches!(
        admit_parent(&forest.placement(epic).unwrap(), Some(&held)),
        Err(CoreError::ParentLoop { .. })
    ));

    // A move changes what the next one sees.
    forest.set_parent(other, Some(task));
    assert_eq!(forest.placement(epic).unwrap().subtree_height, 3);
}

#[test]
fn a_loop_already_stored_does_not_hang_the_walk() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut forest = Forest::new(Uuid::new_v4());
    forest.insert(a, entry(Some(b), Level::Work, "Task"));
    forest.insert(b, entry(Some(a), Level::Work, "Task"));
    assert_eq!(forest.target(a).unwrap().ancestors, vec![b]);
    assert_eq!(forest.placement(a).unwrap().subtree_height, 1);
}
