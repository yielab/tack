# ADR 0068: Tack is maintained by people — retire what only agents needed, delete what nothing calls, and rebuild testing and CI around verified behaviour

**Decide:** approve a plan whose goal is the smallest codebase that still does everything a
user can do today. It rests on four commitments:

1. Retire the development scaffolding that exists only because agents built the tree: the
   card and wave process, the per-card handoffs, the agent skills, the size-budget scripts
   and the tests that proved a card was done.
2. Delete the mechanisms nothing calls, including the legacy Docket control plane. Docket
   stays reachable as a harness under ADR 0066.
3. Rebuild the test suite by layer, so each behaviour is verified once, at the lowest layer
   that can express it.
4. Replace CI's five per-crate coverage floors and ten always-on jobs with a tiered
   pipeline: one coverage measurement for the whole workspace, plus coverage of the lines a
   pull request changes.

**Why now:** Part IX promised a tree a person can maintain, and did not deliver one.
Production code grew by 416 lines, and tests shrank only 6 %. Most of its work moved code
rather than removing it, because it measured file sizes, not what the code is for.
Meanwhile `main` cannot be updated: the coverage job fails on a measurement change, not on
lost coverage. And roughly two thirds of all test lines verify the agent-execution domain,
while board features a person uses every day have none.

**If you do nothing:** the pull request that brings `main` up to date stays red; no release
can be tagged; and every future change keeps paying for 10 legacy Docket tables, two
coexisting orchestration models and an agent process whose last run consumed 5.9 billion
cache-read tokens on mechanical edits.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | **What stays is the product, including every part of it that serves AI agents.** The runner, the harnesses, the runner-v1 protocol, the MCP server and the desktop app are product. What goes is the scaffolding used to *develop* Tack with agents. | "Agent-facing" is Tack's feature. "Agent-built" is how it got here, and nobody who maintains it needs that. |
| 2 | **The legacy Docket control plane is retired.** That means the `ControlPlane` trait, the reconciler, the 10 `orch_*`/`control_planes` tables, the orch routes and tokens, and the Fleet, Approvals, Economics and Provision screens. Docket reaches Tack only as a harness (ADR 0066). **Supersedes ADRs 0060 and 0065.** | ADR 0050 already made the runner the plan of record. ADR 0060 kept the bridge only for DAG dispatch and guardrails; ADR 0066 moves guardrails into docket's own tool loop. It is the largest single block of code and tests in the tree. |
| 3 | **Mechanisms with no caller are deleted.** `model_profiles` (table, routes, MCP tool, panel) goes. The `decisions` path goes from the server, the runner, the UI and the runner-v1 fixtures. The compile-only `github_actions` adapter goes with decision 2. | Nothing reads `model_profiles`. No harness has ever entered a decision state, and ADR 0066 already requires its own ADR to route one. Git keeps the code for the day a caller exists. |
| 4 | **One home for decisions: ADRs. One home for intent: the roadmap. The history is frozen.** `TODO.md`'s card and wave boards, the dispatch plans, the per-card handoff requirement and `.claude/` (skills, context and token budgets) leave the working tree. Existing handoffs and closed boards stay in `docs/closed-cycles/`, read-only and excluded from every check. `CLAUDE.md` becomes a short pointer to the same `CONTRIBUTING.md` and `docs/TESTING.md` a person reads. | An agent should read what a human reads. Maintaining a parallel process for agents is what produced 69 handoff files and a 5 000-line board. |
| 5 | **Tests verify behaviour through a public interface, once per layer.** `tack-core` gets pure unit tests. `tack-db` gets repository tests against SQLite. `tack-api` gets HTTP tests per route: success, auth, validation and each documented error. Runner-v1 and the OpenAPI spec get contract tests. The runner gets fake-harness tests. Critical user journeys get Playwright E2E. Billed or live tests stay `#[ignore]`d and manual. An invariant is tested at the lowest layer that can express it, plus at most one test through the HTTP API. | That is the standard test pyramid. Today one invariant, "a stale lease writes nothing", appears in 12 test files. |
| 6 | **A test that verifies how a card was built, not what the product does, is deleted.** That covers wave gates, narrative tests asserting several unrelated claims, tests of private helpers, and near-duplicates across layers. Missing behaviour tests are added for the board features at or near 0 % coverage: attachments, custom fields, multiple boards. | Development proof belongs in the pull request that needed it, not in the suite that runs forever. |
| 7 | **Coverage is measured once, on the whole workspace, from the same run that executes the tests.** One `cargo llvm-cov nextest --workspace` replaces the plain test run and the five per-crate instrumented builds. One line floor sits 1 point under the measured total. Pull requests must also cover **≥ 80 % of the lines they change** (patch coverage). The Vitest thresholds stay for the frontend. | A per-crate floor ignores that `tack-core` is 83 % covered by its own tests and 96 % by the suite. Patch coverage is what stops new untested code, the thing a floor is meant to guard. |
| 8 | **Assertion quality is checked by mutation testing, not by coverage.** `cargo-mutants` runs weekly on `tack-core` and on `tack-db`'s repository layer, report-only. | Coverage says a line ran; a surviving mutant says no test would notice it breaking. Weekly and report-only keeps it from becoming another gate to chase. |
| 9 | **CI runs in three tiers.** Pull request, under 15 minutes: fmt, clippy, the tests-with-coverage run, frontend typecheck and Vitest, the OpenAPI/schema drift check, and dependency policy and audit. Merge to `main`: adds the single-binary SPA build, the desktop build and Chromium E2E. Nightly or weekly: cross-browser E2E, MSRV, mutation testing, the scheduled audit. `pre-push` keeps only fmt and clippy. | Today every pull request runs 10 jobs, including a 7-minute SPA build and 7 minutes of cross-browser E2E. The ruleset on `main` requires all 10. |
| 10 | **Flaky tests are quarantined, never retried.** nextest keeps `retries = 0`. A test that fails without a code change is `#[ignore]`d with a linked issue the same day, and fixed or deleted within a week. | Retries hide the defect they are retrying. |
| 11 | **Size rules become standard lints, not a custom ratchet.** `scripts/maintainability.py`, its baseline, its exclusion list and `list-fixed-waits.py` are removed. Clippy's `too_many_lines` and `disallowed_methods` (for example `std::env::temp_dir` in tests) and rustfmt carry what is worth keeping. Review carries the rest. | The ratchet measured sizes and produced work that moved code. A maintainer already knows clippy. |
| 12 | **ADRs 0066 and 0067 are accepted and built under these rules**, not under Part IX's per-card budgets. ADRs 0008, 0058, 0059, 0061, 0062 and 0063 are unaffected. ADR 0050 loses only its `legacy-docket` clause. ADR 0064 keeps nextest, feature resolution, the dev profile and binary grouping, and loses its CI-shape and fixed-wait decisions to decisions 7, 9 and 11. | One set of rules. The alignment table below is exact. |

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail; nothing above depends on anything below.

