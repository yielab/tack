# ADR 0023: what the first real project found, and what Tack changes for it

**Status: ACCEPTED 2026-10-17.** Executed by Phase 74 (`docs/plans/phase-74.md`). Amends no
decision; it closes the holes the first real project fell into, inside ADR 0002 (leases and
recovery), ADR 0006 §5 (where the data lives) and ADR 0021 (teams and the ceiling).

**Decide:** that a run which cannot start **ends Failed with its reason**, and one nobody holds
any more **asks a person** instead of sitting in "Preparing"; that a runner's **name and
liveness have one source each**; that what a runner reports about its teams **stays current**,
and the ceiling is judged **per team**; that **everything Tack runs is logged by default**; that
the doctor reads **the same file** the runner reads; that a project file **says the auto-start
switch's state**; that the docket contract is proven by **captures of a real docket**, not by a
fake written from its spec; and that four screens use **the project's words and the one import
flow**.

**Why now:** on 2026-10-17 Meridian was put into Tack and docket end to end (86 tasks, ten roles,
three teams) on the maintainer's own board. The project, the roles, the profiles, the teams and
the resolution of a run's team all worked. Not one run reached a team: docket refused the first
request (docket's to fix, K7), and Tack swallowed the refusal. The three runs are still in
"Preparing", with no reason on any screen and none in any log. Twenty defects were written down
in Tack, with the evidence beside each one.

**If you do nothing:** any refusal from an agent before it starts leaves a run in "Preparing"
for ever, cancel included; the board's runner stops after its first restart on any machine where
the desktop app ran before; a dead runner blocks every run that names no machine; a team created
after the runner started does not exist for Tack; and the logs show none of it.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **A run that cannot start ends Failed with the reason.** Every error before the agent is seen running (the folder could not be prepared, the agent refused the run at validation or at start, the agent's service did not answer) goes through the one path that already reports a pre-start failure. Only a failure to reach the board itself stays an engine error. | "Failed: docket did not accept the team mer-dev: project is required." |
| 2 | **A run nobody holds asks a person.** When an attempt's lease has expired and nobody renewed it, the claim poll moves it to `needs_operator` with a sentence, exactly as a recovery that cannot prove the agent absent does (ADR 0002: lease expiry alone never starts another process). A cancel asked of a run in `needs_operator`, or of one whose lease has expired, ends it Cancelled: that is the person's decision. | "The computer that took this run stopped answering. Run it again or cancel it." Cancel works. |
| 3 | **A runner's name is the board's.** The row's name is set once, when the runner is provisioned or enrolled; a refresh updates capabilities and never the name. A refresh that fails for any reason other than a refused credential is retried, not answered by enrolling again. | The board's runner survives "Re-check" and every restart. |
| 4 | **One rule says a runner is online:** active, and a heartbeat within the scheduler's `max_heartbeat_age`. The scheduler, the run dialog's default machine, the waiting reason and the Agents page all read it; the server returns `online` on every runner it lists, and the page reads that instead of computing its own. | "Runs on this computer" picks the computer that is on; the desktop app's old row no longer counts. |
| 5 | **What a runner reports stays current, and the ceiling is per team.** The runner probes its agents again every five minutes while idle and after each attempt, and sends a refresh only when the report changed. The merge ceiling of a run is its own team's; a team no runner reports yet is "not reported yet", never the lowest ceiling of other teams on that machine. The Ceiling rung's level is the lowest of everything it lists, and the Teams rung shows the board's own verdict on each team. | A team created five minutes ago appears without a restart; "can reach L5" never sits above a ✗. |
| 6 | **Everything Tack runs is logged by default.** The default filter covers every `tack` crate (the runner, the orchestration, the CLI), not only the server. The server's first lines say the absolute database path and whether this build carries the web UI. When the database comes from the working-directory default and a database exists in the per-user data folder, it says so and names the setting. An unknown key in a runner's file is named (only the key, never a value). The working-directory defaults stay (ADR 0006 §5). | "This server uses ./tack.db; your desktop app's data is in ~/.local/share/tack (set TACK_DATABASE_URL)." |
| 7 | **The doctor reads what the runner reads.** `tack runner doctor --config <file>` loads through the same function as the runner and the board's embedded runner, falls back to the file the board was told about, and shows a broken file as an error instead of the defaults. | The doctor and the Agents page say the same thing about docket. |
| 8 | **A project file says the auto-start switch's state.** The export writes `enabled: false` when it is off, so the file brought back into a project turns it off instead of leaving it as it was. Turning it on from a file stays refused (ADR 0022 §4). One function builds the section for the export and for a template. | The exported file and the project agree. |
| 9 | **The docket contract is proven by captures.** Each request Tack sends and each answer it reads is recorded from a real docket started in a throwaway home with no model behind it, with a provenance file beside it, as for the other agents. The in-process fake answers from those files and refuses any key it does not know. Tack's `POST /pods` body stays the one docket's ADR 0027 and operator API 5 publish (`name`, `location`, `blueprint`, `ensure`); making docket accept it is docket's change (K7). Whichever way that is decided, the capture fails first, and the client and the capture change together, with no alias. | Nothing; a renamed field fails a test instead of a run. |
| 10 | **The screens use the project's words and one import.** The page title and the breadcrumb come from one table of top-level pages; the Overview names item types in the project's vocabulary; a settings tab follows the address; a new project can be created from a project file through the same preview and confirm the Data tab uses. | "Agents" on the Agents page; "Iniciativa", not "Epic". |

If you accept this table, you have accepted the ADR. Everything below is evidence.

---

## Evidence (read 2026-10-17 on `develop` at `d99503dd`, docket at its `0.2.0b6` install)

The full list, with screenshots and the JSON each row was read from, is kept beside the test run,
outside this repository; the defect ids below are its own.

| # | Defect | Where the cause is |
|---|---|---|
| 1 | T19: the three R0 runs stayed `preparing`, lease expired, no `started_at`; cancel answers "not yet terminal"; reconcile handles only `needs_operator` | `engine.rs` routes only a `validate` refusal to `fail_before_spawn`; `adapter.start(&spec).await?` and the provision step propagate. No server path closes an expired lease (`execution.rs` only counts them); cancel is settled only by a runner holding the lease |
| 2 | (same) | as above |
| 3 | T14: after the first start every restart of the board's runner leaves it `stopped`; a 500 swallowed the SQL error, then a 401 | provisioning names the row `local-<uuid>` (`runner_admin.rs`); refresh writes `name = config.runner_id` (`"local-runner"`) in both UPDATEs (`runner_protocol.rs`) and hits `UNIQUE(name)` held by the desktop app's runner; `transport.rs` enrolls again on any refresh error. Enrollment already ignores the name for this exact reason (`redeem_enrollment_token`) |
| 4 | T17: a run without a machine was refused "not exactly one active runner" while one of the two had not answered for hours. T4: the Agents page showed a docket version no probe writes any more | `default_selector` counts `state = 'active'` with no heartbeat check; `no_runner_online` and the scheduler use `max_heartbeat_age`; `findThisMachineRunner` takes the oldest `local-*` row |
| 5 | T13, T6: teams created after the runner started were absent; the ceiling was L3 because of another project's attended team. T10: "can reach L5" above six ✗. T15: the Teams rung called an attended team lights-out | capabilities are sent once, in `establish_session`; `ceiling_of` falls back to the machine-wide `autonomy_ceiling`; the settings ceiling ignores the other blockers; `Teams.tsx` decides lights-out from the violations alone, while the Ceiling rung renders the server's `caps_at` |
| 6 | T18: not one runner line in the board's log until `RUST_LOG` was set. T1: `tack serve` started from `~` migrated a stale `~/tack.db`. T7: a debug build has no UI and does not say so at start. T2: `[runner]` refused as "configuration is invalid" | `tracing_subscriber_for` lists `tack_api,tack_db,tack_core,tower_http`; the default database is `sqlite:tack.db` relative to the working directory (kept on purpose by ADR 0006 §5); the parser's error is dropped because its text can echo a secret |
| 7 | T3: the doctor said "docket: absent" while the live runner reported API 5 | the doctor has no `--config`, reads only `TACK_LOCAL_RUNNER_CONFIG` (set inside the server's own process) and turns a broken file into defaults |
| 8 | T8: an exported project with auto-start off has no `enabled` key | `export.rs` and `templates.rs` write `enabled: auto_start_enabled.then_some(true)` |
| 9 | T20: Tack's fake docket requires `name`, the real one requires `project`; both test suites were green | `fixtures/docket/v5/README.md`: "Nothing in this directory is a capture"; `fake_docket::post_pod` is written from the ADR; the e2e script sends the same body by `curl` and was never run on beta.6 |
| 10 | T5, T9, T11, T12 | `Layout.tsx` and `Breadcrumb.tsx` each fall back to "Projects"; `Dashboard.tsx` prints the raw type key, and `typeKey` exists three times; `ProjectSettings.tsx` reads `tab` once; the file import lives inline in `DataPanel.tsx` |

Not Tack's: K1, K2, K4–K7 are docket's and are triaged beside the test
(`docket-triage.md`); K3 (the sandbox is off) is the operator's choice. T16 (the apply button)
stays disabled with its reason: the board still has no channel to the runner (Phase 73's
roadmap gap); its sentence is corrected in decision 5's task.

## Alternatives not taken

- **Change the server's default folders to the per-user data folder.** ADR 0006 §5 rejected it
  because every container and unit relies on the working-directory defaults; nothing found here
  is new against that reason. The warning in decision 6 removes the silent part.
- **Send both `name` and `project` to docket until it decides.** An interim alias is what the
  "negotiated, not pinned" rule refuses; the capture makes the disagreement a failing test.
- **A server-side reaper loop.** The claim poll already runs on every runner's poll and already
  fails unclaimable requests; the expired-lease check is one more condition in it.
- **Requeue an expired lease automatically.** ADR 0002 forbids it without the runner's journal
  proving the agent absent.

## Questions for the maintainer, with the default built

Blocking: none.

1. docket's `POST /pods` (K7): docket reads `name`/`location`, as its ADR 0027 says. Built so:
   Tack does not change its body.
2. How long an expired lease waits before it asks a person: two lease durations (120 s).
3. How often an idle runner probes its agents again: five minutes.

## Amendment 2026-10-18: what Meridian's R0 found after the rounds

The maintainer chose both on 2026-10-18, inside this phase.

1. **A docket task that outlives the runner is picked up again.** A restart of `tack serve` left
   two docket tasks waiting on an approval in `needs_operator`, though docket still held them and
   their questions. With API 5 the task lives in docket's daemon, not in a child of the runner, so
   the reason ADR 0012 cut `resume` no longer holds. On recovery, when the harness sees the task
   still running and the journal kept the claim, the runner renews the lease with the journal's
   fencing token and follows the task to its end through the same path as a fresh run. If the
   lease is no longer its own, or anything is unproven, the attempt goes to `needs_operator` as
   today. Only docket resumes; every local-process harness stays `Unsupported`.
2. **The board makes its own decision token, and the desktop app hands it to its window.** A new
   `tack serve` had no `TACK_EXECUTION_DECISION_TOKEN`, so no question could be answered from the
   Inbox (403). The server now keeps a `decision-token` file (0600) beside `api-token`, created
   when missing, and the environment variable still wins. It stays a second secret. The desktop
   app reads it and gives it to its window, so nobody pastes it there. In a browser the Inbox asks
   once per session and says where the file is. The cost is that an agent running unjailed as the
   same user can read the file. That was already true of `api-token`, and a docket team's sandbox
   reads neither.

