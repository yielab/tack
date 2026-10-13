# Plan: Phase 72 — merge authority follows the autonomy level you set

**Status: proposed 2026-10-13, waiting on the maintainer's go.** Serves ADR 0079
(`docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md`, PROPOSED). Written
against `develop` at `11de5f2e`. Re-run `rg -n` before trusting a line number. Nothing is
dispatched until the ADR is accepted with the cuts below taken or refused.

What this phase builds, in one paragraph. A project can let Tack merge a change by itself (L4)
inside the parts of the code the person authorised, or everywhere (L5). Tack merges only when the
verifier said `merge`, the pack is signed for the exact head commit, the stages the person marked
required passed, CI is green on that commit and no budget is spent; otherwise the item waits in
Needs your review with the first failing condition named. A rejected change goes back to the agent
a bounded number of times before a human sees it. A revert takes the level back down by itself.
Briefs keep their history, PQC stops moving under the reader, and the Factory page shows dollars
when the person enters prices. A project that stays at L3 behaves exactly as today.

## Ask for the owner (before acceptance)

### 1. Cuts proposed to the ADR — each is your call

| # | Cut | Why |
|---|---|---|
| X1 | **Defer decision 6 (directives)** to its own phase. | Its input is review reasons, and Tack has no users yet: there is nothing to group. It adds a proposal table, a state machine and a write into the repository for an empty list. The reasons are already stored (`mrp_reviews.reason`), so nothing is lost by waiting. Q7 goes with it. |
| X2 | **Defer decision 4(a) and 4(b)** (forwarding outcomes to prill's store, reading `prill metrics`). Keep 4(c) rate card and 4(d) PQC. | Every figure `metrics/v1` gives (counts, escalation, agreement, escaped defects) is computable from tables Tack already has: the pack's decision, `mrp_reviews.verdict`, `pull_requests`. Two sources for one number means a "whose figure is this" branch on the page, and the store lives on the runner's machine while the outcomes live on the board's, so forwarding them is a cross-machine write for a duplicate. |
| X3 | **Decision 5 keeps four fields:** `tokens_per_day`, `usd_per_day`, `auto_merges_per_day`, `queue_items_max`. Drop `review_minutes_per_day` and the per-domain merge cap. All budgets are enforced at the attempt boundary (the scheduler) and in the merge check; no new per-run harness flag. | Nothing measures review minutes today; time from pack to verdict is waiting, not effort. One per-project merge cap already stops "a bad hour merging fifty". Per-run limits already reach docket and Claude Code through the profile's `budgets`. |
| X4 | **L5 has no limits of its own.** L5 is L4 applied to every domain. | Decision 1 already demotes on a single revert or escaped defect at any level, so a rate limit under it never fires first. |
| X5 | **No `brief_version` in the pack.** The request snapshot records the version; the pack keeps `brief_sha256`, and the version rows carry the same hash. | Avoids a contract change to `mrp-v1` that the hash already answers. |
| X6 | **Stopping the loop is cancelling the follow-up run** (the existing cancel). A cancelled follow-up starts no other. No new button. | Decision 2's "a human can stop it from the item" is met by what exists. |

### 2. Open questions — the default this plan builds with

None of these blocks a task: each is a value the code reads, not a shape it depends on.

| Q | Default built | Changes later by |
|---|---|---|
| 1 Signing key | A file path in the runner's `[verify] sign_key`; the board stores the project's public key (PEM). | Config only. |
| 2 Defect window | 14 days per project (`pqc_window_days`); a defect is a revert or a failing CI on the merge commit. | A column value. |
| 3 Second person for L4 | No. `autonomy_merges.policy_snapshot` records the policy, not a person (ADR 0059 stands). | Identity work, separate ADR. |
| 4 Authoritative CI | **Both** GitHub signals on the head commit: every check run completed as `success`, `neutral` or `skipped`, and the combined status not `failure`/`pending`. No check and no status at all is **not** green. | One function in `github_sync.rs`. |
| 5 Merge method | GitHub's default (`merge`), branch kept. A repository that forbids it answers 405 and the item is held with that reason. | One parameter. |
| 6 Rate card | Entered by hand, dated; every dollar figure shows the date. | — |
| 8 Loop cap | Per item (`verify_loop_max_attempts`), plus the project's daily token and dollar budgets. | — |
| 9 Required stages at L4 | A project list `autonomy_required_stages`, default `["criteria","suite"]`; each must be `passed`. `not_applicable`, `not_run` and `degraded` hold the merge. | A column value. |
| 10 L5 queue | Kept; filled only on failure. | — |

### 3. What reading the code found that the ADR does not say

These change the plan; none changes a decision.

1. **The pack drops prill's stage statuses.** `prill.rs` reads `stages` only to sum tokens, and
   `MergeReadinessPack` has no per-stage status, so decision 1's "none was `not_applicable`"
   has nothing to read. Task K adds an optional `stages: [{name, status}]` to Tack's `mrp-v1`
   (prill's nine stage names and five statuses, `contracts/mrp-v1/schema.json` `$defs/Stage`).
2. **GitHub's combined status does not include GitHub Actions**, which report as check runs. A
   reader of the combined status alone sees "pending" with zero statuses on most repositories.
   Hence Q4's default reads both.
3. **A pull request opens only for an item linked to a GitHub issue**
   (`handlers/pull_requests.rs:7-8`, `get_github_link`). L4 therefore applies only to linked
   items; an unlinked item is held with "the item has no GitHub link". Widening PR opening to
   `projects.repository` is not in this phase.
4. **The GitHub poll runs only with the board's environment token and a poll interval**
   (`lib.rs:33`). The merge check runs in that same loop, so L4 with the poll off holds every
   item with "GitHub polling is off on this board". No second background task.
5. **`prill attest verify` runs on the board's host**, which may not have prill (or its `sign`
   extra). Then the merge is held with that reason (P3); nothing is faked.
6. **Stale in the ADR:** the next migration is `087` (phase 71 took `086_one_model_string`);
   `prill.rs:690` is now `:684`; there is no `settings` table — the rate card goes in `app_meta`
   (key `rate_card`), where board preferences already live.
7. **Two new crates.** Decision 7's contract test needs a JSON Schema validator and decision 1's
   domains need glob matching; the workspace has neither. B adds `jsonschema` (dev-dependency of
   `tack-runner`), A1 adds `globset` to `tack-core`, both by the floor rule.

## Decided — not re-opened by any task

Every row of ADR 0079's table as accepted, the cuts the owner takes from §1, the defaults of §2,
plus:

| Question | Decision |
|---|---|
| Schema | One migration, `087_autonomy`, lands first (task S) with every table and column of the phase, ahead of its readers — the 078–081 precedent. A reader needing another shape adds a new migration; it never edits 087. |
| Where the policy lives | A pure function in `tack-core::autonomy` (task A1): facts in, `Ok(level)` or the first failing condition out. Nothing in it reads a database or the network. |
| Where it runs | In the GitHub poll loop (`lib.rs::run_github_poll_task`), after `poll_once`, as `autonomy::tick`. The completion handler only records; it never merges. |
| Holds | A row in `autonomy_merges` with `merged_at` NULL and `held_reason` set, updated each tick. One table for merged and held, not two. |
| The merge | `PUT /repos/{repo}/pulls/{n}/merge` with `sha` = the attempt's `actual_execution.git.head_commit`. A 405/409 holds the item with GitHub's message. After a merge the item follows `done_on_mrp_accepted` as if accepted. |
| Order inside a tick | pack says merge → domain level ≥ L4 → required stages passed → attestation verifies for the head commit → budgets → open the PR (reversible) → CI on head green → merge. The first that fails is the held reason. |
| Demotion | On a revert (`github_sync.rs` revert detection) or an escaped defect of an automatically merged change: its domain (or the project, when no domain matched) drops one level, the attempt's row is flagged, an activity event is written. |
| MCP | One read-only tool, `get_autonomy`. Nothing in MCP changes a level. |
| Generated files | Each task regenerates and stages `docs/openapi.json` and `schema.gen.ts`; on a merge conflict the integrator regenerates, never hand-merges. |

## Rounds

At most two agents building cargo at once; a frontend-only task runs beside them.

| Round | Tasks | Needs |
|---|---|---|
| 0 | fixtures + mechanical fixes (integrator); S, A1 (Haiku) | ADR accepted |
| 1 | B, G | S; A1 (G) |
| 2 | K, U | S, A1 |
| 3 | L, P | U (L); S (P) |
| 4 | A2; P-ui beside it | G, K, U, A1 (A2); P (P-ui) |
| 5 | F | A2 |
| 6 | Z (integrator) | everything |

Ceilings (tool calls): Haiku 60; Sonnet one layer 100, two layers 150. Sum of ceilings ≈ 1 100
calls for the eleven agents; one that reaches its ceiling hands off a written state and is
replaced fresh, never resumed.

## How a task is handed to an agent

As `docs/plans/phase-67.md` § "How a task is handed to an agent", with `phase-72.md` in the
first line of the brief, the three lines Phase 69 added, and these:

```
The ADR is docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md; this plan's "Decided" table and §2 defaults are the decision, do not re-open them.
Migration 087 exists; never edit it. A shape it lacks is a finding: stop and report it.
GitHub is never reached: every GitHub response in a test is a fixture under crates/tack-api/tests/fixtures/github/ served by the existing mock-server pattern of github_sync.rs's tests.
prill is never run in a test except as the fake under tests/common; real prill output is a fixture under crates/tack-runner/src/verify/fixtures/prill/.
```

Worktrees are made by the launcher from `develop` (`git worktree add /var/tmp/tack-worktrees/p72-<id>
-b phase72/<id> develop`); the Agent tool's own isolation cuts from `main`.

## The tasks

### 0 — fixtures and mechanical fixes (integrator)

**Do:**
- With prill from source (`uv sync --extra sign`) on a scratch repository: `prill attest keygen`,
  a `prill verify --sign` run, and save `mrp.json`, `mrp.intoto.json`, `prill.pub`, plus the
  envelope verified with the right commit and refused with another (`subject_mismatch`), each
  command's stdout and exit code, under `crates/tack-runner/src/verify/fixtures/prill/signed/`
  with a `.provenance` file. The private key is not kept.
- GitHub responses, from GitHub's REST docs examples, trimmed, under
  `crates/tack-api/tests/fixtures/github/`: `check-runs` all success, one failure, one in
  progress, none; `status` success, pending with zero statuses, failure; `merge` 200, 405, 409.
- The ADR's "Related fixes" that are text: "Assay" → prill in ADR 0071 and in
  `docs/contracts/mrp-v1/fixtures/*.json`; the `require_token` comment in `middleware.rs` and
  `README.md:136` gain the `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` exception.
- Apply the cuts the owner took to the ADR text and mark it ACCEPTED.

### S — migration 087 (Haiku)

**Files:** `crates/tack-db/src/migrations.rs`, its migration test.

**Do:** add `MIGRATION_087` and `ordinary("087_autonomy", …)`, exactly:

```sql
ALTER TABLE projects ADD COLUMN autonomy_level TEXT NOT NULL DEFAULT 'L3'
    CHECK (autonomy_level IN ('L3','L4','L5'));
ALTER TABLE projects ADD COLUMN autonomy_required_stages TEXT NOT NULL DEFAULT '["criteria","suite"]';
ALTER TABLE projects ADD COLUMN attestation_public_key TEXT;
ALTER TABLE projects ADD COLUMN verify_loop_max_attempts INTEGER NOT NULL DEFAULT 0
    CHECK (verify_loop_max_attempts >= 0);
ALTER TABLE projects ADD COLUMN pqc_window_days INTEGER NOT NULL DEFAULT 14
    CHECK (pqc_window_days > 0);
CREATE TABLE autonomy_domains (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    path_globs TEXT NOT NULL DEFAULT '[]',
    item_types TEXT NOT NULL DEFAULT '[]',
    level TEXT NOT NULL CHECK (level IN ('L3','L4','L5')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(project_id, name)
);
CREATE TABLE autonomy_merges (
    attempt_id TEXT PRIMARY KEY NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    domain TEXT,
    level TEXT NOT NULL,
    policy_snapshot TEXT NOT NULL,
    pack_artifact_id TEXT NOT NULL,
    attestation_keyid TEXT,
    ci_state TEXT,
    held_reason TEXT,
    checked_at TEXT NOT NULL,
    merged_at TEXT,
    merge_sha TEXT,
    demoted_at TEXT
);
CREATE INDEX idx_autonomy_merges_project ON autonomy_merges(project_id, merged_at);
CREATE TABLE project_budgets (
    project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tokens_per_day INTEGER,
    usd_per_day REAL,
    auto_merges_per_day INTEGER,
    queue_items_max INTEGER
);
CREATE TABLE item_brief_versions (
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    acceptance TEXT NOT NULL,
    constraints TEXT NOT NULL,
    definition_of_done TEXT,
    risk TEXT,
    brief_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY(item_id, version)
);
ALTER TABLE pull_requests ADD COLUMN merge_sha TEXT;
ALTER TABLE pull_requests ADD COLUMN escaped_defect_at TEXT;
```

Then one row: each existing `item_briefs` row becomes version 1 in `item_brief_versions`
(`brief_sha256` = `''` for those rows; B fills new ones).

**Test (RED first):** in the migration test, a database at 086 with one project and one brief
migrates; the project reads `L3`, `0`, `14`; the brief has a version 1.

**Done when:** `cargo nextest run --workspace -E 'binary(tack_db)'` is green; the gate is green.

### A1 — the policy as a pure function (Haiku)

**Files:** new `crates/tack-core/src/autonomy.rs`; `crates/tack-core/src/lib.rs` (one `pub mod`).

**Do:**
- `enum Level { L3, L4, L5 }` with `FromStr`/`Display` (`"L3"`…), `Ord`, and `fn demoted(self) -> Level` (L5→L4→L3→L3).
- `struct Domain { name: String, path_globs: Vec<String>, item_types: Vec<String>, level: Level }`.
- `fn level_for(project: Level, domains: &[Domain], paths: &[String], item_type: &str) -> (Level, Option<String>)`:
  a domain matches when **every** path matches one of its globs and `item_types` is empty or
  holds `item_type`; with no matching domain the result is `(L3, None)` unless the project is
  L5, which gives `(L5, None)`; with several, the lowest level and that domain's name. Globs use
  the `globset` crate, a new dependency of `tack-core` (no glob crate is in the workspace
  today; version by the floor rule). `**` crosses directories, `*` does not.
- `struct Facts { decision: MergeDecision, level: Level, stages: Vec<(String, String)>, required_stages: Vec<String>, attestation: Check, budget: Check, pr_open: Check, ci: CiState }` with `enum Check { Pass, Fail(String) }` and `enum CiState { Green, Pending, Red, Absent }`.
- `fn evaluate(f: &Facts) -> Result<(), Hold>` checking in the order of the Decided table and
  returning the first failure. `Hold` is an enum with one variant per condition and a `Display`
  that is one plain sentence ("The verifier did not say merge", "Stage suite was not_applicable,
  and this project requires it to pass", "CI is still running on <short sha>", …).

**Test (RED first):** one table test in the module: a row per `Hold` variant plus the all-pass
row; three `level_for` rows (one domain, spanning two domains, none at L5).

**Done when:** `cargo nextest run -p tack-core` is green; the gate is green.

### B — the brief keeps its history; its translation is checked against prill's schema (Sonnet)

**Serves:** decision 7 and the "metric criterion" related fix.
**Files:** `crates/tack-db/src/repo/briefs.rs` and its tests; `crates/tack-api/src/handlers/{briefs*,executions.rs,export.rs}` (where the request snapshot is built and where export carries briefs); `crates/tack-core/src/brief.rs`; `docs/contracts/brief-v1/` (schema + one fixture); `crates/tack-runner/src/verify/prill.rs` (`briefing()` only); `crates/tack-runner/Cargo.toml` (dev-dep); a new `crates/tack-runner/tests/briefing_schema.rs`.
**Do:**
1. `upsert_item_brief` writes the next `item_brief_versions` row in the same transaction, with
   `brief_sha256` of the brief-v1 JSON exactly as the runner stages it; an unchanged brief writes
   no row. `GET …/brief/versions` lists them.
2. The request snapshot records `brief_version`. Export carries the versions; import restores them.
3. brief-v1: a metric criterion gains optional `command` and `pattern` (fixture first). The
   adapter emits a `MetricCriterion` when both are present (`gte`→`>=`, `lte`→`<=`, `eq`→`==`,
   `title`→`description`); without them the hint says "has no command to measure it". A dropped
   `cwd` or `runner` gets a hint too.
4. `briefing_schema.rs` validates `briefing()`'s output for every brief-v1 fixture against
   `$PRILL_SRC/contracts/briefing-v1/schema.json` (default `~/Sites/prill`), and prints a skip
   line and passes when the file is absent.
**Test (RED first):** two upserts with different acceptance give versions 1 and 2, an identical
third gives none; the metric row of the schema test fails before step 3.
**Done when:** the four named test binaries are green with prill's checkout present; the gate is green.
**Hand-off:** Sonnet, three thin layers along one data path, ceiling 150.

### G — Tack reads CI and can merge (Sonnet)

**Serves:** decision 1 (the GitHub half), decision 4(d)'s defect.
**Files:** `crates/tack-api/src/github_sync.rs` and its tests; `crates/tack-db/src/repo/pull_requests.rs`.
**Do:**
1. `pub async fn ci_state(client, base, token, repo, sha) -> anyhow::Result<CiState>` reading
   `GET /repos/{repo}/commits/{sha}/check-runs` and `…/status` with Q4's rule; `CiState` is A1's.
2. `pub async fn merge_pull_request(client, base, token, repo, number, sha) -> Result<String, MergeRefused>`
   returning the merge commit sha, or GitHub's `message` on 405/409.
3. In `poll_repo`, for each `pull_requests` row merged within its project's `pqc_window_days`
   with a `merge_sha` and no `escaped_defect_at`: a `Red` CI on `merge_sha` sets
   `escaped_defect_at`. A merge detected by the poll stores `merge_sha` from the PR's
   `merge_commit_sha`.
**Test (RED first):** one table test over the fixtures for `ci_state` (seven rows), one for
`merge_pull_request` (three), one poll test that marks a defect. Same mock-server pattern as
`push_closes_issue_with_correct_request`.
**Done when:** `cargo nextest run --workspace -E 'binary(tack_api)'` is green; the gate is green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### K — signed packs, stage statuses, and the board's check (Sonnet)

**Serves:** decision 3; finding 1.
**Files:** `crates/tack-runner/src/{config.rs, config/tests.rs, verify/prill.rs, verify/tests.rs, engine.rs (artifact staging only)}`; `docs/contracts/mrp-v1/` (schema + fixture); `crates/tack-core/src/mrp.rs`; new `crates/tack-api/src/attest.rs`; `crates/tack-api/src/config.rs` (`TACK_PRILL_PROGRAM`, default `prill`).
**Do:**
1. `[verify] sign_key = "<path>"` (optional). When set, the runner passes `--sign <path>`; prill's
   `signing_unavailable` refusal fails the verification with that reason — never a silent
   unsigned retry.
2. The envelope becomes a second artifact (`application/vnd.in-toto+json`); `signature` becomes
   `InToto { statement_sha256 }` of the envelope's payload.
3. `mrp-v1` gains optional `stages: [{name, status}]` (fixture first), filled from prill's
   `stages`; `MergeReadinessPack.stages: Option<Vec<Stage>>`.
4. `attest::verify(envelope: &Path, public_key_pem: &str, commit: &str) -> Check` (A1's type):
   writes the key to a 0600 temp file, runs `<TACK_PRILL_PROGRAM> attest verify --key … --commit …`
   with a 20-second timeout; exit 0 is `Pass` and records the key id from stdout; anything else,
   or a missing binary, is `Fail` with prill's reason or "prill is not installed on the board".
**Test (RED first):** the round-0 signed fixture: the adapter stages two artifacts and an
`InToto` signature; `attest::verify` with a fake prill script printing the fixture's outputs gives
`Pass`, `Fail(subject_mismatch)` and `Fail(not installed)`. A config file with `sign_key` parses.
**Done when:** runner and api test binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### U — budgets stop new work, and say why (Sonnet)

**Serves:** decision 5 as cut by X3.
**Files:** new `crates/tack-db/src/repo/budgets.rs`; `crates/tack-orch/src/scheduler/{select.rs, types.rs}` and their tests; `crates/tack-api/src/handlers/executions.rs` (the waiting-reason text only).
**Do:**
1. Repo: get/put `project_budgets`; `spent_today(project_id)` → tokens and attempts from
   attempts' usage since local midnight UTC; `queue_items(project_id)` → items waiting on a human
   (packs with no review plus held `autonomy_merges` rows); `auto_merges_today(project_id)`.
2. Scheduler: a new ineligibility reason `BudgetReached { which }` before a claim, written to
   `waiting_since` like phase 70's reasons. Texts: "This project used its tokens for today",
   "This project's review queue is full (N waiting)". Running attempts are untouched.
3. `usd_per_day` is checked only when a rate card exists (P's `rate_card::price`); before P
   merges, it is read and ignored, with a `// P wires the price` marker removed by P.
**Test (RED first):** a scheduler table row per budget; a request in a project over its queue
budget stays Queued with the reason, and a project with no row is unaffected.
**Done when:** db and orch test binaries green; the gate is green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### L — a rejected change goes back to the agent first (Sonnet)

**Serves:** decision 2 as cut by X6.
**Files:** `crates/tack-api/src/handlers/runner_protocol.rs` (after `apply_status_map_policy` in `submit_completion`), `crates/tack-db/src/repo/execution.rs` (`enqueue_execution`, called as is), `crates/tack-api/tests/handlers/` (one file).
**Do:** when a completed attempt's pack says `reject` and the project's `verify_loop_max_attempts`
is above the number of requests in its `rework_of` chain: create a follow-up request on the same
item, same harness, profile and model, with `metadata.rework_of` = the original request, and
the pack's failed criteria appended one per line inside ADR 0077 decision 13's fenced block. No
follow-up when: the last two packs in the chain have the same set of failed criterion ids
(no progress), a budget of U is reached, the previous follow-up was cancelled, or the pack says
`review`. The item then waits in Needs your review as today.
**Test (RED first):** one table test, a row per stop condition plus the loop row, against a fake
runner completion.
**Done when:** the api test binary is green; the gate is green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### P — PQC that does not move, and dollars when there are prices (Sonnet)

**Serves:** decision 4(c) and 4(d).
**Files:** `crates/tack-db/src/repo/metrics.rs`; `crates/tack-api/src/handlers/metrics.rs`; new `crates/tack-api/src/rate_card.rs`; `crates/tack-api/src/router.rs` (two routes); their tests.
**Do:**
1. `outcomes` returns `pqc` (merged, not reverted, no `escaped_defect_at`, window closed),
   `pqc_pending` (merged, window open) and `merged_not_reverted` (today's figure, renamed).
2. `GET/PUT /api/settings/rate-card` stores `{as_of, models: {"<model>": {input_per_mtok, output_per_mtok}}, ci_minute, human_hour}` in `app_meta` key `rate_card`. `rate_card::price(model, tokens_in, tokens_out) -> Option<f64>`.
3. With a rate card, the metrics response adds `cost_usd`, `pqc_per_usd` per task and per
   repository, and `rate_card_as_of`; without one, those keys are absent. A harness's own
   `cost_usd` is never summed. Remove U's marker and wire `usd_per_day`.
**Test (RED first):** rows for a change in window (pending), reverted, with a defect, and closed
(PQC); a response with and without a rate card.
**Done when:** db and api test binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### A2 — the merge, the settings and demotion (Sonnet)

**Serves:** decision 1.
**Files:** new `crates/tack-db/src/repo/autonomy.rs`; new `crates/tack-api/src/autonomy.rs` (`tick`); new `crates/tack-api/src/handlers/autonomy.rs`; `crates/tack-api/src/{lib.rs (one call in the poll loop), router.rs, openapi.rs, github_sync.rs (demotion at revert detection)}`; `crates/tack-cli/src/{main.rs, mcp.rs}`; one new `crates/tack-api/tests/handlers/autonomy.rs`.
**Do:**
1. `GET/PUT /api/projects/{id}/autonomy`: level, domains (replace-all list), required stages,
   attestation public key, loop cap, PQC window, budgets. Promotion is this PUT; nothing else
   raises a level. `tack project autonomy <project>` prints it; MCP `get_autonomy` returns it.
2. `tick`: candidates are attempts of projects above L3 with a `merge` pack, no review and no
   merged row. For each, gather A1's `Facts` (paths from the evidence file list; G's `ci_state`;
   K's `attest::verify`; U's budgets; `open_pull_request_for_attempt`), call `evaluate`, upsert
   the `autonomy_merges` row (held reason, or merged with `merge_sha` and the policy snapshot),
   and on a merge run `apply_mrp_accepted`'s path and broadcast as today.
3. Demotion: where `github_sync` marks a PR reverted, and when G sets `escaped_defect_at`, an
   automatically merged attempt's domain (or project) takes `Level::demoted()`; `demoted_at` is
   set; an activity event names the domain, the old and new level and the PR.
**Test (RED first):** the phase's acceptance test, end to end against the mock GitHub and a fake
prill: a pack that should merge is held for "CI still running", merges on the next tick with the
head sha, and a revert then demotes the domain from L4 to L3; plus one row each for: L3 project
untouched, unlinked item, poll off, unsigned pack, required stage `not_applicable`, merge budget spent.
**Done when:** api, db and cli test binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, two layers (db + api, CLI/MCP thin), ceiling 150.

### P-ui — the Factory page says what PQC means and what it cost (Haiku)

**Files:** `frontend/src/features/dashboard/{FactoryMetrics.tsx, FactoryMetrics.test.tsx}`; the settings panel that holds board preferences (`rg -ln "local-runner" frontend/src/features/settings/panels`) for the rate card form.
**Do:** show "PQC" with "N pending (window: 14 days)" under it and "Merged, not reverted" as
its own line; when `cost_usd` is present, a cost line and PQC per dollar with "Prices as of
<date>"; when absent, nothing new. A rate-card form: one row per model string plus CI minute and
human hour, and the date.
**Test (RED first):** the page with and without `cost_usd`; the form round-trips.
**Done when:** vitest and typecheck green; the gate is green.
**Hand-off:** Haiku, frontend, ceiling 60.

### F — the Automation panel and the held reason (Sonnet)

**Files:** `frontend/src/features/settings/panels/{AutomationPanel.tsx, panels.test.tsx}`; `frontend/src/shared/runWithAgent/{MrpPanel.tsx, MrpPanel.test.tsx}`; the item's activity rendering for the demotion event.
**Do:** an **Autonomy** section: the level (L3/L4/L5, each with one plain sentence), domains
(name, globs, item types, level; each shows its merged / reverted / defect counts before its level
control, per decision 1), required stages, the public key, the loop cap, the four budgets and the
PQC window. L4/L5 controls render disabled with their reason when the board has no GitHub poll or
no public key (the deferred-shown-disabled rule). The pack panel shows the held reason, or
"Merged by Tack (L4, domain docs) — policy" with the snapshot behind a disclosure.
**Test (RED first):** the section saves; a disabled L4 shows its reason; the pack panel shows both states.
**Done when:** vitest and typecheck green; Playwright `--workers=2` headless green; the gate is green.
**Hand-off:** Sonnet, frontend, ceiling 100.

### Z — docs and the integrated gate (integrator)

`docs/book/src/user-guide/` (one page: levels, domains, what holds a merge, how demotion works,
the signing key), `docs/CONFIG.md` (`[verify] sign_key`, `TACK_PRILL_PROGRAM`), CHANGELOG
`[Unreleased]` by hand, ADR 0079 status line. `.githooks/pre-push` once on the merged tree;
`scripts/smoke.sh` fake mode.

## Status

Not started.
