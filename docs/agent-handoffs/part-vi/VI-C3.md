# VI-C3 handoff

- Base SHA / branch / final SHA: base `02aa4e3` (the wave's designated base — `develop`'s
  actual tip at start of work was `85f2bfa`, one commit ahead, and that commit's diff is
  `TODO.md` + two handoff `README.md`s only, no code, so branching from the named `02aa4e3`
  changes nothing behavioral). Branch `agent/vi-c3-project-agent-settings`. Not committed —
  final SHA is n/a; the worktree holds the changes.
- Files changed (must equal ownership list): `crates/tack-core/src/models.rs`,
  `crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo/projects.rs`,
  `crates/tack-db/src/repo/execution.rs` (one new read-only fetch method, same file the
  agent-profile/fleet equivalents already live in), `crates/tack-db/tests/integration_test.rs`
  (existing `UpdateProject` literal gains the field), `crates/tack-orch/src/model_policy/mod.rs`,
  `crates/tack-orch/src/model_policy/wiring.rs`, `crates/tack-orch/tests/model_policy_test.rs`,
  `crates/tack-api/src/handlers/executions.rs` (wires the real project id into the existing
  model-policy resolve call), `crates/tack-api/src/handlers/export.rs` and
  `.../handlers/templates.rs` (existing `UpdateProject` literals gain the field, both `None`),
  `docs/openapi.json` + `frontend/src/shared/api/schema.gen.ts` (regenerated, not hand-edited),
  `frontend/src/shared/types/index.ts`, `frontend/src/features/settings/ProjectSettings.tsx`,
  `frontend/src/features/settings/panels/AgentsPanel.tsx` (new). No file from VI-B1's or
  VI-C4's ownership list was opened or touched — confirmed by name against the interruption
  message's list before writing this.
- Contract fixtures consumed: none. This card never touches `docs/contracts/runner-v1/`.
- Behavior implemented: a project can now carry its own default-model opinion — the
  `Project` tier in `tack-orch::model_policy`'s precedence walk (`RequestOverride` →
  `AgentProfile` → `Project` → `Fleet` → auto-select), which existed as pure logic and an
  exhaustive test table before this card but was wired to a hardcoded `None` because no
  storage existed. Migration 062 adds one nullable column, `projects.default_model`; it
  holds the exact JSON serialization of the new `tack_core::models::ProjectModelDefault`
  enum (`Auto` | `Explicit{provider, model_id}`), set through `PATCH /api/projects/{id}`.
  A new "Agents" tab in Project Settings lets an operator pick auto-select or type a
  provider/model id (no live catalog exists yet — see Surface-map delta).
