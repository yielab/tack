# prill's own packs

These are prill's `mrp/v1` fixtures, copied verbatim from
`contracts/mrp-v1/fixtures/valid/` in the prill repository: the shape the runner's
`kind = "prill"` verifier translates into `mrp-v1`
(`crates/tack-runner/src/verify/prill.rs`). They are not `mrp-v1` documents and the
contract test does not pin them; the runner's tests translate each one and prove the
result round-trips through `tack_protocol::mrp::MergeReadinessPack`.

| File | prill's decision | What it exercises |
| --- | --- | --- |
| `m2-sections.json` | `human-review` | every section: criteria, suite, mutation, static analysis, judge, risk signals, a flag |
| `reject-failed-criterion.json` | `reject` (prill exits 1) | a failed command criterion with a blocking reason and a brief digest |
| `human-review-no-sandbox.json` | `human-review` | the minimal pack a host run without a sandbox produces |
