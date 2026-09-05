# Repair stale test-file pointers — tack-api/src, tack-orch, tack-db

Comments-only repair of citations left dangling by the 73→32 test-binary
regroup (`88cb4a4` and its component branches). Scope: `crates/tack-api/src/`,
`crates/tack-orch/` (`src/` and `tests/`), `crates/tack-db/` — the last one
extended from the card's stated "`tests/` — 5 lines" to include `src/` too
(see "Scope note" below); `crates/tack-api/tests/` was left untouched, owned
by a concurrent agent.

## Totals

42 sites fixed against the card's 42-line estimate, but not with the same
per-area split the card gave:

| Area | Card estimate | Actual sites |
|---|---|---|
| `crates/tack-api/src/` | 23 | 23 |
| `crates/tack-orch/` (src+tests) | 14 | 14 |
| `crates/tack-db/tests/` | 5 | 2 |
| `crates/tack-db/src/` | (not listed) | 3 |

**Repoint vs. rewrite:** 36 repointed (old filename swapped for the crate-
qualified new path, or new path + corrected function name), 6 rewritten to
state the underlying fact instead of naming a file — the four `tack-api/src/
handlers/executions.rs` sites whose doc comments feed `docs/openapi.json`
(see below), the `legacy_bridge.rs` bullet enumerating five renamed
`tack-api` test files as coverage evidence (rewritten to name the coverage
categories instead, since enumerating five now-differently-named files was
exactly the kind of pointer due to go stale again), and the `handlers/orch.rs`
Prometheus-metrics comment (see "Claim found false" below).

## Scope note: `crates/tack-db/src/`

The card names three areas totalling 42 lines, with `tack-db/tests/` at 5, but
the actual grep recipe against `crates/tack-db/tests/` alone (line-start
comments citing a name from the tracked-vs-cited diff) only turns up 2 dead
citations (`event_artifact_retention.rs:4`, `status_update_checked.rs:8`,
both fixed). The verification command the card itself gives at the end,
though, scans the whole crate (`crates/tack-db`, not `crates/tack-db/tests`)
— and that surfaces 3 more, in `crates/tack-db/src/repo/{items,execution}.rs`,
citing `version_concurrency_test.rs`, `f2_event_artifact_retention_test.rs`,
and `execution_retention_test.rs`. 2 + 3 = 5, matching the card's number
exactly, so the "5 lines" total was almost certainly always meant to include
these three `src/` sites — "tack-db/tests/" reads as a mislabel of "tack-db"
rather than a real scope boundary. Fixed all 5 (all straightforward
repoints; each claim re-verified against the destination file — see below).

## Sites needing real care

**Two `#[path]`-duplication `dead_code` explanations changed shape, not just
filenames** (`artifact_download.rs`'s module doc, `retention.rs`'s
`sweep_events`/`sweep_artifacts` comment, and the matching precedent note in
`runner_protocol.rs`). Before the regroup, `f2_artifact_events_test.rs` and
`c2_handlers_test.rs` were two separate nextest binaries, so "dead-code
analysis is per compiled binary" was the literal mechanism. After the
regroup they are `runner_protocol/artifact_events.rs` and
`runner_protocol/lifecycle.rs` — two sibling **modules of the same**
`runner_protocol` binary (confirmed: `crates/tack-api/tests/runner_protocol.rs`
declares both as `mod`s). The `#[allow(dead_code)]` is still genuinely needed
— each file loads its own independent copy of `runner_protocol.rs` (and
`retention.rs`/`artifact_download.rs` beneath it) via its own `#[path]`
(`#[allow(clippy::duplicate_mod)]`, confirmed present in both), and the two
`#[path]` copies are distinct items even inside one binary — but the old
"per compiled binary" framing was no longer accurate, so all three comments
were rewritten to say "distinct `#[path]`-loaded module trees, not distinct
binaries" and each underlying "who calls what" claim was re-verified by
grep (`with_artifact_storage_root` is called from `artifact_events.rs`,
never from `lifecycle.rs`; neither calls `sweep_events`/`sweep_artifacts`
directly) before touching the wording.

**The two protected contract oracles** (`docket_tick_contract_test.rs`,
`docket_wire_contract_test.rs`) had 7 stale citations between them, all now
fixed, including one non-trivial case: `ingestion_test.rs` (cited 4 times in
`docket_tick_contract_test.rs`) was never a pure rename — `git show dfb8385
--stat` shows it was split, its tests moving mostly into the new
`ingestion/runs.rs` and its byte-identical shared helpers (`setup_repo`,
`seed_workspace`, `seed_project`, `TestRepoStore`, …) factored into the new
`ingestion/support.rs`, per `docs/agent-handoffs/test-architecture/
regroup-orch.md`. Citations were repointed accordingly: the "pattern copied
from" prose now names `ingestion/runs.rs`/`ingestion/traces.rs`, the fixture
mirror comment now names `ingestion/support.rs` alone (where the mirrored
`setup_repo`/`seed_workspace`/`seed_project` now actually live), and the
`TestRepoStore` "duplicated rather than shared" comment now says duplicated
from `ingestion/support.rs`'s copy specifically. Ran
`UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test)
| binary(docket_wire_contract_test)'` afterward (18/18 passed) and
`git diff --exit-code crates/tack-orch/tests/golden/` is clean — the goldens
did not move.

