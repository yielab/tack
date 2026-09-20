> **Archived 2026-09-20 from `docs/book/src/roadmap.md`, verbatim.** The Phase 64 chapter (closed 2026-09-19), the old "Planned" section (Phase 21 and Phase 22 Task 2, whose open slices are now tasks G1–G3 and C1 in `docs/plans/phase-65.md`) and the Phases 26–32 "Known Gaps" audit table. Relative links are as they were in the roadmap.

# A codebase for human maintainers (Phase 64)

**Status:** closed 2026-09-19. Opened 2026-09-18, when ADR 0068 was accepted with its
stages reordered (the amendment at the bottom of that ADR says why); aligned with ADRs
0066 and 0067 and supersedes ADRs 0060 and 0065 and parts of 0050 and 0064. Every stage
below shipped — `docs/plans/phase-64.md`'s status table records each task. Work is
recorded in ADRs, this page and the commit history — not in a card board.

**The goal is the smallest codebase that does everything a user can do today.** Tack's
features for AI agents are product and stay: the runner, harnesses, the runner-v1 protocol,
MCP and the desktop app. What goes is the scaffolding that existed only because agents built
the tree, the mechanisms nothing calls, and the tests that proved a card was done rather
than that the product works.

## Why this phase exists

Phase 63 set out to make the tree maintainable by a person and measured the wrong thing.
File and function size budgets are met by moving code, so production code grew, tests fell
6 %, and a 10-table control plane nobody enables is still compiled, migrated, tested and
rendered. Two thirds of the test lines verify agent execution; attachments, custom fields
and multiple boards have close to none. CI requires 10 jobs on every pull request and fails
on a coverage measurement, not on lost coverage.

## Stages

