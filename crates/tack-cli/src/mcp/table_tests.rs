use super::tool::{self, Kind};
use super::*;

/// What `tools/list` answered before the tools moved into a table.
const LISTED: &str = include_str!("tools_list.json");

const GUIDE: &str = include_str!("../../../../docs/MCP.md");

#[test]
fn tools_list_is_what_it_was_before_the_table() {
    let expected: Vec<Value> = serde_json::from_str(LISTED).unwrap();
    assert_eq!(Value::Array(tool::advertised()), Value::Array(expected));
}

#[test]
fn tools_list_keeps_its_order() {
    let expected: Vec<Value> = serde_json::from_str(LISTED).unwrap();
    let names: Vec<&str> = tool::TOOLS.iter().map(|tool| tool.name).collect();
    let expected: Vec<&str> = expected
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, expected);
}

#[test]
fn every_listed_tool_is_found_by_its_name_once() {
    for tool in tool::TOOLS {
        assert!(std::ptr::eq(tool::find(tool.name).unwrap(), *tool));
        let same = tool::TOOLS.iter().filter(|t| t.name == tool.name).count();
        assert_eq!(same, 1, "{} is in the table {same} times", tool.name);
    }
}

#[test]
fn the_guide_gives_each_tool_the_kind_the_table_does() {
    for tool in tool::TOOLS {
        let row = GUIDE
            .lines()
            .find(|line| line.starts_with(&format!("| `{}` |", tool.name)))
            .unwrap_or_else(|| panic!("docs/MCP.md has no row for {}", tool.name));
        let kind = row.split('|').nth(2).unwrap().trim();
        let expected = match tool.kind {
            Kind::Read => "read",
            Kind::Write => "write",
        };
        assert_eq!(kind, expected, "{}", tool.name);
    }
}