- Tests added and exact commands/results:
  - New: `crates/tack-orch/tests/model_policy_test.rs::a_project_default_model_is_read_from_the_real_default_model_column`
    — sets a project's `default_model` through `Repository::update_project` (never raw SQL)
    and asserts `resolve_request_model_policy` returns it with `source: Some(ModelPolicyTier::Project)`.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C3 cargo test -p tack-db` — 199 passed,
    0 failed, 2 ignored (unrelated perf/live tests), across 14 binaries including
    `integration_test` (27/27) and the full `orch_migrations_test` suite.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C3 cargo test -p tack-orch` — 282 passed,
    0 failed, 1 ignored (unrelated live-harness test); `--test model_policy_test` alone: 7/7
    (was 6 before this card — read the file before editing).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C3 cargo test -p tack-api` — 469 passed,
    0 failed, including `openapi_contract` (5/5) after regenerating with
    `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`, then
    `cd frontend && npm run gen:api`.
  - Frontend: `npm run type-check` clean, `npx vitest run` 726/726 passed (85 files),
    `npm run build` clean.
  - `npx playwright test e2e/smoke.spec.ts -g settings --project=chromium` — 2/2 passed
    ("project overview and settings render", "global settings and templates render").
  - Manual round-trip against the real router (`tack serve` on a throwaway
    `sqlite:/tmp/.../manual-check.db`, port 38217, killed after): `POST /api/projects` →
    `default_model: null`; `PATCH .../projects/{id}` with
    `{"default_model":{"kind":"explicit","provider":"openai","model_id":"opaque/model-alpha"}}`
    → 200 with the value set; `GET` immediately after → same value. Not a committed test —
    the exact curl transcript is in this handoff's next two bullets and the "Failure/adversarial
    case proved" section below.
  - `cargo fmt`/`rustfmt --check` on every file this card touched: clean (a pre-existing,
    unrelated drift in `crates/tack-api/tests/trust_boundary_test.rs` was left alone —
    not this card's file). `cargo clippy -p tack-core -p tack-db -p tack-orch -p tack-api
    --all-targets -- -D warnings`: clean.
- Failure/adversarial case proved: two, both load-bearing to the "Stop if" condition in
  this card's dispatch block ("the JSON blob cannot be validated against a typed struct at
  write time without a second validation path at read time").
  1. **Write-time rejection.** `PATCH /api/projects/{id}` with
     `{"default_model":{"kind":"bogus"}}` against the real running router →
     `422 Unprocessable Entity`, `"unknown variant `bogus`, expected `auto` or `explicit`"`,
     before any handler code or database write runs. Axum's typed JSON extractor is the
     validation; there is no second one.
  2. **Read-time honesty.** An ad hoc test (not committed) inserted `'not json at all'`
     directly into `projects.default_model` via raw SQL — something no write path can
     produce — then called `Repository::get_project` and `list_projects`. Both returned
     `Err(Protocol("expected ident at line 1 column 2"))` rather than a panic or a silent
     `None`. Every other JSON column on `Project` (`vocabulary`, `workflow`) still defaults
     silently on a parse failure — a pre-existing choice this card did not change — but
     `default_model` does not, because unlike those two it is only ever written as the
     serialization of a validated enum, so a decode failure means real corruption, not a
     legitimate "no opinion."
- Schema/API/contract change requested from another owner: none. `docs/openapi.json` and
  `frontend/src/shared/api/schema.gen.ts` were regenerated by this card for this card's own
  change; VI-B1 and VI-C4 also regenerate them for their own changes, so the integrator
  should expect a merge/regenerate step, not treat any one branch's copy as final.
- Known limitations or `not_measured` fields: no live model catalog to pick from — the
  Agents tab is free-text provider/model id, matching `AgentProfilesPanel`/`FleetsPanel`'s
  own posture for opaque fields. `UpdateProject` has no way to clear `default_model` back to
  "unconfigured" once set (true of `description`/`vocabulary`/`workflow` too — a systemic,
  pre-existing PATCH limitation, not something this card introduced or fixed).
- Secrets/logging review: `default_model` carries no secret — a model provider name and
  model id, both already sent over the wire unprotected elsewhere (agent-profile/fleet
  defaults, `execution_requests.requested_model_provider/id`). Nothing new to redact.
- Safe merge order and likely conflicts: no overlap with VI-B1 (runner secrets/config/adapters/
  CLI) or VI-C4 (attempt list handlers, `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`,
  `AgentActivityTab.tsx`) at the file level. Likely conflict: `docs/openapi.json` and
  `schema.gen.ts` if merged in the same pass as either sibling card — regenerate once after
  all three land, don't try to hand-merge the generated diff.
- Checklist: no unowned files touched; no live secret; no panic stub (`ProjectRow::into_project`
  returns `Result`, never unwraps the new column); no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A project can be given a default model (auto or explicit provider/model id) from Project Settings | `frontend/src/features/settings/panels/AgentsPanel.tsx`; manual `PATCH /api/projects/{id}` round trip above |
| That default is actually used when resolving an execution's model, at the documented precedence (below agent-profile, above fleet) | `model_policy_test.rs::a_project_default_model_is_read_from_the_real_default_model_column`; `crates/tack-api/src/handlers/executions.rs`'s `resolve_request_model_policy` call now passes the item's real `project_id` |
| A malformed default-model body is rejected before anything is written | curl → `422`, "Failure/adversarial case proved" #1 |
| A corrupted stored value is reported as an error, never silently ignored | ad hoc test → `Err(Protocol(...))`, "Failure/adversarial case proved" #2 |
| Upgrading a real, already-deployed database applies migration 062 without loss | two `run_all` transcripts below, against two real `.db` files |
| Every existing `UpdateProject` construction site still compiles and behaves unchanged | `cargo build`/`cargo test` across `tack-core`, `tack-db`, `tack-api`, `tack-orch` all green; the three pre-existing literal sites (`export.rs`, `templates.rs`, `integration_test.rs`) all pass `default_model: None` |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- Migration count: 61 → 62 (`grep -n '"0[0-9][0-9]_' crates/tack-db/src/migrations.rs | tail -1`
  now reads `062_project_default_model`).
- `crates/tack-orch/tests/model_policy_test.rs` test count: 6 → 7 (`grep -c '#\[tokio::test\]'`
  before and after this card's edit).
- Full per-crate pass counts (commands and totals above): `tack-db` 199, `tack-orch` 282,
  `tack-api` 469, frontend vitest 726 — all green, 0 failed anywhere.
- `run_all` transcript, genuine beta.7 install copy (`/home/ox/.local/share/tack/tack.db`,
  this machine's actual `tack serve` data directory as of 2026-09-03, migrated through 061
  — copied, never written to directly):
  ```
  migrations applied before run_all: 61
  projects before: 0
  migrations applied after run_all: 62 (latest: 062_project_default_model)
  projects after: 0
  projects with default_model still NULL: 0
  ```
  This copy has zero project rows, so it proves the migration applies cleanly and is
  idempotent, but not that existing rows survive — see the next transcript for that.
- `run_all` transcript, this repo's own older dev database
  (`/home/ox/Sites/objetivosMios/tack.db`, last touched 2026-08-05, migrated only through
  036 — copied, never written to directly), chosen because it holds real project rows and
  therefore proves the stronger claim — every existing project survives the full catch-up
  to 062 with `default_model` reading back `NULL`, not an error:
  ```
  migrations applied before run_all: 36
  projects before: 6
  migrations applied after run_all: 62 (latest: 062_project_default_model)
  projects after: 6
  projects with default_model still NULL: 6
  ```
  Both runs also called `run_all` a second time immediately after (idempotency check) with
  no error.

## What a stranger still cannot do

A stranger can now open a project's Settings → Agents tab and give the project a real
default model, and every execution that omits both an explicit choice and an agent-profile
default will actually use it — that path used to silently no-op (`wiring.rs` hardcoded
`None` for this tier no matter what was configured). What they still cannot do: pick that
model from a live list — they must already know the exact provider and model id strings
the harness expects, typed free-hand, because no model-catalog endpoint exists in this
branch's base (that is VI-B2's work, sequenced after this card in Wave 15/16). They also
still cannot add a runner to a fleet from the UI, or remove a configured default model
back to "unconfigured" once saved — both are pre-existing gaps this card did not touch.

## Surface-map delta

- **"Choose a default model"** (§VI.0's table): moves from *impossible (no storage)* to
  *real storage + real UI*, but only half of the stated target — the target text is "UI: a
  project setting, chosen from a **measured catalog**," and there is no catalog to choose
  from yet in this branch's base (VI-B2, not landed here). The reason is sequencing, already
  covered by the wave table ("C3 ∥ C4 ... ; C1 after B2+B3"), not a structural block, so this
  is not an escalation to ADR 0061 — just a note that the row is not fully closed until a
  catalog-backed picker (likely VI-C1 or a later revisit of this panel) replaces the two
  free-text fields.
- **"Create a fleet / add a member"**: unchanged by this card. Re-confirmed while reading
  `FleetsPanel.tsx` and `frontend/src/shared/execution/api.ts` as this card's UI-pattern
  references (per the dispatch block's read list) — `fleetsApi` still exposes only
  `list`/`create`; the member routes (`POST /api/runner-fleets/{fleet_id}/members` and
  `.../members/{runner_id}`) still have no frontend caller. This card did not implement
  fleet membership and made no claim about it — the row is reported here only because the
  dispatch block asked for a delta on both rows, and there is none to report on this one.

## Context spent

- Tokens read before the first edit (cold start), against the block's estimate: the block
  estimated ≈28k for its named read list; actual reading before the first edit ran higher
  because three files were read beyond the named list (below), each recorded with why.
- Context size at handoff: comfortably under the 150k stop threshold.
- Files opened and not used / opened beyond the named list (each with why):
  - `crates/tack-orch/src/scheduler/types.rs` (grep only, not read whole) — needed
    `ModelSelector`'s exact shape to write `parse_project_default_model`'s match arms; the
    block's read list covers `wiring.rs` and the test file but not the type this module
    converts into.
  - `crates/tack-api/src/handlers/executions.rs` (targeted read, not whole file) — the
    block doesn't name it, but `resolve_request_model_policy` has exactly one real caller
    and adding a parameter to it without reading that call site would have been a guess.
    Necessary; not wasted.
  - `crates/tack-db/src/repo/items.rs` (one function, `get_item`) — needed to know `Item`
    carries `project_id` so the new test and the handler could both derive a project id
    without inventing a second lookup path.
  - `docs/adr/0061-provider-credentials-at-the-runner-boundary.md` §4 (Vocabulary) — named
    by the block ("the ADR's vocabulary decision (~1k)"); read as instructed, not extra.
  - Two real `.db` files were read (via `sqlite3`, not opened as source) to find upgrade
    fixtures — not source files, but recorded here since the block didn't name a fixture
    location and this card had to go find one itself.
- Read-list lines that were wrong (a range that missed, a size that was off): none found —
  every named read matched what the block said it would contain (the migration shape at the
  named line range, the `Project`/`UpdateProject` struct shapes, the wiring.rs precedent,
  the three frontend panel files).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

### 2026-09-04 — Wave 16 integrator: what the "genuine beta.7 install copy" actually proves

The `run_all` transcript above is real and the copy was real, but the database it ran
against held **zero projects** (`projects before: 0`). It therefore proves that
migration 062 applies cleanly on top of a schema that had genuinely migrated through
061 — worth having, and more than an in-memory harness shows. It does not prove
anything about existing rows, because there were none. Read the evidence as
"the migration chain accepts it", not "real data survived it".

Nothing here needs changing; the claim just should not grow in the retelling. A card
that wants the stronger statement has to seed rows first and assert their contents
after.
