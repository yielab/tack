# Plan: Phase 70 — what is still open before the re-cut

**Status: opened 2026-10-12, after Phase 69 closed. No ADR: every task here is a follow-up an
earlier phase found and wrote down, and the decisions it needs are in "Decided" below.**
Written against `develop` at `1800b139`. Re-run `rg -n` before trusting a line number.

What this phase builds, in one paragraph. A request no runner can claim stops sitting in Queued
with no word: the card says why it waits, and after half an hour of nobody being able to take it,
it fails with that reason. The book's "Known gaps" stop listing what Phase 69 already closed, and
the one that may still be true (a docket run that times out) gets a test that settles it. `tack
runner doctor` says, per harness, what it can keep and whether it can ask. A stored run setting
that no longer fits says so in the log. The embedded runner's id is tested against the real
enrolment, not a fake. The real-`bwrap` boundary test stops skipping silently in CI. The Automation
panel offers "Through the gateway" when a gateway key is set.

## Where each item comes from

| Task | Found by | Source |
|---|---|---|
| Q1, Q2 | Phase 68 D4 | `docs/plans/phase-68.md` follow-ups, "A request no runner can ever claim stays Queued forever"; `crates/tack-orch/src/scheduler/wiring.rs:222-231` maps `NoEligibleRunner` to `None` and drops its `reasons` |
| K1 | Phase 66 | `docs/book/src/user-guide/agent-runners.md` Known gaps; `crates/tack-runner/src/harness/process.rs:515-518` already kills announced groups on timeout in the question-driven path, so the gap may be stale |
| K2 | Phase 66 | same Known gaps; `crates/tack-cli/src/doctor.rs:174-179` prints `artifacts` once, runner-wide, and never `permission_policy` |
| K3 | Phase 68 K2, D4 | `crates/tack-db/src/repo/items.rs` row conversion reads `run_settings` with `.ok()`; `crates/tack-cli/src/local_runner.rs:422` has no test against a real enrolment |
| C4 | Phase 69 Status | `crates/tack-runner/tests/agent_boundaries.rs:556-561` prints "skipped" and passes when `bwrap` is absent; `.github/workflows/ci.yml:543-562` never installs it |
| G1 | Phase 68 D1 | `frontend/src/features/settings/panels/AutomationPanel.tsx` has no gateway choice |
| D (integrator) | this inventory | ADRs 0075, 0076, 0077 still read PROPOSED; `roadmap.md` says Phase 69 dispatched no round; Known gaps lists the branch push, which Phase 69 R2/R3 closed |

## Decided — not re-opened by any task

| Question | Decision |
|---|---|
| Which reason does a waiting request show? | The latest one the scheduler computed for it, from whichever runner last polled; a reason is a plain sentence ("codex needs a model named"), never the enum's name. Selecting the request clears it. |
| A request no runner polls at all | Reads "No runner is online" at read time, from runner presence. It never fails for that alone: starting the runner later is normal. |
| When does a waiting request fail? | When it has been continuously ineligible for every runner that polled for 30 minutes (a server setting beside the existing scheduler settings; `0` disables). It fails with the terminal code `no_runner_can_claim` and the last reason as its message. |
| Where do the fields live? | On the request row (a migration adds them); the API returns them on the request and on the item's latest run, so the card needs no second call. |
| The book's Known gaps | The integrator removes what is closed and keeps what K1 proves still open. |
| Timelines for codex, claude-code, opencode | Parked: they need tool lines captured from the real binaries, and no agent runs a real harness. |

## The waves

Two agents building cargo at once (the measured limit); G1 builds no cargo and runs beside them.
A round's tasks share no source file.

| Round | Tasks | Needs |
|---|---|---|
| 1 | Q1, K1, K2, K3, C4, G1 | nothing |
| 2 | Q2 | Q1 (the regenerated schema is the hand-off) |
| — | D | every task merged; the integrator's |

## How a task is handed to an agent

Exactly as `docs/plans/phase-67.md` § "How a task is handed to an agent", with `phase-70.md` in
the first line of the brief, plus the three lines Phase 69 added:

```
Never weaken a check to make a test pass: a boundary test that fails is the finding.
Never run a real harness binary; every harness in a test is the fake under tests/common.
A rule you add gets its test in trust_boundary.rs (server) or agent_boundaries.rs (runner), named after the rule.
```

## The tasks

### Q1 — a request no runner can claim says why, and fails after half an hour

