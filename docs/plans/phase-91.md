# Phase 91: what docket offers, Tack uses or says why not

**Status: OPENED 2026-11-04.** Executes ADR 0037 (`docs/adr/0037-what-docket-offers-tack-uses-or-says-why-not.md`):
read its decision table first; this file says only how. Decision 4 (the read-only token) builds nothing.

## How a card is worked

- The rules of Phase 89's plan hold: any agent first, a docket feature probed (`client.has_feature(..)` in
  `crates/tack-runner/src/harness/docket/probe.rs`), the fake (`crates/tack-runner/tests/fake_docket/`)
  answers from captures under `crates/tack-runner/src/harness/fixtures/docket/v5/`, CI never needs docket.
- A capture is **real** when `scripts/capture-docket-fixtures.sh` or `scripts/compare-constructed-docket-fixtures.py`
  (docket's own scripted-model world, nothing spent) reaches it; a fixture marked `constructed` says in its
  provenance why it cannot be. `onboarding-lab/e2e/docket-prill-e2e.sh --check` must exit 0 at the end of
  every card.
- Run only the tests of what the card touched; the full suites and `.githooks/pre-push` run once per
  round, by the integrator. Each card writes its `[Unreleased]` line and its guide paragraph. No
  `Co-Authored-By`. `./scripts/regen-generated.sh` after any route or schema change.

## Cards

| Card | Crate(s) | What | Tests |
|---|---|---|---|
| **A** — a docket thread continues (d1), and a question in parts is proved (d5) | `tack-runner`, `tack-protocol` | When the team lists `conversation`: the first turn of a thread (`TurnNote` in the request's metadata, `turn == 1`) is enqueued with `conversation: {maxTurns: min(note.max_turns, 20)}`; a later turn finds the thread's docket task (kept by the runner from the first turn's request, keyed by the thread id) and answers its pending `turn` escalation with `optionId: reply`, `content: {text: note.message}`; the turn's reply is the hop's handoff, as today; ending a thread answers `close`. A `400` with `code: conversation_needs_one_step` sends that turn as today's replay and the team's `converse` reads `replays` with docket's reason; the capability reads `continues` for a team that lists the feature. The fake: the `conversation` field, the `turn` escalation after each hop short of `maxTurns`, `reply`/`close`, the 400; and a consult in two parts (`part {index, count}`, `waiting_input` until both are settled) | `tack-runner`: turn 1 enqueues with `conversation`; turn 2 answers the escalation and enqueues nothing; `close`; the 400 falls back to replay once and is reported; without the feature, replay as today. Two-part consult: answering part 1 leaves the run waiting, part 2 continues it |
| **B** — what a team learns needs you (d2) | `tack-protocol`, `tack-runner`, `tack-core`, `tack-app`, `tack-api`, `frontend` | `TeamReport.memory: Option<TeamMemory { pending: Vec<MemoryEntry>, accepted: Vec<MemoryEntry> }>` (`MemoryEntry` as docket lists it: `id, kind, subject, note, reason, stale`), read with the team's report when `memoryStatus` is listed. `tack-core/src/notification.rs`: `Kind::Memory` (**needs you**, key `memory:<team>:<id>`); the inbox reconcile (`tack-app/src/notifications/subscriber.rs`) opens one per pending entry from the latest team reports and closes those no longer pending. Agents → Teams sheet: "What it has learnt" (accepted, newest first, stale marked) and "What it waits to learn" (pending, each with `docket pod memory approve <id> --pod <team>` and `refuse … --reason "…"` to copy, and **Approve**/**Refuse** disabled with "docket decides this with its own command for now"); the notification links to the sheet. The ask to docket is ADR 0037's `memoryDecide`: probed, not built here | `tack-runner`: memory read with and without the feature, from captures. `tack-core`: `Kind::Memory` names, key. `tack-app`: reconcile opens one per pending entry, closes it when the entry leaves pending, nothing for a team that does not list the feature. Vitest: the two sections, the copyable commands, the disabled buttons with their reason |
| **C** — docket checks a named method (d3) | `tack-runner`, `tack-app` | Probe `pipelineChecked`. A task docket refuses at claim with `failureKind: pipeline` ends the run as never started with docket's reason (the same path `capability_missing` and `base_ref` take), so it reads **Not started** with that sentence. Tack's refusal before sending reads only the team's reported `methods[].missingRoles` (check that no copy of the roster rule remains in Tack) | `tack-runner`: a `pipeline` refusal from a real capture reads never started with docket's reason; with and without the feature, a method with a missing role is refused before sending only when the report says so |
| **D** — Tack names docket 0.3.0-beta.1 (d6) | all | Every "docket 0.2.0-beta.6" in code, tests, the doctor, the guides, `README.md`, `docs/CONFIG.md`, `ARCHITECTURE.md` and `scripts/` becomes "docket 0.3.0-beta.1"; the troubleshooting anchors follow | `git grep -n '0\.2\.0-beta\.6' -- ':!CHANGELOG.md' ':!docs/closed-cycles'` prints nothing; the doctor and probe tests read the new sentence |

Rounds: **D**, then **A ∥ B ∥ C**. A and B both touch `fake_docket` (A the task and escalation routes,
B the pod memory route) and `probe.rs` (one feature constant each); keep those edits additive. The
integrator runs `cargo nextest run --workspace`, `cd frontend && npm test`, `.githooks/pre-push` and
`--check`, then the live suite (`docket-prill-e2e.sh`, with `DOCKET_PRILL_PROVIDER=anthropic` for the
billed half) with a thread on a docket team and a pending memory entry.

## Not in this phase

The read-only token (decision 4); deciding a memory entry from Tack before docket offers the route;
docket's `GET /inbox`; editing a team's memory.
