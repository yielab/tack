# IX-M6-dev-notes handoff

- Base SHA / branch / final SHA: `fd8c9011636e2a2ffb8f71cd56d6bb2238653827` / `agent/ix-m6-dev-notes` / (set at commit)
- Files changed (must equal ownership list): the 12 files under `docs/dev-notes/` (deleted) and
  the `//!` preamble of each origin module (`crates/tack-api/src/handlers/economics.rs`,
  `handlers/provisioning.rs`, `handlers/runner_protocol/artifact_download.rs`,
  `orch_runtime.rs`, `sprint_dispatch.rs`; `crates/tack-db/src/repo/economics.rs`;
  `crates/tack-orch/src/adapters/legacy_bridge.rs`, `model_policy/wiring.rs`,
  `scheduler/mod.rs`; `crates/tack-runner/src/git.rs`, `harness/event_sink.rs`,
  `transport.rs`); plus the two destinations content moved to
  (`docs/adr/0060-docket-control-plane-disposition.md`,
  `crates/tack-orch/tests/fixtures/README.md`, `docs/contracts/runner-v1/README.md`) and one
  table row in `docs/plans/human-maintainability.md` recording the directory's deletion.
- Contract fixtures consumed: none changed; `docs/contracts/runner-v1/README.md` gained one
  explanatory paragraph, no fixture bytes touched.
- Behavior implemented: none — comments and docs only, `cargo check --workspace` is the proof
  nothing else moved.
- Tests added and exact commands/results: none; no test file was touched (forbidden by scope).
- Failure/adversarial case proved: n/a (mechanical/documentation card).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see *What a stranger still cannot do* and the
  `scripts/maintainability.py` escalation below.
- Secrets/logging review: n/a, no code behavior changed.
- Safe merge order and likely conflicts: no open Wave 29/30/31 card touches these 12 files
  (§IX.2); should merge cleanly. `docs/plans/human-maintainability.md`'s dev-notes row and
  `docs/adr/0060-*.md` are otherwise unowned this wave.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `docs/dev-notes/` no longer exists | `git status --short \| grep dev-notes` shows 12 `D` entries staged, nothing untracked; `find docs/dev-notes -type f` (after commit) returns nothing |
| Every origin module's preamble is ≤ 30 lines and does not worsen its comment-share ratchet | `python3 scripts/maintainability.py check --changed` → `✓ maintainability budgets hold (12 files checked)` |
| No `//!`/`///` comment reintroduces board archaeology | `./scripts/check-comments.sh` → `✓ no board archaeology in crates/ frontend/src frontend/e2e` |
| Workspace still compiles | `cargo check --workspace` → `Finished` |
| The one design-rationale note whose subject already has an ADR was appended there, not duplicated in the module | `git diff docs/adr/0060-docket-control-plane-disposition.md` adds one new `## Provider-scoped ids and the normalized-attempt projection` section; `legacy_bridge.rs`'s preamble now points to it instead of re-deriving the same text |
| The one vendor/wire-behavior note was moved to the fixture README that already covers that fixture | `git diff crates/tack-orch/tests/fixtures/README.md` adds the docket `POST /pods` 500/rollback paragraph next to the existing `POST /pods` bullet |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

Every number below is `scripts/maintainability.py measure --totals`, run against this branch
(`prod`/`test` totals cover only `.rs` files — the ADR/README moves are not counted here).

- Before (clean `develop` tip, `fd8c901`): `prod=55448 (comments 10993) test=71211 ratio=1.284
  tests=1408`
- After (this branch, working tree): `prod=55599 (comments 11144) test=71211 ratio=1.281
  tests=1408`
- Test line count and test count are unchanged (`71211`, `1408`) — no test file was touched,
  matching the card's scope.
- `find docs/dev-notes -type f | wc -l`: `0` (directory absent).

## What a stranger still cannot do