| # | Stage | What it removes or changes | Done when |
|---|---|---|---|
| 0 | **Measure** | Nothing. Re-measure ADR 0068's tables with their commands: the legacy surface file by file, which `tests/orchestration` files are runner-v1, every caller of DAG-ordered sprint dispatch. | Every table in ADR 0068 re-measured and dated. |
| 1 | **CI and coverage first** | Five per-crate coverage builds → one `cargo llvm-cov nextest --workspace` run that is also the test run; one workspace floor; ≥ 80 % patch coverage; three tiers (pull request / merge to `main` / nightly); the `main` ruleset's required checks updated to match. The rustls advisory resolved. | Pull request #56 is green and merged into `main`; a pull-request run completes in under 15 minutes. |
| 2 | **Agent scaffolding out** | `.claude/`, `TODO.md`'s boards, dispatch plans, per-card handoffs (frozen into `docs/closed-cycles/`), `scripts/maintainability.py` and its baseline, `list-fixed-waits.py`, `check-test-hygiene.sh` → clippy lints; `CLAUDE.md` ≤ 40 lines. | `docs/closed-cycles/` is the only place history lives and no check reads it; `pre-push` runs fmt and clippy only. |
| 3 | **Harnesses on one core** | **Core landed 2026-09-18:** a harness is a `HarnessDescriptor` plus a four-method `HarnessGrammar` on `harness/local_process.rs`; `codex.rs` 886 → 189 lines, `claude_code.rs` 1 141 → 305, harness unit tests 3 240 → 1 554. **Remaining:** the Chat Completions wire, the docket grammar (against docket's shipped `harness-v1` contract) and the opencode grammar, each with captured fixtures and one zero-spend end-to-end test. Plan: `docs/plans/harnesses.md`. | As in ADRs 0066 and 0067; `tack runner doctor` lists four harnesses on a machine that has them. |
| 4 | **Mechanisms with no caller** | `model_profiles` goes (table, routes, MCP tool, panel). The `decisions` path stays and gets the caller it never had: one seam in the harness core that lets any CLI pause and ask the operator, claude-code first, and a choice at dispatch between `auto` and `ask`. | `git grep` finds `model_profiles` nowhere outside the migration that drops it; a run dispatched with `ask` reaches `waiting_decision` and continues when answered. |
| 5 | **Retire the Docket control plane** | `ControlPlane` trait, reconciler, adapters (docket, github_actions, prometheus, registry, legacy_bridge), orch routes and tokens, `tack orch`, the Fleet/Approvals/Economics/Provision screens, their tests; one migration per dropped table, the rows kept in the whole-database snapshot the migration runner takes before a table drop. **Not before Stage 3's docket harness has landed.** | No `orch_*` table on a fresh install; `TACK_ORCH_*` gone from `docs/CONFIG.md`; upgrade from a populated database tested once. |
| 6 | **Test suite rebuilt by layer** | Each invariant kept at its lowest layer plus at most one HTTP test; wave gates and narrative multi-claim tests deleted after their real claims move; behaviour tests added for the board features with no coverage; E2E reduced to critical journeys (Chromium on merge, cross-browser nightly); weekly report-only `cargo-mutants` on `tack-core` and `tack-db`. | Workspace line coverage at or above its Stage 1 floor with fewer tests; every public API route has a success, auth and error test; flaky-test quarantine documented in `docs/TESTING.md`. |

Stages 1, 2 and 3 touch no board behaviour and can run in parallel. Stage 4 and Stage 5
each change the product and each ship with release notes naming what was removed; what a
user of the bridge loses until a harness upgrade replaces it — the approvals inbox,
one-click pod provisioning, per-product cost from docket's events, DAG-ordered sprint
dispatch — is named there too. The release tag and `docs/LAUNCH-CHECKLIST.md` resume after
Stage 6.

## What this phase deliberately does not do

- **Remove agent-facing product.** Runner, harnesses, runner-v1, MCP and
  the desktop app stay.
- **Rewrite history.** Closed boards, handoffs and ADRs stay, frozen.
- **Chase a number.** No stage is done because a ratio or a percentage moved; each is done
  when its surface is gone or its behaviour is verified.
- **Add tooling for its own sake.** Standard tools only: nextest, llvm-cov, clippy, rustfmt,
  cargo-deny, Playwright, Vitest, cargo-mutants.

## Exit

A person clones the repository, reads `README.md`, `CONTRIBUTING.md` and
`docs/TESTING.md`, and has everything needed to change any part of Tack. There is one
orchestration model, no table or route without a caller, a test suite that verifies each
behaviour once at the right layer, and a pull-request CI that finishes in under 15 minutes
with honest coverage of the code the change touched.

---

## Planned

Two items are open. Each carries file paths and acceptance criteria so it can be picked up
cold.

- **Phase 21 — GitHub sync, the inbound half.** Pushing an item's state to its issue
  shipped. Receiving changes from GitHub, mirroring comments, per-project tokens and
  linking an existing issue by hand are not built. One product decision scopes the work:
  mirror status, comments and close-state, or status only.
- **Phase 22, Task 2 — `tack open` / smart start.** `tack branch <item-id>` shipped; the
  command that opens an item's branch and moves it to in-progress in one step did not.

What the harnesses can still gain — a run that asks before it acts on codex, docket and
opencode, cancel and artifacts on docket, one shared install for opencode — is listed
under "Upgrades" in `docs/plans/harnesses.md`.

### Phase 21 — Bi-Directional GitHub Sync ⏳ _v1 push-only shipped; inbound + comments pending_

**Goal:** Upgrade the existing one-way GitHub _import_ into two-way _sync_ so item
status/comments/close-state mirror back to GitHub — closing the biggest "real work" gap
vs Huly. Scope per decision #2.

> **v1 shipped (push-only, status):** imported items are linked in a `github_links`
> table; completing a linked item closes its GitHub issue (reopening on the way back
> out) when `TACK_GITHUB_TOKEN` is set. Best-effort/fire-and-forget. Pieces:
> migration 018, `repo/github_links.rs`, `github_sync.rs` (`push_issue_state` +
> `state_change`), the `maybe_sync_github` hook in `handlers/items.rs`, and import
> linking. `TACK_GITHUB_API_BASE` makes import+push testable/Enterprise-ready. Tests:
> wiremock push (3), link round-trip, and a full import→complete→close integration
> test — all against a **mocked** GitHub (no real repo touched). Decision recorded in
> [docs/GITHUB-SYNC.md](../../GITHUB-SYNC.md).
>
> **Still pending (future slices):** Task 1 comments-mirroring, Task 4 inbound sync
> (webhook/poll), per-project tokens + manual item linking, and a real-repo live test.
> The task notes below describe the _full_ bi-directional vision.
>
> **Scheduled:** the remaining slices are **Phases 47 and 49** of the Agnostic
> Control Plane cycle. Phase 47 builds the inbound webhook receiver (signature
> verification, delivery dedupe, echo suppression) that Task 4 describes; Phase 49 adds
> comment/label mirroring, per-project credentials, PR and check-run state, and merge
> evidence. Task 2's "backfill links for previously imported items" is covered by
> migration 049's reverse index.

#### Task 1 — Spec the sync model

Write `docs/GITHUB-SYNC.md`: link storage (item ↔ GitHub issue number + repo), conflict
policy (last-write-wins vs GitHub-authoritative), v1 scope (recommend: status +
close-state + comments outbound; inbound via webhook or poll), and push trigger (reuse
the webhook dispatch path in [webhook.rs](../../../crates/tack-api/src/webhook.rs) vs a
periodic reconciler). **Blocks the rest of Phase 21.**

#### Task 2 — Persist the issue↔item link

Add migration #18 in
[tack-db/src/migrations.rs](../../../crates/tack-db/src/migrations.rs) — an
`external_links` table (or `gh_repo` / `gh_issue_number` / `gh_synced_at` columns on
items) — plus a repo module under `crates/tack-db/src/repo/`, following the "Adding a
New Entity" pattern. Backfill links for previously imported items.

#### Task 3 — Outbound push

On a linked item's update, PATCH the GitHub issue (state, optionally labels) and POST
mirrored comments via the GitHub REST API using the stored PAT. Hook into the item
update path in `handlers/items.rs`; new logic in `handlers/import_github.rs` (or a new
`github_sync.rs`). Rate-limit aware; best-effort with logged failures.

#### Task 4 — Inbound sync

Implement either `POST /api/projects/{id}/github/webhook` (verify
`X-Hub-Signature-256`) or a poll-on-interval reconciler, per Task 1. Apply remote
changes through `tack-core` so workflow rules hold. Register the route in
[router.rs](../../../crates/tack-api/src/router.rs).

#### Acceptance criteria

- Moving a linked item to a Done status closes the GitHub issue; adding a comment mirrors
  it (test with a mocked GitHub endpoint).
- Closing an issue on GitHub moves the linked Tack item to Done (test with a captured
  webhook payload fixture).
- Migration runs clean on startup; `cargo test --workspace` and `cargo clippy` pass.

### Phase 22 — Dev-Native CLI (git-from-card)

**Goal:** Make the CLI the surface Rust devs love — cheap, high-signal, and a praised
feature in the Rust kanban field (mirrors fulsomenko/kanban).

#### Task 1 — `tack branch <item-id>` ✅ _done_

Fetch the item via `client.rs`; slugify `type/id-short-title` (e.g.
`feat/a1b2-add-table-view`). Print `git checkout -b <branch>` by default; run it with
`--checkout`; `--json` outputs the name. New `crates/tack-cli/src/git.rs` +
subcommand in `main.rs`. Respect a configurable prefix template. Unit-test the slug
function.

> Shipped: [git.rs](../../../crates/tack-cli/src/git.rs) (`slugify`, `type_prefix`,
> `branch_name`, 8 unit tests) + the `Branch` subcommand and `cmd_branch` in
> `main.rs`. `--prefix` overrides the type-derived prefix (feature→feat, bug→fix,
> …). Documented in the [CLI reference](user-guide/cli.md).

#### Task 2 — `tack open` / smart start

`tack open <id>` opens the item's web URL in `$BROWSER`. Optional `tack start <id>` =
move to the first in-progress status + `tack branch --checkout` in one step. Cover with
CLI help + a smoke test.

#### Acceptance criteria

- `tack branch <id>` prints a sane branch name; `--checkout` creates+switches; `--json`
  works. `cargo test` green.

---

## Known Gaps

> **This table is the Phases 26–32 audit snapshot, not a current gap list.** Each row records
> what that audit found; the "Tracked in" column names the phase that took the work on, and
> those phases are marked done above. Rows verified closed as of 2026-08-14 are annotated
> inline. Read the owning phase's own section for what actually shipped versus what it
> deferred — several closed only partially. Current Part III gaps live in the Part III section
> below and on `TODO.md`'s board, which is the authority.

| Area | Gap | Tracked in |
|---|---|---|
| Item updates | `sprint_id` / `due_date` / `estimate_unit` not persisted by `update_item`; `started_at`/`completed_at` never set on ordinary status moves | Phase 26 (blocker) |
| Security | Alexa endpoint lacks Amazon cert-chain validation; no warning on unauthenticated non-loopback bind; tar-slip + unverified sha256 in backup restore; S3 secret embedded in backup bundles | Phase 27 |
| Backup as sync | No conflict detection between installs; scheduler ignores UI-saved settings; non-atomic restore swap | Phase 28 |
| API contract | ~~No OpenAPI spec; hand-maintained TS types and API docs have drifted from the router~~ **closed** — `docs/openapi.json` (90 paths) is generated from the code, `frontend/src/shared/api/schema.gen.ts` is generated from it, and CI fails on drift. "Two error JSON shapes" **still holds and is now deliberate**: `ErrorEnvelope` for operator routes, `RunnerV1ErrorEnvelope` for the runner protocol — two separate auth surfaces, not an oversight | Phase 29 |
| Vocabulary | Global "+ New" modal, Sprints view, tabs/palette/first-run guide hardcode "sprint"/"Story Points" | Phase 30 |
| Coverage reporting | ~~168 Vitest unit tests and a Playwright E2E suite ship; automated coverage thresholds in CI are not yet enforced~~ **closed** — CI's `coverage` job enforces `cargo llvm-cov --fail-under-lines` per crate (tack-core 85, tack-db/tack-api/tack-orch 70) alongside Vitest thresholds; the frontend suite is now 724 tests across 85 files | Phase 32 |
| Release integrity | No checksums/SBOM/provenance on release assets; SECURITY.md routes disclosure through public issues | Phase 32 |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported | Future |
| Auth | No multi-user auth (by design for v1) | Future |

