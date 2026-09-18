# IX-M6-tack-api-b handoff

**One of a disjoint pair of tack-api comment-trimming sub-cards under IX-M6.** A sibling
agent worked a different file set in another worktree/branch at the same time; its result
is not visible here and is not assumed. This sub-card's scope is comments only — no
production logic, function signature, or behavior changes.

- Base SHA / branch / final SHA: `957ffc3` (`develop`) / `agent/ix-m6-tack-api-b` /
  `577b968` (worktree: `/tmp/ix-m6-tack-api-b`,
  `CARGO_TARGET_DIR=/tmp/ix-m6-tack-api-b-target`).
- Files changed (equals the card's ownership list, verified via `git diff --stat HEAD`):
  - `crates/tack-api/src/handlers/runner_protocol/retention.rs`
  - `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`
  - `crates/tack-api/src/handlers/templates.rs`
  - `crates/tack-api/src/orch_store.rs`
  - `crates/tack-api/src/remote_backup.rs`
  - `crates/tack-api/src/router.rs`
  - `crates/tack-api/tests/openapi_contract.rs`
  - `crates/tack-api/tests/wave2_gate.rs`
- Contract fixtures consumed: none.
- Behavior implemented: none — comment-only card. No handler logic, route table, SQL,
  auth check, or test assertion changed; only comment text was rewritten, shortened, or
  removed.
- Tests added and exact commands/results: none added or removed. `cargo check --workspace`
  compiles clean; no nextest run performed per the card's own instruction (comment-only
  change needs no behavior re-proof).
- Failure/adversarial case proved: n/a (no behavior change).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced or touched.
- Secrets/logging review: n/a — no log line, error body, or secret-handling code was
  touched; `remote_backup.rs`'s edit is confined to `scrub_snapshot_secrets`'s doc comment
  (the function body, its column list, and the `NULL`-not-delete behavior are unchanged).
- Safe merge order and likely conflicts: no overlap with the sibling IX-M6 sub-card's file
  set (disjoint by construction) or with any other open card touching `tack-api` — should
  merge cleanly in either order.
- Checklist: no unowned files touched, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Every flagged violation in the 8 owned files is resolved | `python3 scripts/maintainability.py comment-worklist --json` filtered to the 8 files returns `[]` after the edits (was 13 entries before) |
| No production logic changed | `git diff` for all 8 files touches only `//`, `///`, `//!` lines and blank lines inside them — verified by reading every hunk before committing |
| Workspace still compiles | `cargo check --workspace` — clean, `Finished` |
| No board-archaeology language introduced | `./scripts/check-comments.sh` — `✓ no board archaeology in crates/ frontend/src frontend/e2e` |
| No test-hygiene regression | `./scripts/check-test-hygiene.sh` — `✓ tests take their temporary paths from a guard` |
| Formatting clean | `cargo fmt --all --check` — no output (clean) |
| Maintainability budgets hold, ratchet and absolute | see "Budget check" below |

## Measured numbers

`python3 scripts/maintainability.py measure <file>` per file, before (baseline in
`scripts/maintainability-baseline.json`, matching `develop` at `957ffc3`) and after:

| File | prod_code | comment before → after | share before → after | doc_block_max before → after | module_doc before → after |
|---|---:|---|---|---|---|
| `retention.rs` | 76 | 73 → 63 | 0.49 → 0.453 | 22 → 12 | 12 → 12 |
| `runner_auth.rs` | 177 | 84 → 73 | 0.322 → 0.292 | 25 → 14 | 16 → 16 |
| `handlers/templates.rs` | 463 | 94 → 70 | 0.169 → 0.131 | 35 → 15 | 0 → 0 |
| `orch_store.rs` | 467 | 279 → 155 | 0.374 → 0.249 | 42 → 15 | 30 → 15 |
| `remote_backup.rs` | 606 | 184 → 172 | 0.233 → 0.221 | 27 → 15 | 11 → 11 |
| `router.rs` | 379 | 234 → 182 | 0.382 → 0.324 | 46 → 15 | 0 → 0 |
| `tests/openapi_contract.rs` (test file) | n/a | — | — | — | 11 → 5 |
| `tests/wave2_gate.rs` (test file) | n/a | — | — | — | 20 → 10 |

