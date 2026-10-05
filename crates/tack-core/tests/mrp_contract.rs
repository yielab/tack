//! `docs/contracts/mrp-v1/fixtures/` round-trip through the Rust types
//! byte-for-byte, their bytes are pinned, and the pull-request rendering of
//! `ready.json` is a fixed string.

use tack_core::mrp::{MergeReadinessPack, render_markdown, summary_line};

/// (fixture, its bytes, FNV-1a 64 of those bytes, `summary_line`)
const FIXTURES: [(&str, &str, u64, &str); 4] = [
    (
        "ready",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/ready.json"),
        0x370d_4506_5b4c_7466,
        "Ready to merge (low risk)",
    ),
    (
        "not-ready",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/not-ready.json"),
        0xc963_83ec_d5ed_6138,
        "Not ready (high risk)",
    ),
    (
        "manual-only",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/manual-only.json"),
        0x573d_077f_b77c_46a4,
        "Needs review (medium risk)",
    ),
    (
        "verifier-failed",
        include_str!("../../../docs/contracts/mrp-v1/fixtures/verifier-failed.json"),
        0xc5c5_9686_9c6e_8a42,
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

**Command:** `assay --workspace /tmp/work --evidence /tmp/evidence --output /tmp/mrp.json`

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

assay 0.1.0
"#;
