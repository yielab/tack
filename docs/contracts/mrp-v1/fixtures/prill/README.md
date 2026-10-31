# prill's own packs

These are prill's `mrp/v1` fixtures, copied verbatim from
`contracts/mrp-v1/fixtures/valid/` in the prill repository at commit `6b95423`
(committed 2026-10-14): the shape the runner's
`kind = "prill"` verifier translates into `mrp-v1`
(`crates/tack-runner/src/verify/prill.rs`). They are not `mrp-v1` documents and the
contract test does not pin them; the runner's tests translate each one and prove the
result round-trips through `tack_protocol::mrp::MergeReadinessPack`.

| File | prill's decision | What it exercises |
| --- | --- | --- |
| `m2-sections.json` | `human-review` | every section: criteria, suite, mutation, static analysis, judge, risk signals, a flag |
| `reject-failed-criterion.json` | `reject` (prill exits 1) | a failed command criterion with a blocking reason and a brief digest |
| `human-review-no-sandbox.json` | `human-review` | the minimal pack a host run without a sandbox produces |
| `m6-fields.json` | `reject` (prill exits 1) | the M6 fields: the `coverage` stage and section, `nextSteps`, `tools`, `antiTautology` |

Tack carries prill's stages (the `coverage` one included) and its `nextSteps` into the pack; the
`coverage` section itself (the share of changed lines covered, the uncovered ones) has no section in
`mrp-v1` and is left out, which `prills_coverage_section_is_not_translated_but_its_stage_is` pins.
