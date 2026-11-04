# ADR 0037: what docket offers, Tack uses or says why not

**Status: ACCEPTED 2026-11-04.** Executed by Phase 91 (`docs/plans/phase-91.md`). Builds on ADR 0035 (the
inbox), ADR 0036 (one owner per concept; a team declares what it accepts) and ADR 0032 (any agent first).

**The ask.** docket 0.3.0-beta.1 lists 21 features. Tack uses 16 of them. Five are not used: a task that
converses turn by turn (`conversation`), a second, read-only token (`readScope`), what a team has learnt
and what it waits to learn (`podMemory`, `memoryStatus`), and docket's own check of a named method
(`pipelineChecked`). Each is decided here: used, and how; or not used, and why. And Tack names a docket
version that was never released.

**The rule behind every decision.** A docket feature is adopted when it gives a person something they
cannot get today, or moves a check to the one who owns the fact. It is probed, never assumed (ADR 0036);
without it Tack does what it does today. What docket offers only through its own command line is shown
with the command, never hidden, until docket offers it over HTTP.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **A thread on a docket team continues instead of replaying.** When the team lists `conversation`, a thread's first turn is one docket task with `conversation: {maxTurns}` (the thread's turns, at most docket's 20); every later turn answers that task's `turn` escalation with `reply`, and closing the thread answers `close`. The runner reports `converse: continues` for that team. A team whose method has more than one step answers `conversation_needs_one_step`; that turn is sent again as today's replay and the team is reported `replays` with docket's reason. Without the feature, nothing changes | A conversation with a docket team keeps its context between messages, as one with Claude Code already does, and costs one task, not one per message |
| 2 | **What a team waits to learn needs you; what it learnt is on its sheet.** When the team lists `memoryStatus`, the runner reads `GET /pods/<p>/memory?status=pending` and `?status=accepted` with the team's report. A pending entry opens a **needs-you** notification (key `memory:<team>:<id>`, kind `memory`) that closes when the entry is no longer pending, however it was decided. The team's sheet shows what it learnt and what waits. docket decides an entry only through `docket pod memory approve` and `refuse`: the notification and the sheet give both commands to copy, and **Approve** and **Refuse** are shown disabled with that reason. Tack asks docket for an HTTP route to decide an entry, probed as a feature; when listed, the two buttons work | A team that wants to remember something asks the person in Tack's inbox, the same day, and the person sees what each team has learnt |
| 3 | **docket checks a named method; Tack reads the answer.** When the team lists `pipelineChecked`, docket owns the check that a method's steps have their roles: Tack's refusal before sending reads docket's own `methods[].missingRoles` (a fact docket reports, not a second rule), and a task docket refuses at claim (`failureKind: pipeline`) reads **Not started** with docket's reason, as Tack's own refusal does. Without the feature (an older docket, which would skip the step silently), Tack's refusal stays the only one | A method that cannot be staffed is refused once, with one sentence, whoever noticed |
| 4 | **The read-only token is not used.** Every docket call goes through the runner, which must hold the full token to start tasks; a second token in the same process limits nothing. It is adopted when a process that only reads docket exists (none does) | Nothing |
| 5 | **A question in parts is proved, not only parsed.** Tack already shows each part of a multi-part consult (`consultParts`) as its own question. The fake docket answers it as docket does (the task waits until every part is answered; answering the last reopens it), so the test answers part 1, sees the run still waiting, answers part 2 and sees it continue | Nothing new; a regression would now fail a test |
| 6 | **Tack names docket's next version, 0.3.0-beta.1.** The messages, the doctor and the guides said "docket 0.2.0-beta.6", a version that was never tagged; the last release, 0.2.0-beta.4, does not speak API 6. Tack needs API 6 and the escalation record, which docket's declared next release, 0.3.0-beta.1, carries | "needs docket 0.3.0-beta.1" names a version that exists in docket's tree and is its next release |

**Why now.** docket closed its Phases 62 to 64 on 2026-11-04 and the live check (`docket-prill-e2e.sh
--check`) passes against it; what is left is what Tack chose not to read.

**If you do nothing.** A docket thread keeps paying one task per message and losing its context; a team's
memory waits on a person who is never told; the same method is refused by two rules with two sentences;
and the doctor names a version nobody can install.

## Asked of docket

- `POST /pods/<p>/memory/<id>` with `{"decision": "approve" | "refuse", "reason"?}`, answering the entry as
  `GET /pods/<p>/memory` lists it, announced as a feature (proposed name `memoryDecide`). Additive to API 6.

## Evidence (read 2026-11-04, docket `1d180651`, Tack `8d8c4973`)

| Fact | Where |
|---|---|
| The 21 features | `rack-cli/src/docket/interfaces/http/routes/health.py` `FEATURES` |
| `conversation: {maxTurns}`, one step only (`conversation_needs_one_step`), `turns` computed on read, an escalation of kind `turn` with `reply {text}` and `close` | `rack-cli/specs/api/operator-api.spec.md:111`; `specs/functional/operator-loop.spec.md:727` |
| Tack's docket adapter reports `converse: replays` and says docket's own continuing task "is not used yet"; `ConverseMode::Continues` exists for other harnesses | `crates/tack-runner/src/harness/docket.rs:301`; `crates/tack-protocol/src/conversation.rs` |
| `GET /pods/<p>/memory?status=accepted\|pending\|refused`; entries `{id, kind, subject, note, status, reason, provenance, stale, evidence}`; deciding is `docket pod memory approve/refuse`, no HTTP route | `operator-api.spec.md:82,203,395-403`; `operator-loop.spec.md:233-235` |
| `pipelineChecked`: a step whose role the roster lacks refuses at claim with `failureKind: pipeline` | `operator-api.spec.md:313-317` |
| `DOCKET_SERVE_READ_TOKEN`: `GET`/`HEAD` only, other methods `403 read_scope` | `specs/data/serve-read-api.spec.md:44-47` |
| `consultParts`: `part {index, count}`, task `waiting_input` until every part is settled | `operator-api.spec.md:578-584` |
| Last docket tag `v0.2.0-beta.4` (2026-10-06); `VERSION` 0.3.0-beta.1, declared 2026-10-30, untagged | `git -C rack-cli tag`; `rack-cli/ROADMAP.md:74-75` |

## Considered and not adopted

- **Reading pod memory without `memoryStatus`.** Without the status query only accepted entries are
  served; the part that needs a person is missing, so the feature is probed as a whole.
- **Approving a memory entry by running docket's command from the runner.** The runner would act for the
  person on docket's machine with a command Tack composed; the person approves in docket's own tool, or
  in Tack once docket offers the route.
- **Using docket's `GET /inbox` as the source of what needs a person.** Tack's inbox is fed by its own
  board facts (ADR 0035); docket's inbox would duplicate decisions Tack already shows.