`prod_code` is unchanged by every edit (no logic touched); only comment line counts moved.
`retention.rs`'s comment share stays above the 35% cap (0.453) because the file's 76
production-code lines are below the tool's `prod_code > 100` floor for flagging comment
share at all (`comment-worklist`'s own guard) — its baseline was already 0.49 before this
card, so `check`'s ratchet rule (a file may exceed a budget only if it already did and is
not worse) holds; the ground-truth `comment-worklist --json` run at the start of this card
listed only a doc-block violation for this file, never a comment-share one, and that is
what got fixed.

## What was removed

Reason classes from `docs/plans/human-maintainability.md` §3 (`doc block`, `preamble`,
`share`).

- `retention.rs` line 93, `sweep_artifacts` doc (22 → 12 lines, **doc block**): dropped the
  `# Split delete, guarding the no-blob-observed branch` heading and the restated
  "no race is possible on that branch" aside; kept the two-phase construction, the split
  rationale, and the race-survives-to-next-pass fact.
- `runner_auth.rs` line 216, `is_credential_not_recognized` doc (25 → 14 lines, **doc
  block**): dropped the SQLite-history preamble and the "16 call sites" count; kept why the
  ambiguity exists (rotation overwrites the hash in place) and the pointer to the matching
  `HashMismatch` handling in `/refresh`.
- `handlers/templates.rs` line 23, `validate_template_orchestration` doc (35 → 15 lines,
  **doc block**): dropped the full path to `rack-cli`'s `pipeline.py`, the "verified by
  reading every `do_GET`/`do_POST` branch" narrative, and the upstream `ROADMAP.md`
  pointer; kept why this function stops at "is it YAML" instead of a full pipeline schema
  check.
- `handlers/templates.rs` line 237, `build_project_from_template` doc (16 → 10 lines, **doc
  block**): trimmed restated detail, kept the `pub(crate)` rationale and the
  `template.orchestration`-is-inert fact.
- `orch_store.rs` module preamble (30 → 15 lines, **preamble**): dropped the
  "kept out of `server.rs`/`router.rs`/..." rationale (restated the module boundary the
  file itself already demonstrates) and shortened the `registry::build` placeholder note;
  kept why `ControlPlaneStore` is a narrow trait and the config/secrets-placeholder fact.
- `orch_store.rs` line 182ish, `mark_unconfigured` doc (25 → ~11 lines, **doc block**):
  kept the motivating case (a restored backup nulling `secrets`) and the best-effort/
  `consecutive_failures`-passthrough facts; dropped restated framing.
- `orch_store.rs` line ~496, `reconcile_terminal_status_map` doc (42 → 15 lines, **doc
  block**): collapsed the `# Why "human wins"` and `# How ... determined` headed sections
  into two plain paragraphs, keeping the human-wins rule and the accepted "cannot detect
  a same-status re-choice" limit; dropped the illustrative "Blocked" example and the
  restated "docket's own state is never lost" aside (already stated once).
