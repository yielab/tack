use super::*;

const EVERY_PROJECT_TYPE: [ProjectType; 11] = [
    ProjectType::Software,
    ProjectType::Web,
    ProjectType::Mobile,
    ProjectType::Construction,
    ProjectType::Personal,
    ProjectType::Homework,
    ProjectType::Maintenance,
    ProjectType::Legal,
    ProjectType::Research,
    ProjectType::Event,
    ProjectType::Custom,
];

fn work(key: &str, default_work: bool) -> ItemTypeDef {
    ItemTypeDef {
        key: key.into(),
        name: key.into(),
        level: Level::Work,
        color: TypeColor::Neutral,
        order: 0,
        default_work,
    }
}

#[test]
fn every_preset_passes_check_catalog_and_names_a_group() {
    for project_type in EVERY_PROJECT_TYPE {
        let types = preset(&project_type);
        check_catalog(&types).unwrap_or_else(|e| panic!("{project_type}: {e}"));
        assert!(
            types.iter().any(|t| t.level == Level::Group),
            "{project_type} has no type that holds work"
        );
        let orders: Vec<u32> = types.iter().map(|t| t.order).collect();
        assert!(orders.windows(2).all(|w| w[0] < w[1]), "{project_type}");
    }
}

#[test]
fn software_holds_work_in_epic_and_construction_names_its_levels() {
    let software = preset(&ProjectType::Software);
    let epic = software.iter().find(|t| t.key == "epic").unwrap();
    assert_eq!((epic.name.as_str(), epic.level), ("Epic", Level::Group));
    assert_eq!(default_work_key(&software), Some("task"));

    let construction = preset(&ProjectType::Construction);
    let groups: Vec<&str> = construction
        .iter()
        .filter(|t| t.level == Level::Group)
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(groups, ["Building", "Section"]);
    let default = construction.iter().find(|t| t.default_work).unwrap();
    assert_eq!(default.name, "Work Order");
    assert!(construction.iter().all(|t| t.key != "subtask"));
}

#[test]
fn check_catalog_refuses_two_defaults_a_group_default_and_a_bad_key() {
    assert!(check_catalog(&[work("task", true), work("bug", true)]).is_err());
    assert!(check_catalog(&[work("task", false)]).is_err());

    let mut group = work("epic", true);
    group.level = Level::Group;
    assert!(check_catalog(&[group, work("task", false)]).is_err());

    assert!(check_catalog(&[work("Task", true)]).is_err());
    assert!(check_catalog(&[work("two words", true)]).is_err());
    assert!(check_catalog(&[work("", true)]).is_err());
    assert!(check_catalog(&[work(&"a".repeat(41), true)]).is_err());
    assert!(check_catalog(&[work("task", true), work("task", false)]).is_err());
    assert!(check_catalog(&[work("a-b_9", true)]).is_ok());
}

#[test]
fn a_word_becomes_a_valid_key() {
    assert_eq!(key_of_word("Spike").as_deref(), Some("spike"));
    assert_eq!(key_of_word("  Work Order ").as_deref(), Some("work-order"));
    assert_eq!(key_of_word("!!!"), None);
    assert!(is_valid_key(&key_of_word(&"x ".repeat(40)).unwrap()));
}

#[test]
fn level_and_colour_round_trip_through_their_words() {
    for level in [Level::Group, Level::Work] {
        assert_eq!(Level::parse(level.as_str()), Some(level));
    }
    for color in [
        TypeColor::Accent,
        TypeColor::Accent2,
        TypeColor::Danger,
        TypeColor::Warning,
        TypeColor::Success,
        TypeColor::Neutral,
    ] {
        assert_eq!(TypeColor::parse(color.as_str()), Some(color));
        assert_eq!(
            serde_json::to_value(color).unwrap(),
            serde_json::json!(color.as_str())
        );
    }
}

fn software() -> Vec<ItemTypeDef> {
    preset(&ProjectType::Software)
}

fn keys(types: &[ItemTypeDef]) -> Vec<&str> {
    types.iter().map(|t| t.key.as_str()).collect()
}

#[test]
fn a_type_is_added_last_and_never_as_the_default() {
    let new = CreateItemType {
        key: "spike".into(),
        name: " Spike ".into(),
        level: Level::Work,
        color: TypeColor::Accent,
    };
    let next = add(&software(), &new).unwrap();
    let spike = next.last().unwrap();
    assert_eq!(
        (spike.key.as_str(), spike.name.as_str()),
        ("spike", "Spike")
    );
    assert!(!spike.default_work);
    let again = add(&next, &new);
    assert!(again.is_err(), "a key is used once");
    let bad = CreateItemType {
        key: "No Good".into(),
        ..new
    };
    assert!(add(&software(), &bad).is_err());
}

#[test]
fn the_level_of_a_type_changes_only_while_no_item_uses_it() {
    let patch = UpdateItemType {
        level: Some(Level::Group),
        ..Default::default()
    };
    assert!(matches!(
        update(&software(), "feature", &patch, 3),
        Err(CoreError::TypeInUse { items: 3, .. })
    ));
    let next = update(&software(), "feature", &patch, 0).unwrap();
    assert_eq!(next[1].level, Level::Group);
}

#[test]
fn another_default_replaces_the_first_and_a_group_cannot_be_it() {
    let patch = UpdateItemType {
        default_work: Some(true),
        ..Default::default()
    };
    let next = update(&software(), "bug", &patch, 0).unwrap();
    assert_eq!(default_work_key(&next), Some("bug"));
    assert_eq!(next.iter().filter(|t| t.default_work).count(), 1);
    assert!(update(&software(), "epic", &patch, 0).is_err());
    let unset = UpdateItemType {
        default_work: Some(false),
        ..Default::default()
    };
    assert!(update(&software(), "task", &unset, 0).is_err());
}

#[test]
fn a_type_moves_to_the_position_asked() {
    let to = |key: &str, order: u32| {
        let patch = UpdateItemType {
            order: Some(order),
            ..Default::default()
        };
        update(&software(), key, &patch, 0).unwrap()
    };
    assert_eq!(
        keys(&to("bug", 0)),
        ["bug", "epic", "feature", "task", "requirement"]
    );
    assert_eq!(
        keys(&to("epic", 2)),
        ["feature", "task", "epic", "bug", "requirement"]
    );
    assert_eq!(
        keys(&to("epic", 99)),
        ["feature", "task", "bug", "requirement", "epic"]
    );
    let moved = to("bug", 1);
    let orders: Vec<u32> = moved.iter().map(|t| t.order).collect();
    assert_eq!(orders, [0, 1, 2, 3, 4]);
}

#[test]
fn deleting_a_used_type_needs_a_type_of_its_level_to_merge_into() {
    assert!(matches!(
        remove(&software(), "bug", None, 2),
        Err(CoreError::TypeInUse { items: 2, .. })
    ));
    let next = remove(&software(), "bug", Some("feature"), 2).unwrap();
    assert!(!keys(&next).contains(&"bug"));
    assert!(
        remove(&software(), "bug", Some("epic"), 2).is_err(),
        "other level"
    );
    assert!(remove(&software(), "bug", Some("bug"), 2).is_err());
    assert!(remove(&software(), "bug", Some("ghost"), 2).is_err());
    assert!(
        remove(&software(), "task", Some("feature"), 2).is_err(),
        "the default stays"
    );
    assert!(remove(&software(), "bug", None, 0).is_ok());
}
