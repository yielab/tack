# Plan: Phase 72 — merge authority follows the autonomy level you set

**Status: proposed 2026-10-13; cuts X1, X2, X3, X5 and X6 taken by the owner the same day, X4
and acceptance pending.** Serves ADR 0079
(`docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md`), whose closing section
"Amendments agreed 2026-10-13" wins where it disagrees with the table. Written against `develop`
at `11de5f2e`, prill at `324842e` (ADR 0014 accepted, M6 specified, not built) and docket at
`e8774fad` (ADR 0025 accepted with amendments). Re-run `rg -n` before trusting a line number.

What this phase builds, in one paragraph. A project can let Tack merge a change by itself (L4)
inside the parts of the code the person authorised, or everywhere (L5). Tack merges only when
prill said `merge` under the stages the project requires, the pack is signed for the exact head
commit, CI is green on that commit and no budget is spent; otherwise the item waits in Needs your
review with the first failing condition named. A rejected change goes back to the agent with
prill's next steps a bounded number of times before a human sees it. A revert takes the level back
down by itself. Briefs keep their history. The Factory page shows prill's quality figures beside
Tack's cost. A project that stays at L3 behaves exactly as today.

## Who owns what

The owner's rule (2026-10-13): each responsibility has exactly one owner among the three tools;
nothing is computed twice and nothing is left half-built. This table is that rule applied, read
from Tack ADR 0079, prill ADR 0014 and docket ADR 0025 as each stands today.

