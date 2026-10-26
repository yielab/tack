//! `docs/contracts/mrp-v1/fixtures/` round-trip through the Rust types
//! byte-for-byte, their bytes are pinned, and the pull-request rendering of
//! `ready.json` is a fixed string.

use tack_protocol::mrp::{MergeReadinessPack, render_markdown, summary_line};

/// (fixture, its bytes, FNV-1a 64 of those bytes, `summary_line`)
const FIXTURES: [(&str, &str, u64, &str); 5] = [
    (
        "ready",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/ready.json"),
        0xc2b1_f791_13ac_3662,
        "Ready to merge (low risk)",
    ),
    (
        "not-ready",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/not-ready.json"),
        0x96cc_27df_71b8_f2b6,
        "Not ready (high risk)",
    ),
    (
        "manual-only",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/manual-only.json"),
        0x55b4_80c5_4c35_00c2,
        "Needs review (medium risk)",
    ),
    (
        "verifier-failed",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/verifier-failed.json"),
        0x733d_cfa7_0dbd_506c,
        "Not ready (high risk)",
    ),
    (
        "signed-with-next-steps",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/signed-with-next-steps.json"),
        0xc794_b12e_2565_8fc3,
        "Not ready (high risk)",
    ),
];

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn pack(json: &str) -> MergeReadinessPack {
    serde_json::from_str(json).expect("a MergeReadinessPack")
}

#[test]
fn every_fixture_round_trips() {
    for (name, json, pin, summary) in FIXTURES {
        assert_eq!(fnv1a64(json.as_bytes()), pin, "{name}: pin");
        let parsed = pack(json);
        let pretty = serde_json::to_string_pretty(&parsed).expect("serializes");
        assert_eq!(format!("{pretty}\n"), json, "{name}: byte-for-byte");
        assert_eq!(summary_line(&parsed), summary, "{name}: summary_line");
    }
}

#[test]
fn render_markdown_of_ready_is_stable() {
    let rendered = render_markdown(&pack(FIXTURES[0].1));
    assert_eq!(rendered, READY_SNAPSHOT);
}

const READY_SNAPSHOT: &str = r#"# Merge Readiness Assessment

**Decision:** merge

**Risk:** low

All criteria passed, risk is low, ready for merge

## Criteria

| Criterion | Kind | Status | Note |
|-----------|------|--------|------|
| `cmd_build` | command | passed | Build completed successfully |
| `test_unit` | test | passed | All unit tests passed |
| `metric_coverage` | metric | passed | Coverage above 80% |
| `file_readme` | file | passed | README.md exists |
| `absent_todo` | absent | passed | No TODO.md found |
| `manual_review` | manual | passed | Reviewed and approved |

## Risk Assessment

- All acceptance criteria passed
- Good test coverage
- No static analysis errors

## Verification

**Command:** `prill verify --repo /tmp/work --evidence /tmp/evidence --out /tmp/out --json`

**Exit code:** 0

**Duration:** 2.50s

**Output (tail):**

```
All criteria passed
```

## Mutation Testing

**Score:** 95.0%

**Killed:** 19

**Survived:** 1

## Static Analysis

| Severity | Count |
|----------|-------|
| Error | 0 |
| Warning | 1 |
| Note | 3 |

## Judge Assessment

**Model:** claude-3-5-sonnet (blind: true)

| Criterion | Verdict | Reason |
|-----------|---------|--------|
| `cmd_build` | pass | Build succeeds cleanly |
| `test_unit` | pass | All tests green |
| `metric_coverage` | pass | Coverage exceeds threshold |
| `file_readme` | pass | Documentation complete |

## Usage

**Tokens:** 1500 in, 800 out

## Produced By

prill 0.1.0
"#;

/// A required stage counts only when the pack lists it as `passed`.
#[test]
fn required_stages_are_read_off_the_pack() {
    let mut ready = pack(FIXTURES[0].1);
    let required = ["criteria".to_owned(), "suite".to_owned()];
    assert_eq!(
        ready.required_stage_missing(&required).as_deref(),
        Some("The required stage criteria is not in the pack")
    );
    let stage = |name: &str, status: &str, reason: Option<&str>| tack_protocol::mrp::StageRun {
        name: name.to_owned(),
        status: status.to_owned(),
        reason: reason.map(str::to_owned),
    };
    ready.stages = Some(vec![
        stage("criteria", "passed", None),
        stage("suite", "not_applicable", Some("no_suite_command")),
    ]);
    assert_eq!(
        ready.required_stage_missing(&required).as_deref(),
        Some("The required stage suite is not_applicable (no_suite_command)")
    );
    ready.stages = Some(vec![
        stage("criteria", "passed", None),
        stage("suite", "passed", None),
    ]);
    assert_eq!(ready.required_stage_missing(&required), None);
}

/// The verifier's own next steps win; without them, each failed criterion and
/// each failed stage but `criteria` is one thing to fix.
#[test]
fn what_to_fix_falls_back_to_the_failures() {
    let with_steps = pack(FIXTURES[4].1);
    assert_eq!(
        with_steps.what_to_fix(),
        with_steps.next_steps.clone().unwrap()
    );

    let mut bare = with_steps;
    bare.next_steps = None;
    bare.stages = Some(vec![
        tack_protocol::mrp::StageRun {
            name: "criteria".to_owned(),
            status: "failed".to_owned(),
            reason: None,
        },
        tack_protocol::mrp::StageRun {
            name: "static_analysis".to_owned(),
            status: "failed".to_owned(),
            reason: Some("2 new findings".to_owned()),
        },
    ]);
    let codes: Vec<String> = bare.what_to_fix().into_iter().map(|s| s.code).collect();
    assert_eq!(
        codes,
        [
            "criterion_failed:cmd_build",
            "criterion_failed:test_unit",
            "criterion_failed:metric_coverage",
            "stage_failed:static_analysis",
        ]
    );
}