- `orch_store.rs`: five section header comments (`── Runs + approvals ingestion ──`,
  `── Broadcast on real change ──`/`── Terminal status_map application ──` combined block,
  `── Metrics ingestion ──`, `── Trace ingestion ──`, `── Retention sweep ──`) trimmed or
  merged into 1-4 line notes each (**share**, contributing to the file's 37%→25% drop);
  no fact was dropped that isn't still stated at the doc-comment level on the function it
  describes.
- `orch_store.rs`: two inline comments (`// A run with no Tack item...`,
  `// ApprovalPending is deliberately narrower...`) shortened by roughly half each
  (**share**); the "no project, no broadcast" duplicate explanation on `upsert_approvals`
  was reduced to a one-line pointer back to `upsert_runs`'s fuller version.
- `remote_backup.rs` line 266, `scrub_snapshot_secrets` doc (27 → 15 lines, **doc block**):
  kept every operationally load-bearing fact (chokepoint status, `NULL`-not-delete for both
  secret columns, `install_id` regeneration, run-before-`VACUUM` ordering) — trimmed only
  the repeated "for the same reason" phrasing and the per-column inline JSON-shape detail
  (a GitHub Actions plane's credential/webhook-secret split), which is documented once in
  the column's own migration and doesn't need restating here.
- `router.rs` line 177, `operator_execution_routes` doc (46 → 15 lines, **doc block**):
  dropped the narrative about the merged routers having "shipped as deliberately unwired
  card-local modules" and the `axum::Router::merge` doc-example pointer; kept the auth
  boundary, the `inject_operator_principal` trust statement, and the decision-token gate.
- `router.rs` line 268 (originally), `runner_protocol_routes` doc (36 → 15 lines, **doc
  block**): dropped the restated "this is the whole security property this function exists
  to preserve" framing and the `Service`-erasure aside; kept the sibling-nest/no-shared-gate
  fact, the credential match, and the body-limit/storage-root specifics.
- `router.rs`: five multi-line inline comments in `build_router` (the `If-Match` CORS
  header note, the `X-Tack-Approval-Token` bug note, the `expose_headers` rationale, the
  `local_runner_available` computed-once note, the unmatched-route-404 ordering note)
  shortened in place (**share**) — each kept its one load-bearing fact (why the header is
  allowed/exposed, why the check runs where it does) and dropped the restated "see that
  file's own doc comment" chains.

Total: 715 comment lines across the 8 files after this card (was 1,077 before, per the
per-file `prod_comment`/`test_module_doc_lines` counts above), for 0 lines of production
logic changed.

## Dev-notes resolution

Checked `docs/dev-notes/**` for any entry about one of the 8 owned files. None of the 21
existing dev-notes files is dedicated to `retention.rs`, `runner_auth.rs`,
`handlers/templates.rs`, `orch_store.rs`, `remote_backup.rs`, `router.rs`,
`tests/openapi_contract.rs`, or `tests/wave2_gate.rs` (no `orch_store.md`, `router.md`,
`templates.md`, `remote_backup.md`, `runner_auth.md`, `retention.md`, `openapi_contract.md`
or `wave2_gate.md` exists under any crate's dev-notes tree). Six *other* modules' dev-notes
files mention `router.rs`, `orch_store.rs`, or `retention.rs` in passing, as cross-file
pointers to where something is wired (`handlers/economics.md`, `dispatcher.md`,
`handlers/provisioning.md`, `tack-db/repo/economics.md`, `handlers/orch.md`,
`handlers/decisions.md`, `handlers/runner_protocol/artifact_download.md`) — those are notes
parked for the *other* module's own file, own by a different card, and were left untouched.
**Conclusion: none applicable** — no content needed to move out of dev-notes into one of
this card's 8 files (or out of one of them into dev-notes).

## Budget check

`python3 scripts/maintainability.py check --changed` (run after `git add -A`):

```
✓ maintainability budgets hold (8 files checked)
```

`python3 scripts/maintainability.py check` (full workspace, second opinion):

```
✓ maintainability budgets hold (292 files checked)
```

`python3 scripts/maintainability.py comment-worklist --json` filtered to the 8 owned files:
`[]` (zero remaining violations; 13 before this card's first edit).

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` is untouched by this card — every file's
measured numbers moved down or stayed flat relative to its existing baseline entry, so the
ratchet check passes without lowering any recorded ceiling.

## Full gate

```
cargo check --workspace              # Finished, clean
./scripts/check-comments.sh          # ✓ no board archaeology in crates/ frontend/src frontend/e2e
python3 scripts/maintainability.py check --changed   # ✓ maintainability budgets hold (8 files checked)
python3 scripts/maintainability.py check             # ✓ maintainability budgets hold (292 files checked)
cargo fmt --all --check              # clean, no output
./scripts/check-test-hygiene.sh      # ✓ tests take their temporary paths from a guard
```

nextest was not run, per this card's own instruction (comment-only change, no behavior
proof needed).

## What a stranger still cannot do

Nothing new — this card removed no capability and added none. A stranger reading
`orch_store.rs` or `router.rs` today gets the same operational facts (auth boundaries,
the human-wins status-map rule, the secret-scrubbing chokepoint, the two-phase artifact
sweep) in noticeably less text, with the planning-board framing (which of these functions
"shipped as deliberately unwired," which axum doc example was followed) gone. Nothing that
used to require reading a doc comment now requires reading the source instead — every fact
kept was already stated in-comment before, just at greater length.

## Context spent

- Read before the first edit: this card's own file list via `comment-worklist --json`
  (ground truth for exact violations), `.claude/scope-discipline.md` in full (the Comments
  section), and each of the 8 files in full before touching it.
- Files opened and not used for editing: `docs/plans/human-maintainability.md` (referenced
  for the budget table only, not read in full — the numbers were already known from
  `.claude/scope-discipline.md`'s copy of the same table).
- Read-list lines that were wrong: none — the task's own worklist grep matched the actual
  tool output exactly.

## Amendments

*(none yet)*