**Files:** a new migration in `crates/tack-db/migrations/`; the execution-request repository in
`crates/tack-db/src/repo/execution.rs`; `crates/tack-orch/src/scheduler/wiring.rs`; the server setting beside the scheduler's existing
ones; the request and item-run response types in `crates/tack-api/src/`; the regenerated
`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` (via `./scripts/regen-generated.sh`);
tests beside the files they test.
**Do:** add `waiting_code`, `waiting_reason`, `waiting_since` (nullable) to the request row. When
a claim poll leaves a queued request unselected, write the reason from `NoEligibleRunner.reasons`
or `UnknownRunner` as a plain sentence and its code, and set `waiting_since` if it was null; when
the request is selected, clear all three. There is no sweep that expires requests
(`execution_retention.rs` only purges), so the claim path decides: a request it finds ineligible
whose `waiting_since` is older than the setting is failed there (default 30 minutes, `0` = never) with terminal code `no_runner_can_claim` and the
reason as message, through the same path any other request failure takes, so its events and the
item's status follow. A request with no `waiting_since` (no runner ever polled) is not failed.
Return `waiting_reason` and `waiting_since` on the request and on the item's latest run.
**Test (RED first):** a request needing a model no runner offers, polled by one runner: the row
carries the sentence and a `waiting_since`; the same request later selected has all three cleared;
with the clock 31 minutes on, the next poll fails it with `no_runner_can_claim`; with the setting `0`,
it does not; a request no runner polled is never failed. One table test maps every
`IneligibleReason` variant to a non-empty sentence that is not its `Debug` name.
**Done when:** those pass; `cargo nextest run --workspace -E 'package(tack-orch) + package(tack-db) + package(tack-api)'` green; the regenerated files staged; the gate green.
**Hand-off:** Sonnet, two layers (db + orch, with the API fields), ceiling 150.

### K1 — a docket run that times out stops every group it announced

