# ADR 0069: the brief is a first-class entity on the item, typed, and it travels

**Status: ACCEPTED 2026-10-03.** Written for `docs/plans/phase-66.md` (C1–C3).
The user accepts, amends or rejects it in that plan's Wave 0.

**Decide:** approve that an item may carry a **brief** — typed acceptance criteria,
constraints, a definition of done and a risk hint — stored as its own entity (`item_briefs`,
one row per item), published as a versioned contract (`docs/contracts/brief-v1/`), rendered
into the text every harness receives and copied, structured, into the evidence an independent
verifier reads. Approve that this is **not** built on custom fields.

**Why now:** an attempt today ends with a log and no way to say whether it did what was asked,
because nothing recorded what was asked in a checkable form. The item has title, description,
type, priority, estimate, tags and custom fields (`crates/tack-core/src/models.rs:103-135`) and
no criterion. Worse, the item's own text does not reach the harness at all: the prompt is the
agent profile's `instructions` verbatim (`crates/tack-runner/src/harness/local_process.rs:527`,
`frontend/src/shared/runWithAgent/shared.ts:131`). The business plan makes the brief the first
contract of the factory (`plan-fabrica-agentica-l3-l5-2026-09-29.es.md` §3.3, "BriefingScript
v1") and the merge-readiness pack (ADR 0071) is a map *criterion by criterion* — without typed
criteria there is nothing to map.

**If you do nothing:** every attempt keeps being judged by reading a log; the verifier, when it
exists, has no criteria to check; the only escalation an agent can make is prose; and the
verification tax stays entirely human.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | A brief is an entity, `item_briefs(item_id PK → items ON DELETE CASCADE, acceptance, constraints, definition_of_done, risk, timestamps)`, with `GET`/`PUT`/`DELETE /api/items/{id}/brief`, exported and imported with the project. | One row per item, deleted with it, exported with it. It is the item's contract, not a property. |
| 2 | Criteria are a tagged enum: `command {run, expect_exit}`, `test {name, runner}`, `metric {name, op, threshold, unit}`, `file {path}`, `absent {path}`, `manual {text}`. Each has an `id` unique in the brief. | Five of six are machine-checkable. `manual` is allowed and marked as the kind that costs a human; the L4 threshold in the business plan is "≥ 80 % not manual", and that can only be measured if `manual` is a kind. |
| 3 | Constraints are a tagged enum too: `forbidden_path`, `allowed_dependency`, `max_changed_files`, `note`. Definition of done is text. Risk is `low|medium|high` or absent. | A verifier can check the first three mechanically against the evidence's file list. |
| 4 | The contract is `docs/contracts/brief-v1/schema.json` with an example pinned by a test in `tack-core`. The Rust types are the shared type; the frontend reads the regenerated OpenAPI schema. | "A string between two modules without a shared type is two assumptions" (docket, Wave 30). The brief crosses three boundaries: UI → API, API → harness, runner → verifier. |
| 5 | The brief travels twice. **Rendered**: `create_execution` appends the item's title, description and `render_markdown(brief)` to the profile's instructions, server-side, for the UI, CLI and MCP alike. **Structured**: the request snapshot carries `brief`, and the runner writes `brief.json` into the evidence directory (ADR 0071). | The harness reads prose; the verifier reads JSON; both must see the same brief. Rendering server-side is the rule in `docs/book/src/developer/adding-features.md` ("don't scatter validation across handlers") applied to prompt assembly. |
| 6 | Not custom fields. | A custom field is per-project and scalar — text, number, date, boolean, select, multi-select, url, email, long text (`models.rs:683-695`); it cannot hold a typed array, cannot be validated in `tack-core`, and would need a convention on field names to be found by the harness and the verifier — a second contract nobody pins. |

## Evidence

| Fact | Locator (read at `7718420`) |
|---|---|
| No acceptance criteria, risk or definition of done on the item | `crates/tack-core/src/models.rs:103-135` |
| Custom field types: text, number, date, boolean, select, multi-select, url, email, long text — all scalar, per project | `models.rs:666-745` |
| The prompt is the profile's `instructions`, verbatim; the item's title and description are not in it | `crates/tack-runner/src/harness/local_process.rs:527`; `frontend/src/shared/runWithAgent/shared.ts:131`; `RunWithAgentModal.tsx:399` |
| `ItemSource::is_trusted` exists for the prompt-injection boundary and has no caller in a handler | `models.rs:121-143`; `rg 'is_trusted\(\)' crates/tack-api` → none |
| The request snapshot accepts additive fields | `crates/tack-orch/src/execution/types.rs:341-361` (`additional`, flatten); `docs/contracts/runner-v1/README.md` ("additive object fields round-trip unchanged") |
| The entity path: model in core, migration, repository, `repo.rs`, handler, router | `docs/book/src/developer/adding-features.md`, "Adding a New Entity" |
| Next migration number | `078` (`grep -o '"[0-9]\{3\}_[a-zA-Z0-9_]*"' crates/tack-db/src/migrations.rs \| sort -u \| tail -1` → `077`) |

## What this reverses or amends

Nothing in the Phase 65 "Decided" table. It changes one behaviour that was never decided:
the harness now receives the item's title and description (decision 5). That is stated in a
test name in C3 so nobody mistakes it for an accident.

## Cut and deferred

| Item | Status | Why |
|---|---|---|
| A brief template per project type (education, software…) | Deferred | Trigger: a second project type asks for defaults. |
| A brief generated from the description by a model | Deferred | Trigger: the intake ODD (business plan Phase 4). It would be untrusted text either way. |
| Enforcing `is_trusted` at dispatch | Parked in the plan | A finding, not this ADR's question. |

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-10-03 — accepted by the maintainer as written.** Tasks C1a, C1b, C2 and C3 of `docs/plans/phase-66.md`.
