# IX-X3-api handoff

Base `ffb60a1`, branch `agent/ix-x3-api`. The four named files, split into seven;
`scripts/maintainability.py` (EXCLUSIONS) + its baseline.

## Method

Built a `Fixture` struct per file (router/repo/clock/attempt, plus short methods for
the repeated runner-protocol calls: `claim`/`accept`/`start`/`events`/`decision`/
`artifact`/`complete_default`/`heartbeat`/`recovery_observation`) and free wire-body
builders (`one_event`, `artifact_manifest_body`, `heartbeat_body`, `events_body`,
etc.) so a call site is one line instead of a 6–10-line `json!({...})` literal.
Table-driven cases moved to a module-level struct with `for case in cases` (no
destructuring pattern) so rustfmt doesn't re-explode the loop header. Split
`lifecycle.rs` into itself (claim-through-completion) + `enrollment.rs`
(enroll/refresh/rotation/auth-boundary), and `chaos_recovery.rs` into itself
(fencing/artifacts/replay/corruption) + `chaos_races.rs` (races, revocation) +
`chaos_common.rs` (shared infra, `#[path]`-loaded from both, `#![allow(dead_code)]`
since each importer uses a different subset) — the same duplication pattern
`runner_protocol.rs` already uses for `lifecycle`/`artifact_events`/`enrollment`.

## Before / after

| file | lines before | lines after | tests before | tests after | max body before | max body after |
|---|---|---|---|---|---|---|
| lifecycle.rs | 1 821 | 1 183 | 14 | 14 | 270 | 50 |
| enrollment.rs (new) | — | 592 | — | 7 | — | 89 |
| artifact_events.rs | 1 154 | 1 086 | 13 | 16 | 105 | 43 |
| decisions.rs | 1 069 | 1 055 | 19 | 19 | 98 | 72 |
| chaos_recovery.rs | 1 243 | 574 | 8 | 7 | 106 | 70 |
| chaos_races.rs (new) | — | 306 | — | 3 | — | 97 |
| chaos_common.rs (new) | — | 504 | — | 0 | — | n/a |

Totals: 5 287 → 5 300 test lines (flat: fixture methods add lines that reused JSON
literals removed), 54 → 66 tests (+12, all splits — no assertion lost). Workspace
`test_to_prod_ratio` 1.234 → 1.259 (no production code touched; the new fixtures and
wire-body builders are test-only lines with no prod-code counterpart).

## Coverage

`cargo llvm-cov -p tack-api --summary-only`, TOTAL Cover column: **69.26% before,
69.26% after** (identical line/region/function counts — no production code changed).

## Deletion (named, per the rule)

`lifecycle.rs`'s `heartbeat_and_completion_replay_vs_conflict` had two halves: a
heartbeat replay-vs-conflict proof (kept, renamed
`heartbeat_replay_returns_original_success_conflicting_retry_rejected`) and a
completion replay-vs-conflict proof with fewer assertions than the file's own
`completion_replay_changed_content_is_idempotency_conflict` (same claim, same layer,
already pinned there in full: `idempotency_conflict` code, `retryable: false`,
replay-count, unchanged state). Dropped the weaker duplicate, not flagged by
`duplicate-tests` (whole-body similarity, not sub-section).

## Unmet (documented in EXCLUSIONS with these reasons)

- `lifecycle.rs`, `artifact_events.rs`, `decisions.rs`: file length still over 1 000
  (183 / 86 / 55 over) — each already split or fixture-ized once; no further split
  attempted for the residual.
- One 2-case table each in `lifecycle.rs` (50, 46) and `artifact_events.rs` (43):
  case struct hoisted to module scope, loop body itself still a few lines over.
- `enrollment.rs::concurrent_refresh_rotations_exactly_one_wins` (89),
  `decisions.rs::concurrent_resolves_serialize_to_exactly_one_winner` (72),
  `chaos_races.rs::revoked_credential_rejected_everywhere_freezes_attempt` (97):
  genuine lock-race/adversarial narratives (spawn, held `BEGIN IMMEDIATE`, join,
  multi-way outcome check, or a multi-step revocation proof) — compressing further
  would hide the interleaving or narrative each exists to prove.
- `chaos_recovery.rs::stale_fence_writes_nothing_across_every_mutation_route` (70):
  one shared superseded-fence setup tried against 5 distinct fenced routes.

## Gate tails

`maintainability baseline && check`: bare, green (299 files) — `rs_files`/`baseline`
read `git ls-files`, so the three new files needed `git add` before either the
dead-pointer comment check or `baseline` would see them; re-ran both post-commit.
`duplicate-tests crates`: 0 pairs, every crate. `cargo fmt --all --check` (root +
`tack-desktop`) and `cargo clippy --workspace --all-targets -- -D warnings`: clean
(clippy needed `#[allow(clippy::duplicate_mod)]` on both `chaos_common.rs` loads).
`cargo nextest run --workspace -E 'package(tack-api)'`: 516 passed. Plain
`cargo test -p tack-api --tests -q` × 3: no FAILED/panicked lines.