A reader who lands on `crates/tack-orch/src/scheduler/mod.rs` still cannot see, from that file
alone, that `wiring::choose_request_for_runner` is the only production caller of this module's
`select`/`batch` functions or that it fetches its candidate data from
`agent_runners`/`agent_fleet_members`/`execution_requests` — that fact was cut for budget (see
*What was removed*) and now has to be found by reading `wiring.rs` directly. Everything else the
twelve notes carried is either in a preamble, in ADR 0060, in the two fixture/contract READMEs,
or was already stated at the crate level (`tack-orch`'s `lib.rs` doc already states the
`tack-core`+`tack-db`-only dependency rule cited by the old scheduler note).

## Budget check

`python3 scripts/maintainability.py check --changed` (final tree):

```
✓ maintainability budgets hold (12 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=55448 (comments 10993) test=71211 ratio=1.284`
- After: `prod=55599 (comments 11144) test=71211 ratio=1.281`

`python3 scripts/maintainability.py comment-worklist`:

```
0 blocks over budget in 0 files (largest first):
```

`python3 scripts/maintainability.py extract-module-docs`:

```
0 module preambles over 30 lines
```

`cargo fmt --all --check` (workspace) and `cargo fmt --all --check --manifest-path
crates/tack-desktop/Cargo.toml`: both silent (no diff).

`cargo check --workspace`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`.

Per-file detail, budget vs. baseline (`was` = the file's entry in
`scripts/maintainability-baseline.json`; a `*` marks a file whose comment-share ratchet, not the
30-line cap, was the binding constraint):

| File | Metric | Final | Cap | Baseline | Verdict |
|---|---|---:|---:|---:|---|
| `handlers/economics.rs` | `src_module_doc_lines` | 28 | 30 | 8 | pass |
| `handlers/provisioning.rs` | `src_module_doc_lines` | 30 | 30 | 8 | pass (was 35, trimmed) |
| `handlers/runner_protocol/artifact_download.rs` | `src_module_doc_lines` | 23 | 30 | 8 | pass |
| `orch_runtime.rs`* | `src_comment_share` | 0.49 | 0.35 | 0.49 | pass (was 0.568, trimmed) |
| `sprint_dispatch.rs` | `src_module_doc_lines` | 28 | 30 | 8 | pass |
| `repo/economics.rs` | `src_module_doc_lines` | 23 | 30 | 8 | pass |
| `adapters/legacy_bridge.rs`* | `src_comment_share` | 0.533 | 0.35 | 0.538 | pass (was 0.625, trimmed + ADR split) |
| `model_policy/wiring.rs`* | `src_comment_share` | 0.34 | 0.35 | 0.293 | pass (was 0.386, trimmed) |
| `scheduler/mod.rs`* | `src_comment_share` | 0.389 | 0.35 | 0.421 | pass (was 0.686, trimmed) |
| `git.rs` | `src_module_doc_lines` | 24 | 30 | 8 | pass |
| `harness/event_sink.rs` | `src_module_doc_lines` | 24 | 30 | 8 | pass |
| `transport.rs` | `src_module_doc_lines` | 30 | 30 | 8 | pass (was 31, trimmed) |

## What was removed

Per note, per destination, per §3/comment-rule class. "Preamble" = kept in the module's `//!`
block; "ADR" = moved to `docs/adr/0060-*.md`; "fixture README" = moved to a fixture/contract
README; "nothing" = dropped as history/narrative/already-stated-elsewhere.