## `docs/openapi.json` — changed, regenerated

Four sites are in doc comments (`///`) on `RunnerV1ErrorEnvelope` and
`MeasurementSourceSchema` in `crates/tack-api/src/handlers/executions.rs`,
both `utoipa::ToSchema` types whose doc comments are pulled verbatim into the
published spec (confirmed: `grep -c c1_handlers_test docs/openapi.json` was
2 before this change). Per the card's own instruction for this file, these
were **rewritten**, not repointed — a reference to an internal test's
`#[path]`-loading mechanics has no business in a public API schema
description regardless of whether the filename is current. Regenerated with
`UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'`
then `cd frontend && npm run gen:api` (needed `npm install` first — frontend
`node_modules` was not present in this worktree). Both diffs are exactly the
two rewritten descriptions, nothing else:
`docs/openapi.json` (2 lines) and `frontend/src/shared/api/schema.gen.ts`
(19 lines, the same text reflowed into JSDoc). No other `docs/openapi.json`
schema text changed.

## Claims found false, not quietly repaired

Two comments cited a test file/function that, on verification, does not
and — as far as `git log -p` on the pre-regroup file shows — never did exist
under that name, independent of this card's rename work:

1. `crates/tack-api/src/handlers/orch.rs`'s `GET /api/metrics` comment cited
   `tests/orch_metrics_test.rs`'s `get_metrics_response_parses_with_the_real_
   prometheus_parser` as proof the handler's Prometheus output round-trips
   through the real parser. No test of that name exists anywhere in the
   tree now, and diffing tack-db's actual `orch_metrics_test.rs` (now
   `migrations/orch_metrics.rs`) at the commit before this regroup shows its
   tests are all migration/table-existence checks — none of them touch HTTP
   or the Prometheus format at all. Nothing in `tack-api/tests` or
   `tack-orch/tests` round-trips this handler's own output through
   `adapters::prometheus::parse` either. Rewrote the comment to state the
   one fact that is true and checkable — the handler reuses the same
   `adapters::prometheus::parse` that decodes docket's own `/metrics`
   responses — without citing a specific proof that isn't there. Flagging
   for whoever owns `handlers/orch.rs` next: either write that round-trip
   test, or drop the "round-trips cleanly" framing to match what's actually
   proven.
2. `crates/tack-api/src/handlers/decisions.rs`'s module doc cited
   `f1_decisions_test.rs`'s `self_resolution_is_denied_*` (a wildcard,
   implying a family) and, separately, `expiry_never_touches_item_status`
   and `resolve_never_touches_item_status` as specific proofs. Checked
   `git show ead643f^:crates/tack-api/tests/f1_decisions_test.rs` (the last
   version before the regroup touched it): the actual self-resolution test
   was always named `self_resolution_via_a_valid_runner_bearer_credential_
   is_denied_and_writes_nothing` (singular, not a family), the expiry test
   was always `expiry_denies_records_audit_and_never_marks_the_item_done_
   even_against_a_valid_allow_answer`, and no test named
   `resolve_never_touches_item_status` has ever existed — the closest real
   coverage is that `resolve`'s code path never touches the `items` table at
   all (a structural fact, not a specific test) plus the expiry tests'
   `item_status`-unchanged assertions. Both stale names predate this
   regroup and are unrelated to the file rename it did. Repointed the
   self-resolution citation to the real, current test name and rewrote the
   "never touches item status" claim to what is actually provable (the
   structural fact plus the expiry tests' own assertions) instead of citing
   two non-existent test names.

**Not fixed, out of this card's scope:** `crates/tack-api/src/openapi.rs`
line ~644 makes the same `self_resolution_is_denied_*` claim in a spec-
feeding `OperationBuilder` description — found while investigating (1)
above, but it cites a test *name*, not a *filename*, so it never appeared in
this card's grep-based worklist. Whoever touches that file next should
correct it to the real test name (see item 2 above) since it currently ships
the same false claim into `docs/openapi.json`.

**Also noted, not touched:** `crates/tack-api/tests/runner_protocol/
decisions.rs`'s own module doc still cites `c1_handlers_test.rs` (dead) —
inside `crates/tack-api/tests/`, explicitly out of this card's scope
(owned by a concurrent agent this session).

## Verification

- `./scripts/check-comments.sh` — passes.
- `cargo fmt --all --check` — passes.
- `cargo clippy --workspace --all-targets -- -D warnings` — passes.
- `cargo nextest run --workspace` — 1392 run, 1392 passed, 6 skipped.
- `cargo nextest list --workspace | wc -l` — 1392 before and after (the
  `88cb4a4` baseline was confirmed 1392 before any edit in this session).
- `git diff --exit-code crates/tack-orch/tests/golden/` — clean.
- Final grep (the card's own recipe, `crates/tack-api/src crates/tack-orch
  crates/tack-db`) — prints only `foo.rs` (prose placeholder).
- `git diff -U0 | grep '^[+-]' | grep -v '^\(+++\|---\)' | grep -vE
  '^[+-][[:space:]]*(//|///|//!)'` — empty: every changed line in every
  crate source file is a comment line. `git diff --summary` shows no
  renames/creates/deletes/mode changes. The only non-comment-line diffs in
  the whole change are the two generated files (`docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts`), regenerated by the documented
  commands, never hand-edited.