**Files:** `crates/tack-runner/tests/crash_matrix.rs`; `crates/tack-runner/src/harness/process.rs`
only if the test fails.
**Do:** write the test first. It runs the fake harness with a request timeout of a few seconds,
announcing a detached group (`fake_harness.sh`'s `spawn_detached` with `TACK_FAKE_HARNESS_ANNOUNCE=1`),
and never exiting on its own. After the timeout, the announced group must be gone and the result
must say so (`ProcessGroups { tracked: 1, killed: 1, survived: 0 }`). Check both the
question-driven wait (`wait_with_capture_and_questions`) and the plain `wait_with_capture`, which
today returns `ProcessGroups::default()` (`process.rs:325`): if docket can reach the plain path,
it must kill announced groups too. If the test passes against today's code, it stays as the proof
and the report says the gap is closed.
**Test (RED first):** `timeout_stops_every_group_the_harness_announced` in `crash_matrix.rs`, one
case per wait path.
**Done when:** the test passes; `cargo nextest run --workspace -E 'binary(crash_matrix)'` green
three times under `--test-threads 4`; the gate green. The report says, in one line, whether code
changed.
**Hand-off:** Sonnet, one layer, ceiling 100.

### K2 — `tack runner doctor` says, per harness, what it keeps and whether it can ask

**Files:** `crates/tack-cli/src/doctor.rs` and its tests.
**Do:** under each harness's block, print `artifacts:` and `permission policy:` from that harness's
own capability (the same values the probe sends the server), in the existing `render_feature`
style. Keep the runner-wide block, retitled so it no longer claims to "apply identically" to
features that differ per harness. A harness whose value is decided when the runner starts (docket)
says that, not a guess.
**Test (RED first):** the existing doctor output test grows rows: a harness with artifacts and a
policy prints both lines; one without prints `not supported` with its reason.
**Done when:** those pass; `cargo nextest run --workspace -E 'package(tack-cli)'` green; the gate
green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### K3 — a run setting that no longer fits is logged; the embedded runner's id is tested for real

**Files:** `crates/tack-db/src/repo/items.rs` (the row conversion that reads `run_settings`);
`crates/tack-cli/src/local_runner.rs` (tests only) or a new test file under `crates/tack-cli/tests/`.
**Do:** where the row conversion turns stored `run_settings` into `RunSettings` with `.ok()`, keep
the `None` but log at `warn` with the item id and the serde error (never the stored value). Add a
test that starts the embedded runner against a loopback API (the pattern the existing
`crates/tack-cli/tests/` enrolment tests use), enrols it, and asserts
`EmbeddedRunnerControl::runner_id()` returns the `runr_` id the API assigned, not the configured name.
**Test (RED first):** a stored `run_settings` of `{"timeout_seconds": "ten"}` reads back `None`
and the captured log has one `warn` naming the item; the enrolment test above.
**Done when:** those pass; `cargo nextest run --workspace -E 'package(tack-db) + package(tack-cli)'`
green; the gate green.
**Hand-off:** Sonnet, two small files in two crates, ceiling 100.

### C4 — the real-`bwrap` boundary test runs in CI, or fails

**Files:** `.github/workflows/ci.yml` (the `security-boundaries` job only);
`crates/tack-runner/tests/agent_boundaries.rs` (`sandbox_hides_the_state_dir_from_a_real_bwrap_run` only).
**Do:** in the job, before the nextest step, `sudo apt-get install -y --no-install-recommends bubblewrap`
and `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` (Ubuntu 24.04 restricts
unprivileged user namespaces, which `bwrap` needs), and set `TACK_REQUIRE_BWRAP=1` on the nextest
step. In the test, when `bwrap` is not detected and `TACK_REQUIRE_BWRAP` is `1`, `panic!` with
"bwrap required by TACK_REQUIRE_BWRAP but not on PATH"; otherwise keep today's skip. Pin nothing
new: `apt-get` packages are not pinned elsewhere in `ci.yml`.
**Test (RED first):** run the test locally with `TACK_REQUIRE_BWRAP=1 PATH=/usr/bin:/bin` minus
`bwrap` (a scratch `PATH` without it) and see it fail; with `bwrap` present it passes.
**Done when:** that, and `cargo nextest run --workspace -E 'binary(agent_boundaries)'` green; the
gate green. CI is verified by the integrator after the push.
**Hand-off:** Haiku, ceiling 60.

### G1 — "Through the gateway" in Automation, when a gateway key is set

**Files:** `frontend/src/features/settings/panels/AutomationPanel.tsx` and its test
(`panels.test.tsx`); read-only: `frontend/src/features/agents/api.ts`, the run dialog under
`frontend/src/shared/runWithAgent/`.
**Do:** the run dialog already lets a run go through a configured provider endpoint (`model_provider`
in `RunSettings`). Offer the same choice in the project's Automation panel, beside the agent's own
login, only when the runner reports a gateway key (the same check the Agents page's provider panel
uses); without one, show it disabled with its reason ("Add a gateway key on the Agents page"),
never hidden. The stored value is exactly what the dialog would send. If the panel cannot know
whether a key is set without a new API field, stop: that is a finding.
**Test (RED first):** in `panels.test.tsx`: with a key reported, the choice is enabled and saving
stores the dialog's value; without one, it is disabled and names its reason.
**Done when:** those pass; `npx vitest run src/features/settings` green; `npm run typecheck` green;
the gate green.
**Hand-off:** Sonnet, one layer (frontend), ceiling 100.

### Q2 — the card and the run page say why a request waits

**Files:** the board card's run status and the run page's header under `frontend/src/` (find them
with `rg -n "Queued" frontend/src --glob '!*.gen.ts'`), and their tests.
**Do:** where a queued run is shown, show `Waiting: <waiting_reason>` and how long it has waited
(from `waiting_since`); with no reason and no runner online, "Waiting: no runner is online". A run
that failed with `no_runner_can_claim` shows the reason as its failure, in the existing failure style.
**Test (RED first):** the card and the run page each render the three cases.
**Done when:** those pass; vitest and typecheck green; the gate green.
**Hand-off:** Sonnet, one layer (frontend), ceiling 100.

### D — the integrator's

After every task merges: ADRs 0075, 0076 and 0077 read ACCEPTED with the date and the phase that
executed them; `roadmap.md` says Phase 69 closed and adds Phase 70; the book's Known gaps lose the
branch push (Phase 69 R2/R3) and, if K1 proved it, the docket timeout, and the doctor line if K2
closed it; `CHANGELOG.md`'s `[0.1.0-beta.10]` gains this phase's lines; `CLAUDE.md`'s pointer
names Phase 70.

## Parked — not scheduled, and why

| Item | Why |
|---|---|
| codex, claude-code and opencode on the timeline | needs tool lines captured from the real binaries; no agent runs a real harness |
| docket: a recipe run's token budget, a recipe's verify command, Tack's tool names in docket | waits on docket itself |
| Phase 66, 67, 68 and 69's own Parked tables | their triggers have not happened |

## Status

Opened 2026-10-12. Each merge is a `--no-ff` into `develop` with the gate on the merge.
