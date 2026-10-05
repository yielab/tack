use validator::Validate;

use super::*;
use crate::models::UpsertItemBrief;

const EXAMPLE: &str = include_str!("../../../../docs/contracts/brief-v1/example.json");

/// FNV-1a 64 of the example's bytes, pinned like the other contract fixtures.
const EXAMPLE_FNV1A64: u64 = 0xc522_828c_401d_7700;

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn example() -> ItemBrief {
    serde_json::from_str(EXAMPLE).expect("example.json is an ItemBrief")
}

fn item(title: &str) -> Item {
    serde_json::from_value(serde_json::json!({
        "id": "0192f5a0-7c1e-7d3a-9b2e-5f6a7b8c9d0e",
        "project_id": "0192f5a0-0000-7000-8000-000000000001",
        "parent_id": null, "title": title, "description": null,
        "item_type": "task", "status": "To Do", "priority": "medium",
        "estimate": null, "estimate_unit": "story_points", "tags": [],
        "sort_order": 0, "sprint_id": null, "assignee": null, "due_date": null,
        "started_at": null, "completed_at": null,
        "created_at": "2026-10-05T10:00:00Z", "updated_at": "2026-10-05T10:00:00Z"
    }))
    .expect("an Item")
}

fn upsert(acceptance: Vec<AcceptanceCriterion>, constraints: usize) -> UpsertItemBrief {
    UpsertItemBrief {
        acceptance,
        constraints: (0..constraints)
            .map(|n| Constraint::Note {
                text: format!("note {n}"),
            })
            .collect(),
        definition_of_done: None,
        risk: None,
    }
}

fn command(id: &str, run: String) -> AcceptanceCriterion {
    AcceptanceCriterion::Command {
        id: id.into(),
        title: "runs".into(),
        run,
        expect_exit: 0,
        cwd: None,
    }
}

#[test]
fn the_example_round_trips_and_its_pin_holds() {
    assert_eq!(fnv1a64(EXAMPLE.as_bytes()), EXAMPLE_FNV1A64);
    let pretty = serde_json::to_string_pretty(&example()).expect("serializes");
    assert_eq!(
        format!("{pretty}\n"),
        EXAMPLE,
        "byte-for-byte after pretty-printing"
    );
    upsert(example().acceptance, example().constraints.len())
        .validate()
        .expect("the example is valid");
}

#[test]
fn upsert_limits() {
    let many = |n: usize| {
        (0..n)
            .map(|i| command(&format!("c{i}"), "true".into()))
            .collect()
    };
    let cases: Vec<(&str, UpsertItemBrief, Option<&str>)> = vec![
        ("fifty criteria", upsert(many(50), 0), None),
        ("fifty-one criteria", upsert(many(51), 0), Some("length")),
        ("a hundred constraints", upsert(vec![], 100), None),
        (
            "a hundred and one constraints",
            upsert(vec![], 101),
            Some("length"),
        ),
        (
            "duplicate id",
            upsert(vec![command("a", "x".into()), command("a", "y".into())], 0),
            Some("duplicate_criterion_id"),
        ),
        (
            "run at 2000",
            upsert(vec![command("a", "x".repeat(2_000))], 0),
            None,
        ),
        (
            "run at 2001",
            upsert(vec![command("a", "x".repeat(2_001))], 0),
            Some("run_too_long"),
        ),
    ];
    for (name, input, expected) in cases {
        let codes: Vec<String> = match input.validate() {
            Ok(()) => vec![],
            Err(errors) => errors
                .field_errors()
                .values()
                .flat_map(|errs| errs.iter().map(|e| e.code.to_string()))
                .collect(),
        };
        match expected {
            None => assert!(codes.is_empty(), "{name}: unexpected {codes:?}"),
            Some(code) => assert!(codes.iter().any(|c| c == code), "{name}: got {codes:?}"),
        }
    }
}

#[test]
fn render_markdown_of_the_example_is_stable_with_manual_last() {
    let rendered = render_markdown(&item("Ship the release build"), &example());
    assert_eq!(rendered, SNAPSHOT);
}

const SNAPSHOT: &str = r#"# Ship the release build

Risk: medium

## Definition of done

All criteria met and approved by the team lead

## Acceptance

- `cmd_build` Build succeeds: `cargo build --release` exits 0
- `test_unit` Unit tests pass: test `cargo test --lib` passes
- `metric_coverage` Code coverage above 80%: `coverage` ≥ 80%
- `file_readme` README exists: `README.md` exists
- `absent_todo` No TODO comments: `TODO.md` does not exist

## Constraints

- do not touch `src/temp/*`
- may add the dependency `serde`
- change at most 10 files
- Keep database migrations minimal

## Needs a human

- `manual_review` Code reviewed by maintainer: A maintainer must review and approve the changes
"#;