| Responsibility | Owner | What the others do |
|---|---|---|
| The work order (brief, its history) | **Tack** (`brief-v1`) | Tack's adapter translates it to prill's `briefing/v1`; prill reads it. |
| Running the agent, per-run limits, sandbox, `no_progress` inside a run | **docket** (or another harness) | Tack passes `--max-tokens` / `--max-budget-usd` from the profile. |
| The verdict, required stages, what to fix next, the signature | **prill** (`require`, `nextSteps`, `--sign`) | Tack passes the project's required stages as a `--config` layer and the key path; it never re-checks a stage. |
| Looping a rejected change back to the agent (count, budget, stop) | **Tack**, the caller of `docket exec` (docket ADR 0025 cut its own exec gate: "the caller closes the loop") | prill hands the `nextSteps`; docket only runs the new attempt. |
| Deciding and making the merge; reading CI; all GitHub reads and writes | **Tack** (P5) | prill never merges; docket never merges. Tack does not use `prill poll github`: one GitHub reader. |
| Observing outcomes (merged, closed, reverted, red CI) | **Tack** observes them | Tack forwards each one with `prill record outcome`. |
| Quality figures: escalation, agreement, escaped defects, production-qualified changes | **prill** (`metrics/v1`, `qualified` in M6) | Tack shows them and **computes none of them**. Tack keeps only the facts it owns about its pull requests ("opened", "merged, not reverted"). |
| Money | **Tack** (rate card) | prill and docket never price. Tack divides prill's `qualified` by its own cost. |
| The autonomy level and its domains; promotion; demotion | **Tack** (the operator's setting) | prill M7 will *recommend* levels (`autonomy-policy/v1`), shown as advice. docket's `lights-out` pods are its own standalone use, not Tack's domains. |
| Grouping corrections into proposed directives | **prill** (M7, `directives-proposal/v1`) | Tack does not group. |
| Approving a directive and writing it into the repository | **Tack** (a human approves; Tack opens the PR) | docket, Codex and opencode read `AGENTS.md` from the checkout. Not built in this phase: it waits for prill M7 (cut X1). |

## Owner decisions

### Cuts (2026-10-13)

| # | Decision | State |
|---|---|---|
| X1 | Decision 6 (directives) is not built. Tack will apply directives prill proposes; it never groups them itself. Trigger: prill publishes `directives-proposal/v1` and a contract through which Tack sends the human corrections it holds. | **Taken** |
| X2 | Reshaped by the ownership rule: Tack drops its own PQC (ADR 0079 decision 4(d)) and keeps 4(a), 4(b), 4(c). prill's store lives **on the board's host**, one per repository, so outcomes never cross machines. Without prill on the board, the page shows Tack's own facts and says the quality figures need prill. | **Taken (reshaped)** |
| X3 | Budgets: `tokens_per_day`, `usd_per_day`, `auto_merges_per_day`, `queue_items_max`; enforced at the attempt boundary and in the merge check. | **Taken** |
| X4 | L5 has no limits of its own: L5 is L4 over every domain, because one revert already demotes. | **Pending** |
| X5 | No `brief_version` in the pack; the request snapshot records it, the hash already identifies it. | **Taken** |
| X6 | Stopping the loop is cancelling the follow-up run. | **Taken** |

### Open questions — the default this plan builds with

| Q | Default built |
|---|---|
| 1 Signing key | A file path in the runner's `[verify] sign_key`; the board stores the project's public key (PEM). |
| 2 Defect window | prill's: 14 days, a constant of `prill metrics` (ADR 0014). Tack keeps no window of its own. |
| 3 Second person for L4 | No; the merge record holds the policy, not a person (ADR 0059 stands). |
| 4 Authoritative CI | Both GitHub signals on the commit: every check run completed `success`/`neutral`/`skipped`, and the combined status not `failure`/`pending`. No check and no status at all is **not** green. |
| 5 Merge method | GitHub's default, branch kept; a 405 holds the item with GitHub's message. |
| 6 Rate card | By hand, dated; every dollar figure shows the date. |
| 7 Directive threshold | Moot (X1): prill's to decide. |
| 8 Loop cap | Per item, plus the project's daily token and dollar budgets. |
| 9 Required stages | prill's `require`, sent by the runner as a `--config` layer from the project's list. L4 refuses an empty list. The Automation panel offers prill's L4 example (`criteria`, `suite`, `anti_tautology`, `static_analysis`). |
| 10 L5 queue | Kept; filled only on failure. |

## What reading the code found

1. **Two prill features this phase needs are specified but not built** (prill M6): `require`
   and `nextSteps`. As with docket (negotiated, not pinned), Tack targets them by name and
   probes at runtime: a runner whose prill lacks `require` holds every L4 merge with "prill
   <version> cannot require stages"; a pack without next steps goes to a human instead of
   looping. No Tack-side stand-in for either. Task K2 waits on that prill.
2. **GitHub's combined status misses GitHub Actions** (check runs), hence Q4. prill's own
   `poll github` reads only the combined status — a note for prill, not a Tack task.
3. **A pull request opens only for an item linked to a GitHub issue**
   (`handlers/pull_requests.rs:7-8`). L4 holds an unlinked item with that reason.
4. **The GitHub poll runs only with the board's environment token** (`lib.rs:33`). The merge
   check runs inside it; with the poll off L4 holds with that reason.
5. **Tack's revert rule misses `git revert`**: it reads PR titles (`github_sync.rs:464-498`).
   prill's rule (a default-branch commit saying `This reverts commit <merge sha>`) catches it.
   Tack, as the one GitHub reader, uses both.
6. **The ADR's default directive path is read by no agent.** `directives/<slug>.md` is not
   loaded by docket, Codex or opencode; `AGENTS.md` is. Recorded for when X1's trigger fires.
7. **Stale in the ADR:** next migration is `087`; `prill.rs:690` is `:684`; there is no
   `settings` table (the rate card goes in `app_meta`, key `rate_card`).
8. **New crates:** `globset` in `tack-core` (domains), `jsonschema` as a dev-dependency of
   `tack-runner` (contract test), both by the floor rule.

## Decided — not re-opened by any task

Every row of ADR 0079's table as amended, the "Who owns what" table, the cuts, the defaults, and:

| Question | Decision |
|---|---|
| Schema | One migration, `087_autonomy` (task S), ahead of its readers — the 078–081 precedent. A reader needing another shape adds a new migration; it never edits 087. |
| The policy | A pure function in `tack-core::autonomy` (A1): facts in, `Ok` or the first failing condition out. |
| Where it runs | In the GitHub poll loop, after `poll_once`, as `autonomy::tick`. The completion handler records; it never merges. |
| Holds | A row in `autonomy_merges` with `merged_at` NULL and `held_reason` set. One table for merged and held. |
| The merge | `PUT /repos/{repo}/pulls/{n}/merge` with `sha` = `actual_execution.git.head_commit`. After it, the item follows `done_on_mrp_accepted` as if accepted. |
| Order inside a tick | pack says merge → domain ≥ L4 → the request sent a non-empty `require` that the runner's prill honours → attestation verifies for the head commit → budgets → open the PR → CI on head green → merge. |
| Demotion | A revert or red CI on the merge commit of an automatically merged change drops its domain (or the project) one level; an activity event names it. |
| prill on the board | `TACK_PRILL_PROGRAM` (default `prill`). The board runs only `attest verify`, `record mrp`, `record outcome` and `metrics`, against `<storage_dir>/prill/<owner>__<repo>.db`. Absent: no L4, no quality figures, each with its reason. |
| MCP | One read-only tool, `get_autonomy`. |
| Generated files | Each task regenerates and stages them; on a conflict the integrator regenerates. |

## Rounds

At most two agents building cargo at once; a frontend-only task runs beside them.

| Round | Tasks | Needs |
|---|---|---|
| 0 | fixtures + mechanical fixes (integrator); S, A1 (Haiku) | ADR accepted |
| 1 | B, G | S; A1 (G) |
| 2 | K1, U | S, A1 |
| 3 | A2, P; P-ui beside | G, K1, U (A2); G (P); P (P-ui) |
| 4 | L; F beside | K1, U (L); A2 (F) |
| 5 | K2 | prill with M6 (`require`, `nextSteps`) and its captured fixtures |
| 6 | Z (integrator) | everything |

Rounds 0–4 do not wait on prill. Until K2 lands, L4 holds every merge with finding 1's reason and
the loop sends packs to a human: correct by P3, nothing faked. Ceilings: Haiku 60; Sonnet one
layer 100, two layers 150 — twelve agents, ≈ 1 200 calls at the ceiling.

## How a task is handed to an agent

As `docs/plans/phase-67.md` § "How a task is handed to an agent", with `phase-72.md` in the
first line of the brief, the three lines Phase 69 added, and these:

```
The ADR is docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md; its closing amendments and this plan's "Who owns what" and "Decided" tables are the decision, do not re-open them.
Tack computes no quality figure prill owns and re-checks no stage prill judged. If your task seems to need one, stop and report it.
Migration 087 exists; never edit it. A shape it lacks is a finding: stop and report it.
GitHub is never reached: every GitHub response in a test is a fixture under crates/tack-api/tests/fixtures/github/ served by the mock-server pattern of github_sync.rs's tests.
prill is never run in a test except as a fake script printing captured output from crates/tack-runner/src/verify/fixtures/prill/ or crates/tack-api/tests/fixtures/prill/.
```

Worktrees: `git worktree add /var/tmp/tack-worktrees/p72-<id> -b phase72/<id> develop`, made by
the launcher (the Agent tool's own isolation cuts from `main`).

## The tasks

### 0 — fixtures and mechanical fixes (integrator)

- prill from source with `uv sync --extra sign`, on a scratch repository: `attest keygen`; a
  `verify --sign` run (`mrp.json`, `mrp.intoto.json`, `prill.pub`, stdout, exit code);
  `attest verify --commit` with the right commit and with another (`subject_mismatch`);
  `record mrp`, `record outcome` (merged, reverted) and `metrics --json` against a scratch store.
  Under `crates/tack-runner/src/verify/fixtures/prill/signed/` and
  `crates/tack-api/tests/fixtures/prill/`, each with a `.provenance` file. No private key is kept.
- GitHub responses from GitHub's REST docs, trimmed, under `crates/tack-api/tests/fixtures/github/`:
  `check-runs` (all success, one failure, one in progress, none), `status` (success, pending with
  zero statuses, failure), `merge` (200, 405, 409), and a default-branch `commits` list holding a
  `This reverts commit <sha>` message.
- Text fixes: "Assay" → prill in ADR 0071 and `docs/contracts/mrp-v1/fixtures/*.json`; the
  `require_token` comment and `README.md:136` gain the
  `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` exception.
- ADR 0079's status line, once accepted.

### S — migration 087 (Haiku)

**Files:** `crates/tack-db/src/migrations.rs`, its migration test.

**Do:** add `MIGRATION_087` and `ordinary("087_autonomy", …)`, exactly:

```sql
ALTER TABLE projects ADD COLUMN autonomy_level TEXT NOT NULL DEFAULT 'L3'
    CHECK (autonomy_level IN ('L3','L4','L5'));
ALTER TABLE projects ADD COLUMN verify_require TEXT NOT NULL DEFAULT '[]';
ALTER TABLE projects ADD COLUMN attestation_public_key TEXT;
ALTER TABLE projects ADD COLUMN verify_loop_max_attempts INTEGER NOT NULL DEFAULT 0
    CHECK (verify_loop_max_attempts >= 0);
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
ALTER TABLE pull_requests ADD COLUMN red_ci_at TEXT;
CREATE TABLE prill_records (
    attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('mrp','merged','closed','reverted','red_ci')),
    recorded_at TEXT NOT NULL,
    PRIMARY KEY(attempt_id, kind)
);
```

Then each existing `item_briefs` row becomes version 1 (`brief_sha256` = `''`).

**Test (RED first):** a database at 086 with one project and one brief migrates; the project
reads `L3`, `[]`, `0`; the brief has a version 1.
**Done when:** `cargo nextest run --workspace -E 'binary(tack_db)'` and the gate are green.

### A1 — the policy as a pure function (Haiku)

**Files:** new `crates/tack-core/src/autonomy.rs`; `crates/tack-core/src/lib.rs`; `crates/tack-core/Cargo.toml` (`globset`).

**Do:**
- `enum Level { L3, L4, L5 }` with `FromStr`/`Display` (`"L3"`…), `Ord`, `fn demoted(self) -> Level` (L5→L4→L3→L3).
- `struct Domain { name: String, path_globs: Vec<String>, item_types: Vec<String>, level: Level }`.
- `fn level_for(project: Level, domains: &[Domain], paths: &[String], item_type: &str) -> (Level, Option<String>)`:
  a domain matches when every path matches one of its globs (`**` crosses directories, `*` does
  not) and `item_types` is empty or holds `item_type`. No match: `(L3, None)`, or `(L5, None)`
  when the project is L5. Several: the lowest level and its name.
- `enum Check { Pass, Fail(String) }`; `enum CiState { Green, Pending, Red, Absent }`.
- `struct Facts { decision: MergeDecision, level: Level, require: Check, attestation: Check, budget: Check, pr_open: Check, ci: CiState }`.
- `fn evaluate(f: &Facts) -> Result<(), Hold>` in the Decided order; `Hold` has one variant per
  condition and a `Display` of one plain sentence ("prill did not say merge", "CI is still running
  on <short sha>", …); `require` and the others carry their own reason through `Fail`.

**Test (RED first):** one table test: a row per `Hold` variant and the all-pass row; three
`level_for` rows (one domain, two domains, none at L5).
**Done when:** `cargo nextest run -p tack-core` and the gate are green.

### B — the brief keeps its history; its translation is checked against prill's schema (Sonnet)

**Serves:** decision 7 as amended by X5, and the "metric criterion" related fix.
**Files:** `crates/tack-db/src/repo/briefs.rs` and its tests; `crates/tack-api/src/handlers/{briefs*, executions.rs, export.rs}`; `crates/tack-core/src/brief.rs`; `docs/contracts/brief-v1/` (schema + one fixture); `crates/tack-runner/src/verify/prill.rs` (`briefing()` only); `crates/tack-runner/Cargo.toml` (dev-dep `jsonschema`); new `crates/tack-runner/tests/briefing_schema.rs`.
**Do:**
1. `upsert_item_brief` writes the next `item_brief_versions` row in the same transaction, with
   `brief_sha256` of the brief-v1 JSON as the runner stages it; an unchanged brief writes none.
   `GET …/brief/versions` lists them. The request snapshot records `brief_version`. Export
   carries the versions; import restores them.
2. brief-v1: a metric criterion gains optional `command` and `pattern` (fixture first). The
   adapter emits prill's `MetricCriterion` when both are present (`gte`→`>=`, `lte`→`<=`,
   `eq`→`==`, `title`→`description`); otherwise the hint says "has no command to measure it". A
   dropped `cwd` or `runner` gets a hint too.
3. `briefing_schema.rs` validates `briefing()`'s output for every brief-v1 fixture against the
   output of `prill schema briefing` (prill ADR 0014 L-3). When `prill` is absent or has no
   `schema` command, the test prints why and passes.
**Test (RED first):** versions 1, 2, and none for an identical third upsert; the metric
translation rows in `verify/tests.rs`.
**Done when:** the named binaries are green; the gate is green.
**Hand-off:** Sonnet, three thin layers along one data path, ceiling 150.

### G — Tack reads CI, detects reverts and can merge (Sonnet)

**Files:** `crates/tack-api/src/github_sync.rs` and its tests; `crates/tack-db/src/repo/pull_requests.rs`.
**Do:**
1. `pub async fn ci_state(client, base, token, repo, sha) -> anyhow::Result<CiState>` with Q4's rule.
2. `pub async fn merge_pull_request(client, base, token, repo, number, sha) -> Result<String, MergeRefused>`:
   the merge commit sha, or GitHub's `message` on 405/409.
3. In `poll_repo`: a merged PR stores `merge_sha` from `merge_commit_sha`. For each merged row
   in the last 14 days: a default-branch commit whose message says `This reverts commit
   <merge_sha>` marks it reverted, beside the existing title rule; a `Red` CI on `merge_sha` sets
   `red_ci_at`. Nothing here talks to prill.
**Test (RED first):** table tests over the fixtures: `ci_state` (seven rows), `merge_pull_request`
(three), one poll test for each of revert-by-commit and red CI.
**Done when:** `cargo nextest run --workspace -E 'binary(tack_api)'` and the gate are green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### K1 — signed packs, next steps in Tack's pack, and the board's check (Sonnet)

**Serves:** decision 3; the Tack half of the loop's input.
**Files:** `crates/tack-runner/src/{config.rs, config/tests.rs, verify/prill.rs, verify/tests.rs, engine.rs (artifact staging only)}`; `docs/contracts/mrp-v1/` (schema + fixture); `crates/tack-core/src/mrp.rs`; new `crates/tack-api/src/prill.rs`; `crates/tack-api/src/config.rs` (`TACK_PRILL_PROGRAM`).
**Do:**
1. `[verify] sign_key = "<path>"` (optional). When set, the runner passes `--sign <path>`; a
   `signing_unavailable` refusal fails the verification with that reason, never an unsigned retry.
2. The envelope becomes a second artifact (`application/vnd.in-toto+json`); `signature` becomes
   `InToto { statement_sha256 }`.
3. Tack's `mrp-v1` gains optional `next_steps: [{code, severity, stage?, criterion_id?, path?, line?, command?, message?}]`
   (fixture first) and `MergeReadinessPack.next_steps: Option<Vec<NextStep>>`. K1 does not fill
   it from prill (K2 does).
4. `crates/tack-api/src/prill.rs`: one `run(args, timeout) -> Result<Output, Unavailable>` for the
   board's prill, and `attest_verify(envelope, public_key_pem, commit) -> Check` (key in a 0600
   temp file, 20-second timeout; exit 0 is `Pass` with the key id from stdout; anything else, or
   no binary, is `Fail` with prill's reason or "prill is not installed on the board").
**Test (RED first):** with the round-0 signed fixture, the adapter stages two artifacts and an
`InToto` signature; `attest_verify` with a fake prill gives `Pass`, `Fail(subject_mismatch)` and
`Fail(not installed)`; a runner file with `sign_key` parses.
**Done when:** runner and api binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### U — budgets stop new work, and say why (Sonnet)

**Files:** new `crates/tack-db/src/repo/budgets.rs`; `crates/tack-orch/src/scheduler/{select.rs, types.rs}` and their tests; `crates/tack-api/src/handlers/executions.rs` (the waiting-reason text only).
**Do:**
1. Repo: get/put `project_budgets`; `spent_today(project_id)` (tokens from attempts' usage since
   00:00 UTC); `queue_items(project_id)` (packs with no review plus held `autonomy_merges` rows);
   `auto_merges_today(project_id)`.
2. Scheduler: ineligibility `BudgetReached { which }` before a claim, written to `waiting_since`
   like phase 70's reasons ("This project used its tokens for today", "This project's review
   queue is full (N waiting)"). Running attempts are untouched.
3. `usd_per_day` is read and ignored with a `// P wires the price` marker that P removes.
**Test (RED first):** a scheduler row per budget; a project with no row is unaffected.
**Done when:** db and orch binaries and the gate are green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### A2 — the merge, the settings and demotion (Sonnet)

**Files:** new `crates/tack-db/src/repo/autonomy.rs`; new `crates/tack-api/src/autonomy.rs` (`tick`); new `crates/tack-api/src/handlers/autonomy.rs`; `crates/tack-api/src/{lib.rs (one call in the poll loop), router.rs, openapi.rs}`; `crates/tack-cli/src/{main.rs, mcp.rs}`; new `crates/tack-api/tests/handlers/autonomy.rs`.
**Do:**
1. `GET/PUT /api/projects/{id}/autonomy`: level, domains (replace-all), `verify_require`, public
   key, loop cap, budgets. Promotion is this PUT. `tack project autonomy <project>` prints it; MCP
   `get_autonomy` returns it.
2. The request snapshot carries `verify_require` (the runner sends it in K2).
3. `tick`: candidates are attempts of projects above L3 with a `merge` pack, no review and no
   merged row. Gather `Facts` (paths from the evidence file list; `require` = `Fail` when the
   snapshot's list is empty or the runner's report says its prill cannot require — K2 writes that
   report; until then every L4 hold reads "the runner has not reported that its prill can require
   stages"; G's `ci_state`; K1's `attest_verify`; U's budgets; `open_pull_request_for_attempt`),
   call `evaluate`, upsert `autonomy_merges` (held reason, or merged with `merge_sha` and the policy
   snapshot), and on a merge follow `apply_mrp_accepted`'s path and broadcast as today.
4. Demotion: when G marks an automatically merged PR reverted or sets `red_ci_at`, its domain (or
   project) takes `Level::demoted()`, `demoted_at` is set, an activity event names the domain, the
   levels and the PR.
**Test (RED first):** the acceptance test against the mock GitHub and a fake prill: a pack held
for "CI still running" merges on the next tick with the head sha, then a revert demotes its
domain L4→L3; plus one row each: L3 untouched, unlinked item, poll off, unsigned pack, empty
`verify_require`, merge budget spent.
**Done when:** api, db and cli binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, two layers (db + api; CLI/MCP thin), ceiling 150.

### P — prill's figures, Tack's money (Sonnet)

**Serves:** decision 4(a), 4(b), 4(c) as amended by X2.
**Files:** new `crates/tack-api/src/prill_store.rs`; `crates/tack-api/src/{handlers/runner_protocol.rs (one call after completion), github_sync.rs (one call per observation), handlers/metrics.rs, router.rs}`; new `crates/tack-api/src/rate_card.rs`; `crates/tack-db/src/repo/metrics.rs` (rename only); their tests.
**Do:**
1. Store: `<storage_dir>/prill/<owner>__<repo>.db`. After a completion with a pack, `prill record
   mrp --db … mrp.json [--attestation mrp.intoto.json]`; after G records merged, closed,
   reverted or red CI, `prill record outcome` with an `outcome/v1` file (`source: "tack"`,
   `reference` the PR URL). `prill_records` makes each one happen once; a failure is logged and
   retried on the next tick.
2. Metrics: per repository, `prill metrics --db … --json`, returned as `quality` with
   `source: "prill"` and prill's version. Tack's own `pqc`/`pqc_rate` are removed; the same count
   is returned as `merged_not_reverted`. Without prill: `quality` is absent and
   `quality_unavailable` says why.
3. `GET/PUT /api/settings/rate-card` in `app_meta` (`rate_card`): `{as_of, models: {"<model>":
   {input_per_mtok, output_per_mtok}}, ci_minute, human_hour}`; `rate_card::price(model, in, out)`.
   With it the response adds `cost_usd`, `rate_card_as_of` and, when prill returns `qualified`,
   `qualified_per_usd` per repository. A harness's own `cost_usd` is never summed. Remove U's
   marker and wire `usd_per_day`.
**Test (RED first):** with the round-0 prill fixtures and a fake prill: a completion records the
MRP once; a revert records one outcome; the metrics response with and without prill and with and
without a rate card.
**Done when:** api and db binaries green; regenerated files staged; the gate is green.
**Hand-off:** Sonnet, one layer plus a rename, ceiling 150.

### P-ui — the Factory page names its sources (Haiku)

**Files:** `frontend/src/features/dashboard/{FactoryMetrics.tsx, FactoryMetrics.test.tsx}`; the board-preferences settings panel (`rg -ln "local-runner" frontend/src/features/settings/panels`) for the rate card form.
**Do:** a **Quality (from prill <version>)** block per repository with escalation, agreement,
escaped defects and, when present, qualified changes; without it, one line with
`quality_unavailable`. "Merged, not reverted" stays as Tack's own line. With `cost_usd`: cost
and qualified changes per dollar, "Prices as of <date>". A rate-card form: one row per model
string plus CI minute and human hour, and the date.
**Test (RED first):** the page with and without `quality` and `cost_usd`; the form round-trips.
**Done when:** vitest, typecheck and the gate are green.
**Hand-off:** Haiku, frontend, ceiling 60.

### L — a rejected change goes back to the agent first (Sonnet)

**Serves:** decision 2 as amended by X6.
**Files:** `crates/tack-api/src/handlers/runner_protocol.rs` (after `apply_status_map_policy` in `submit_completion`); `crates/tack-db/src/repo/execution.rs` (`enqueue_execution`, called as is); one new file under `crates/tack-api/tests/handlers/`.
**Do:** when a completed attempt's pack says `reject`, carries `next_steps`, and the project's
`verify_loop_max_attempts` is above the length of its `rework_of` chain: enqueue a follow-up on the
same item, harness, profile and model, with `metadata.rework_of` = the original request and the
next steps appended, one per line (`message`, `path:line`, `command`), inside ADR 0077 decision
13's fenced block. No follow-up when: the pack has no `next_steps`, the last two packs carry the
same set of `code`s (no progress), a budget of U is reached, the previous follow-up was
cancelled, or the pack says `review`. The item then waits in Needs your review as today.
**Test (RED first):** one table test, a row per stop condition plus the loop row.
**Done when:** the api binary and the gate are green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### F — the Automation panel and the held reason (Sonnet)

**Files:** `frontend/src/features/settings/panels/{AutomationPanel.tsx, panels.test.tsx}`; `frontend/src/shared/runWithAgent/{MrpPanel.tsx, MrpPanel.test.tsx}`; the item activity rendering for the demotion event.
**Do:** an **Autonomy** section: the level with one plain sentence each; domains (name, globs,
item types, level, and their merged / reverted counts shown before the level control); "Stages
prill must run" with a button that fills prill's L4 example; the public key; the loop cap; the four
budgets. L4/L5 render disabled with their reason when the board has no GitHub poll, no prill or
no public key. The pack panel shows the held reason, or "Merged by Tack (L4, domain docs)" with
the policy snapshot behind a disclosure; when the pack has next steps, it lists them.
**Test (RED first):** the section saves; a disabled L4 shows its reason; both pack panel states.
**Done when:** vitest, typecheck, Playwright `--workers=2` headless and the gate are green.
**Hand-off:** Sonnet, frontend, ceiling 100.

### K2 — the runner sends the required stages and reads prill's next steps (Sonnet; waits on prill M6)

**Needs:** a prill that implements ADR 0014 L-1 and L-2, named by version, and the integrator's
fixtures from it (a run with `require` holding a stage `not_run: required`, an MRP with
`nextSteps`, `prill schema config`).
**Files:** `crates/tack-runner/src/{verify/prill.rs, verify/tests.rs, engine.rs (probe report only)}`; the runner's capability report in `docs/contracts/runner-v1/` (fixture first).
**Do:** when the request snapshot's `verify_require` is non-empty, write a `config/v1` layer
`{require: [...]}` into the run's out dir and pass `--config`; the runner's probe reports
`verify.require_supported` from `prill schema config` (it lists `require`). The adapter fills
`next_steps` from prill's `nextSteps`, redacted text kept as prill wrote it, an unknown code passed
through (the board treats it as `review`, ADR 0014 L-3). A2's hold then reads that report.
**Test (RED first):** with the M6 fixtures: the layer is written and passed; `next_steps`
translates; a prill without `require` reports `false`.
**Done when:** runner binaries green; the e2e `scripts/docket-prill-e2e.sh` passes with an L4
project whose change merges against the mock GitHub; the gate is green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### Z — docs and the integrated gate (integrator)

`docs/book/src/user-guide/` (one page: who owns what, levels, domains, what holds a merge,
demotion, the signing key, prill on the board), `docs/CONFIG.md` (`[verify] sign_key`,
`TACK_PRILL_PROGRAM`), CHANGELOG `[Unreleased]` by hand. `.githooks/pre-push` once on the merged
tree; `scripts/smoke.sh` fake mode.

## Notes for the other tools (not Tack tasks)

- **prill:** `poll github` reads only the combined status and misses GitHub Actions (finding 2).
  M7's corrections input does not exist yet: `outcome/v1` carries no text, so prill has no way to
  receive the human corrections Tack (`mrp_reviews.reason`) and docket (its corrections ledger)
  hold. X1's trigger needs that contract.
- **docket:** nothing. ADR 0025 already leaves the loop to the caller of `exec` and reads
  `AGENTS.md` from the base commit, which is where an approved directive should land (finding 6).

## Status

Not started.