---

- **Status:** accepted 2026-09-18 — recorded as a dated amendment at the bottom of this file, which also reorders the stages.
- **Date:** 2026-09-14
- **Supersedes:** ADR 0060 (the Docket bridge stays maintained), ADR 0065 (Docket pipeline
  dispatch trigger), ADR 0064 decisions 5 and 6, ADR 0050's `legacy-docket` clause.
- **Wire contract:** `docs/contracts/runner-v1/` **changes**: the decision fixtures and routes
  are removed (decision 3). No shipped harness declares `decisions`, so no runner in the
  field sends them. The pin table in `crates/tack-orch/tests/runner_contract.rs` changes in
  the same commit.
- **Roadmap:** Phase 64, `docs/book/src/roadmap.md`.

## ADR alignment

| ADR | Status before | After this ADR |
|---|---|---|
| 0008 migration rebuild recovery | accepted | unchanged; the retirement migrations follow it |
| 0050 Tack schedules, the runner executes | accepted | unchanged except "Docket may exist only as `legacy-docket`", which becomes "Docket exists only as a harness (ADR 0066)" |
| 0058 standalone single binary | accepted | unchanged |
| 0059 single-operator identity | accepted | unchanged |
| 0060 Docket control plane stays a maintained bridge | accepted | **superseded** by decision 2 |
| 0061 provider credentials at the runner boundary | accepted | unchanged |
| 0062 desktop app and background service | accepted | unchanged; the desktop job moves to the merge tier |
| 0063 harness credential modes | accepted | unchanged (decision 8 was already superseded by 0067) |
| 0064 test architecture | accepted | decisions 1–4 and 7 stand; **5 and 6 superseded** by decisions 7, 9 and 11; `docs/adr/0064-fixed-waits.txt` removed |
| 0065 Docket pipeline dispatch trigger | accepted | **superseded** by decision 2; `TACK_ORCH_DISPATCH_TOKEN` and `tack orch dispatch` are removed with the bridge |
| 0066 docket as a third harness | proposed | **accepted 2026-09-14**; its decision 2 ("does not touch the bridge") now reads "the bridge no longer exists"; built under decisions 5–11 |
| 0067 opencode readmitted | proposed | **accepted 2026-09-14**; built under decisions 5–11 |