| Note | Destination(s) | Kept | Dropped, and why (§3 class) |
|---|---|---|---|
| `handlers/economics.md` | preamble (28 ln) | gating, money-honesty rules, agent/human split, no-single-ratio rule, retention/truncation distinction | itemized `MIN_SAMPLE_SIZE`/const-by-const enumeration folded into prose — restatement, not a new fact (doc block) |
| `handlers/provisioning.md` | preamble (30 ln) + fixture README (`tack-orch/tests/fixtures/README.md`) | route-separation rationale, 4-step rollback ordering, `PodCreatedLinkFailed` handling, approval-token privilege reasoning | the `POST /pods` 500/rollback/no-delete-route evidence → **vendor → fixture README** (docket's contract, not this module's design); enumerated numbered lists folded into prose (share) |
| `handlers/runner_protocol/artifact_download.md` | preamble (23 ln, unchanged) | operator-surface mount, streaming design, `dead_code` allow rationale | none — fit under budget as originally written |
| `orch_runtime.md` | preamble (8 ln) | runtime-toggle purpose, stop-signal safe-point guarantee, at-most-one-generation invariant | the full "list of planes isn't read once" / supervisor-loop design → **nothing new**: already written in `tack_orch::reconciler.rs`'s own `// Supervisor` comment block (verified: `grep -n Supervisor crates/tack-orch/src/reconciler.rs` shows the section exists); a pointer to it is kept (share) |
| `sprint_dispatch.md` | preamble (28 ln, unchanged) | all five numbered decisions + "what this module does not do" | none — fit under budget as originally written |
| `repo/economics.md` | preamble (23 ln, unchanged) | rework-signal item-level-only correlation gap, retention/truncation distinction | none — fit under budget as originally written |
| `adapters/legacy_bridge.md` | preamble (7 ln) + ADR 0060 (new section) | "Decision: maintain" pointer, one-scheduling-owner invariant | the full three-option evidence (maintain/export/deprecate) → **nothing new added**: ADR 0060 already carries this decision in more depth than the note did, so the preamble now points to it instead of repeating it (design → ADR, already satisfied); provider-scoped-id/`LegacyAttemptProjection` rationale → **design → ADR** (new section appended, see Claim→evidence) |
| `model_policy/wiring.md` | preamble (15 ln) | all four tiers' data locations, project-default typed-error distinction, capability-intersection note | mirrors-`scheduler::wiring` cross-reference folded into one clause (share) |
| `scheduler/mod.md` | preamble (7 ln) | pure-decision-library/no-I/O statement, never-grants-the-lease invariant, two entry points | "why this crate, not tack-db/tack-api" → **nothing new**: `tack-orch`'s own `lib.rs` module doc already states the same dependency rule (verified: `grep -n tack-api crates/tack-orch/src/lib.rs`); "live wiring" paragraph (which tables `wiring::choose_request_for_runner` reads, that the `claim` handler is its only caller) → dropped for the comment-share ratchet (share) — this file's 11 lines of code leave almost no headroom; flagged under *What a stranger still cannot do* |
| `git.md` | preamble (24 ln, unchanged) | worktree-vs-clone rationale, restart-safety sentinel, secret-redaction rule | none — fit under budget as originally written |
| `harness/event_sink.md` | preamble (24 ln, unchanged) | the three independent bounds and why each exists | none — fit under budget as originally written |
| `transport.md` | preamble (30 ln) + `docs/contracts/runner-v1/README.md` (new paragraph) | protocol/client roles, retry discipline, secrets handling | the "wire carries info no fixture fixes" artifact-upload-grant detail → **vendor → fixture/contract README** (it is a property of the `artifact.response.json` fixture, not of this module) |

## Re-baselined?

`no`. `git diff scripts/maintainability-baseline.json` is empty — every file's comment-share or
line-count entry stayed at or under its existing baseline value; nothing needed a lowered
baseline.

## Escalation for the integrator

`scripts/maintainability.py` (owned by IX-M0, not editable by this card) still references
`docs/dev-notes/` twice — the `extract-module-docs` docstring (line 18) and the `DEV_NOTES`
constant (line 41). The mechanism is intentionally still live (a future over-budget preamble
would recreate the directory), so this is not a broken link, but the integrator should decide
whether a follow-up card retires the mechanism now that Part IX's own transition is finished.
`.claude/scope-discipline.md` and `docs/plans/human-maintainability.md` also mention
`docs/dev-notes/` in past/mechanism-descriptive text (not as a live pointer into deleted
content) — left unchanged for the same reason. `docs/book/src/roadmap.md` mentions dev-notes
too, but it is owned by `IX-M7-roadmap` (§IX.2), not this card.

## Context spent

- Tokens read before the first edit (cold start): this run resumed a previous attempt's
  finished edits; the first action was verifying inherited state (`git status`, `git diff
  --cached --stat`, `git log`), then the mandated read list (~6k tokens).
- Context size at handoff: single session, no compaction needed.
- Files opened and not used: none beyond the mandated read list and the files the card owns.
- Read-list lines that were wrong: none — the sizes given for `TODO.md` §IX.0–§IX.1, the card
  block, and `.claude/scope-discipline.md`'s Comments section all matched.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
