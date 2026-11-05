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