## Full reasoning

*(Detail for whoever implements this or wants to check one call. Every number carries the
command that produced it, measured on `develop` at `a52a229`/`165c102`, 2026-09-14. Re-run
before quoting.)*

### What Part IX changed, measured

`python3 scripts/maintainability.py measure --totals`, run against `f95fbc2` (the tree
before Part IX, with today's script copied in) and against `develop`:

| | Before Part IX | Today | Change |
|---|---|---|---|
| Production code (no comments, no tests) | 44 079 | 44 495 | **+416** |
| Production comments | 13 374 | 10 468 | −2 906 |
| Test lines | 74 014 | 69 610 | −4 404 (−6 %) |
| Tests | 1 498 | 1 427 | −71 |
| Fixed sleeps in tests | 74 | 3 | −71 |
| Test lines inside production files | 20 882 | 0 | moved |

Its plan said so at the top: "the code is fine; what surrounds it is not", and it excluded
"any change to behaviour" and "any abstraction beyond the two named". Budgets on file and
function size are met by moving and splitting, so that is what happened. This ADR measures
the product surface instead.

### Where the tests are

The same `measure --json`, grouped by crate. The ratio is test lines per production-code
line:

| Crate | Production code | Test lines | Ratio |
|---|---|---|---|
| tack-orch | 3 744 | 11 845 | **3.16** |
| tack-runner | 7 638 | 12 876 | 1.69 |
| tack-api | 17 225 | 28 395 | 1.65 |
| tack-db | 8 711 | 10 405 | 1.19 |
| tack-cli | 4 355 | 4 222 | 0.97 |
| tack-desktop | 940 | 863 | 0.92 |
| tack-core | 1 755 | 1 004 | 0.57 |

About 44 000 of the 69 610 test lines exercise agent execution and orchestration. That
counts `tack-runner`, `tack-orch`, and `tack-api`'s `orchestration`, `runner_protocol`,
`security` and `wave2_gate`, plus `tack-db`'s execution repository. The frontend has
37 370 source lines, 13 924 unit-test lines (91 files) and 16 Playwright specs (4 479 lines):
`git ls-files` piped through `wc -l`.

`git grep -c "stale_lease\|StaleLease" -- 'crates/*/tests/**' 'crates/**/tests.rs'` finds
the stale-lease invariant in **12** test files across five crates: repository,
router, contract, wave gate, chaos, runner transport, runner engine, observability, CLI
client. Decision 5 keeps the repository test and one HTTP test.

### Coverage, measured

`cargo llvm-cov -p <crate> --summary-only` (what CI runs), the TOTAL line's line column:
core 83.08 %, db 69.89 %, api 69.28 %, orch 91.17 %, runner 91.55 %. The same measurement
with `-p tack-core -p tack-db -p tack-api` in one run: core **95.74 %**, db **85.57 %**, api
69.28 %. Core and db are well tested; the per-crate floors (85/70/70) fail because each
crate is measured against only its own tests. Uncovered production lines in api, largest
first: `handlers/runner_protocol.rs` 513, `handlers/executions.rs` 365,
`handlers/runner_admin.rs` 350, `handlers/decisions.rs` 250, `handlers/boards_multi.rs` 193
(15 % covered), `handlers/templates.rs` 181, `handlers/custom_fields.rs` 79 (15 %),
`handlers/attachments.rs` 58 (**0 %**).

The job fails today on pull request #56 for that reason, and separately on RUSTSEC-2026-0285
(rustls) in the dependency-policy and audit jobs: `gh run view <id> --log-failed`.

Why the numbers dropped during Part IX: `cargo-llvm-cov` 0.9.0 does not report
`src/**/tests.rs` files, but counted `#[cfg(test)] mod tests` code inline in a production
file as covered production code. Moving those modules out removed lines that were covered
only by being themselves; no test lost coverage.

### The legacy Docket surface, re-measured

ADR 0060 measured this on 2026-08-31. Today, with `cat <files> | wc -l`:

| Where | Files | Lines |
|---|---|---|
| `tack-orch` production | `adapters/*.rs`, `lib.rs`, `reconciler.rs` | 3 432 |
| `tack-api` production | `handlers/orch.rs`, `handlers/provisioning.rs`, `handlers/economics.rs`, `dispatcher.rs`, `orch_store.rs`, `orch_runtime.rs`, `sprint_dispatch.rs` | 6 156 |
| Frontend | `features/{fleet (not runnerFleet), approvals, economics, provisioning}`, `features/settings/{orchestration, orchestrationSettings}` | 7 323 |
| `tack-api` tests | `tests/orchestration/**` | 7 957 |
| `tack-orch` tests | `docket_*` tests, `reconciler/tests.rs` | ~5 000 |
| `tack-db` tests | files matching `orch` | 2 755 |
| Schema | `control_planes` + 9 `orch_*` tables | 10 tables |

Phase 0 of the plan re-measures this exactly. Some files in `tests/orchestration` may cover
runner-v1 fleet templates, and those stay.

ADR 0060 rejected deletion for three reasons:

- **Guardrails and DAG-ordered sprint dispatch had no runner-v1 equivalent.** ADR 0066 moves
  guardrails into docket's own per-tool policy engine, where the tool call happens.
  DAG-ordered dispatch is re-implemented on runner-v1 scheduling only if the plan's
  measurement finds a user of it. Otherwise it is dropped and the release notes say so.
- **Installed databases might hold rows.** Handled by one migration per `DROP TABLE`
  (the one-statement-per-migration rule), preceded by a JSON export of those tables into
  the pre-upgrade snapshot the migration runner already writes.
- **234 `TODO.md` citations blocked a clean delete.** They are gone:
  `grep -rn "TODO\.md" crates --include='*.rs' | wc -l` returns 0.

### What exists only for the agent process

`git ls-files <path> | xargs cat | wc -l`:

| Item | Size | Fate |
|---|---|---|
| `.claude/` (6 skills, context/token/reporting/decision/scope contracts) | 11 files, 1 249 lines | leaves the repository; the two rules worth keeping (plain-language decisions, no mechanism without a caller) move into `CONTRIBUTING.md` |
| `CLAUDE.md` | 222 lines | ≤ 40 lines, pointing at `CONTRIBUTING.md` and `docs/TESTING.md` |
| `TODO.md` boards | 5 029 lines | replaced by the roadmap plus GitHub issues; the live part is archived to `docs/closed-cycles/boards/` |
| `docs/agent-handoffs/` | 69 files, 15 644 lines | frozen into `docs/closed-cycles/handoffs/`; no new ones |
| `docs/closed-cycles/` | 215 files, 69 081 lines | kept, frozen, excluded from every check |
| `docs/plans/` | 4 files, 2 094 lines | archived once their phase lands; `harnesses.md` is rewritten against this ADR when carded |
| `scripts/maintainability.py`, baseline, `list-fixed-waits.py`, `docs/adr/0064-fixed-waits.txt` | 605 lines + generated | removed (decision 11) |
| `scripts/check-comments.sh` | 165 lines | reduced to the one rule a person needs (a cited file must exist), or replaced by review |
| `scripts/check-test-hygiene.sh` | 52 lines | replaced by a clippy `disallowed_methods` entry |
| `crates/tack-api/tests/wave2_gate.rs` | 1 069 lines | deleted; each real claim it holds moves to its owning layer first |
| Agent cost, last session | integrator 6 866 calls; 65 subagents 17 462 calls; 5.9 B cache-read tokens | the reason decision 4 exists |

The ADRs stay. They are the one artifact of the agent process a maintainer reads.

### CI today

Measured with `gh run view <id> --json jobs` on the latest `develop` pull-request run:
Embed SPA 7.2 min, E2E cross-browser 6.9 min, Rust 4.3 min, MSRV 1.7 min, Desktop 1.3 min,
the rest under a minute. The Coverage job runs five instrumented builds that share nothing
with the Rust job's build. The `main` ruleset requires all 10 jobs, a pull request and
linear history: `gh api repos/:owner/:repo/rules/branches/main`. Decision 9 changes the
required checks to the pull-request tier plus the merge-tier jobs that must pass before a
release.

### Patch coverage, concretely

`cargo llvm-cov nextest --workspace --lcov --output-path lcov.info` in the pull-request run.
A step then compares `lcov.info` with the pull request's diff and fails under 80 %: the
`diff-cover` tool, or an equivalent script of about 30 lines. The report is uploaded as an
artifact. No external coverage service is required.

### Considered and not adopted

- **Raising the floors by adding tests to `tack-core` and `tack-db`.** Their behaviour is
  already covered by the suite; adding tests to satisfy an isolated measurement is the
  work this ADR stops.
- **Branch coverage.** `llvm-cov` branch coverage needs a nightly toolchain; not worth a
  second toolchain in CI.
- **A gate on mutation score.** Report-only until a baseline exists; a gate would recreate
  the ratchet.
- **Keeping the Docket bridge behind a cargo feature.** ADR 0060 already costed it: it hides
  too little and risks silently dropping Docket from release builds.
- **Splitting `tack-api` into crates.** ADR 0064's reasoning still holds, and removing the
  bridge shrinks the crate anyway.

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-09-18 — ACCEPTED, with the stages reordered.** The user directed the work this ADR describes to begin, with two corrections that came out of re-auditing the harness integration against docket's own repository. The twelve decisions stand as written. What changes is the order in Phase 64, one deletion that becomes a decision, and one rule that was implicit.

1. **docket's harness contract already exists.** `docket harness run` and `status`, with a versioned schema and fixtures, shipped in docket `0.2.0-beta.2` on 2026-09-12 (`git -C ../rack-cli log --oneline -- src/docket/core/harness.py`). Stage 6's precondition ("once its upstream contract exists") is met, so nothing about the harnesses has to wait.
2. **The harnesses move ahead of the bridge retirement.** Retiring the control plane (old Stage 4) before docket is reachable as a harness (old Stage 6) would leave docket with no integration at all in between. The new order is in `docs/book/src/roadmap.md`: measure; CI and the agent scaffolding; **harnesses on one core**; mechanisms with no caller; retire the bridge; rebuild the test suite. `docs/plans/harnesses.md` is that stage's plan.
3. **The harness seam was redesigned, which Part IX could not do.** IX-M5 extracted a shared core under a no-behaviour-change rule, so each historical difference between the two adapters became a trait hook: 14 of them, with `codex.rs` at 886 lines and `claude_code.rs` at 1 141. The seam is now a `HarnessDescriptor` plus a four-method `HarnessGrammar` — 189 and 305 lines (`wc -l crates/tack-runner/src/harness/{codex,claude_code}.rs`) — and harness unit tests fell from 3 240 lines to 1 554 by proving the lifecycle once, in the core, as decision 5 of this ADR asks. Behaviour was unified on purpose where the two adapters disagreed by accident: one handle format, one cancellation policy (a failed signal is `Ambiguous` evidence), reconciliation always checks that a live pid is still the harness program, run logs are always staged outside the workspace, the runner's configured process limits apply to every harness, and an injected provider credential is always registered for redaction — it was not, in either adapter.
4. **`decisions` (decision 3) is removed only after the docket `decisions` question is answered.** docket is the one harness that could ever enter a decision state — it has an approval store, and its harness mode today refuses approvals rather than pausing. ADR 0066 already requires a separate ADR to route a mid-run approval to a Tack operator. If that ADR is wanted, `decisions` has its first caller and stays; if it is not, `decisions` goes as decision 3 says. Removing a byte-pinned protocol path and then rebuilding it would be the worst of the three outcomes, so Stage 4 deletes `model_profiles` unconditionally and `decisions` only once this is decided.
5. **A policy one harness ignores is declared, not silent.** `codex.rs` never read a request's `permission_policy` or `budgets`; `claude_code.rs` did. Every grammar's capability table now carries a `permission_policy` entry (claude-code `advisory`, codex `unsupported`), and a request a harness cannot honour at all is rejected before spawn. This is decision 5's "verified behaviour" applied to the request policy.
6. **VIII-C3 is closed as obsolete.** It corrects one unit test of `DocketAdapter::dispatch`, which the bridge retirement deletes.

**2026-09-18, later the same day — two questions this ADR left open are answered by the user.**

1. **`decisions` stays, and is finished rather than removed.** This replaces decision 3's second half and point 4 above. The premise that only docket could ever ask was wrong: these CLIs are built to stop and ask before they act, and Tack runs them with asking switched off. That is checked for claude-code (Tack passes `--permission-mode bypassPermissions`; `claude --help` lists `manual` and `--permission-prompts host`) and is to be measured for each of the other three in its own change. The board half is already built. The runner half is built once, in the harness core, so that a harness uses it when its CLI offers a way and declares `decisions: unsupported` until then; whoever dispatches a run chooses `auto` or `ask`. Tasks D1 and D2 in `docs/plans/harnesses.md` and `docs/plans/phase-64.md`.
2. **DAG-ordered sprint dispatch goes with the bridge** and is named in the release notes. It comes back on runner-v1 scheduling only if use of the released product asks for it.

Work targets `develop`; the ruleset on `main` is left as it is until `develop` is next released.

Rules in force from this date, replacing Part IX's process: decisions are recorded in ADRs and intent in the roadmap; no new card boards, dispatch plans or per-card handoffs are written; `CLAUDE.md` says the same. `scripts/maintainability.py`, `check-comments.sh` and `check-test-hygiene.sh` keep running in `pre-push` until Stage 2 replaces them, because a gate that exists and is ignored is worse than either.
