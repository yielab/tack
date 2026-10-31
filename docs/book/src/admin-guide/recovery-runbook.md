# Recovery Runbook

This page is for the person who is called when a run stops answering. After reading it you can tell
what happened when a runner, its machine or the agent it started died in the middle of an attempt,
know whether Tack retried the attempt by itself, and put a stuck attempt back in the queue when it is
safe.

**The governing rule:** Tack does not
claim that an agent runs exactly once. The board can make sure two runners never hold the same attempt
at once, but a runner or network crash after a process has actually started
can leave it genuinely unclear whether that process is still running, and nothing on the board can prove
it is dead. The promise is narrower: at most one runner holds an attempt at a time, the runner
writes a local journal before it starts any process, repeated completion reports change nothing, and
an attempt goes to `needs_operator` whenever a safe retry cannot be proven. **A runner going quiet never,
by itself, starts a second process on the same workspace.**

---

## The lifecycle states involved

```text
leased ──▶ preparing ──▶ running ──▶ waiting_decision
   │            │            │              │
   └────────────┴────────────┴──────────────┘
                       │  (recovery service observes)
                       ▼
              lost  or  needs_operator
```

`leased` is the state of an attempt a runner has claimed but not yet started.

- **`lost`** — the recovery machinery is convinced no process was ever actually
  running for this attempt (it crashed or the runner restarted before spawning
  anything). Automatically safe to requeue.
- **`needs_operator`** — anything less certain than that. Not automatically
  retryable. Requires an explicit, audited operator decision.

An attempt that was claimed, preparing, running or waiting for a decision can move to either state,
and only the **recovery service** may move it: not the scheduler, and not a missed heartbeat on its
own. There is no background sweep that turns a stale claim into `lost` on a timer. Recovery is
**observation-driven**, described next.

---

## How recovery starts: the runner reports what it saw

When a `tack-runner` process restarts (after a crash, a host reboot, or an operator
kill), it reads its own local journal — written to `TACK_RUNNER_STATE_DIR` *before*
any agent process was ever allowed to start
— and reports one of three observations to
`POST /api/runner/v1/attempts/{id}/recovery-observation`:

| Observation | What it means |
|---|---|
| `process_stopped` | The runner found no live process for this attempt (checked by PID/process-group, not merely "the runner restarted") |
| `process_running` | The runner found the agent still running but could not take it back (see below) |
| `ambiguous` | The runner could not determine either way — e.g. the process table is inconclusive, or the journal itself was corrupt/incomplete |

An agent that runs in docket is the exception to that last row. docket keeps the task
in its own daemon, so it survives a restart of the runner. When the restarted runner finds
the task still running, and its journal kept the claim, the runner asks the board once
whether the attempt's lease is still its own. If it is, the runner follows the task to its
end as it would a new run, and no recovery observation is sent: a question the task was
waiting on is still on the board and is answered there, and the run ends as any other.
The attempt's time limit starts over at that moment, so the time before the restart is not
counted against it. If the lease is no longer the runner's, or anything cannot be proven,
the runner reports `process_running` and the attempt goes to an operator as described below.
Every other agent runs as a process of the runner, which a restart ends, so it always takes
the path below.

The server decides the disposition from the observation **and** the journal state it
already had on file for that attempt:

- **`safe_pre_spawn_requeue`** — granted only when the observation is
  `process_stopped`, the attempt's `started_at` was never set, and the last known
  journal state was `prepared` (the agent's process was never started in the
  first place). This is the one case narrow enough to resolve automatically: nothing
  ever ran, so there is nothing ambiguous to reconcile. The attempt moves to `lost`,
  the request moves back to `queued`, no operator involved.
- **`needs_operator`** — every other combination: `process_running` (a live process
  exists but the runner that owned it just changed identity across a restart — no
  safe way to hand it back into scheduling from here), `ambiguous`, or
  `process_stopped` after the process *had* actually started (it may have made
  side effects — a partial commit, a half-written file — that a blind requeue would
  duplicate).
- **`already_terminal`** — the attempt had already reached `succeeded`/`failed`/
  `cancelled` before the observation arrived; the observation is recorded for audit
  but changes nothing.

Recovery observations are idempotent, scoped by a caller-supplied `recovery_key`: a
second call with the byte-identical request replays the same response;
a second call with a *different* body under
the same key returns `409 idempotency_conflict` rather than silently overwriting the
first audit record.

---

## Resolving a `needs_operator` attempt

This is the only path back to `queued` for anything other than the narrow
`safe_pre_spawn_requeue` case. It requires a human decision and an audit trail:
there is no API shortcut that skips the reason field.

1. **Investigate.** Check the runner's machine directly: is the agent's process actually
   still running? Did it leave a partial workspace behind (look in the runner's state
   directory, `TACK_RUNNER_STATE_DIR`, under `workspaces/`)?
   Read the run's event timeline (`GET /api/executions/{id}/attempts/{n}/events`) for the last thing
   the runner reported before contact was lost.
2. **Decide and record why.** Once you're confident it's safe (the process is
   confirmed dead, or you've manually cleaned up any side effects), requeue with an
   explicit reason:

   ```sh
   tack execution reconcile <request-id> \
     --recovery-key "ops-2026-08-19-host-restart" \
     --reason "confirmed via ps on runner-3 that the codex process is gone; \
   no uncommitted worktree changes found"
   ```

   This calls `POST /api/executions/{id}/requeue`, which:
   - succeeds only for a request the recovery service *itself* already
     marked `needs_operator`. It is not a general-purpose "force
     requeue anything" escape hatch;
   - is idempotent on `recovery_key`: replaying the identical call returns
     `replayed: true` instead of queuing twice;
   - returns `409 conflict` (`idempotency_conflict`) if the same key is reused with a
     different confirmation, and `409 invalid_transition`, naming the actual current
     state in `details.from`, for any request that isn't in a recovered
     `needs_operator` state.

   For example, requeuing a request that was never put into `needs_operator` answers
   `{"code":"invalid_transition","details":{"from":"unknown","to":"queued"}}`, never a
   silent success.
3. **Confirm.** `tack execution get <request-id>` shows `queued`. The requeue path only ever
   re-queues; if the work cannot be recovered, end the run by other means rather than expecting this
   command to force a terminal state.

---

## Cancellation is a request, not a guarantee

`POST /api/executions/{id}/cancel` records a **request** for cancellation; the runner
observes and reports the actual outcome via
`POST /api/runner/v1/attempts/{id}/cancellation-observation`. This mirrors the
recovery split deliberately: Tack cannot itself kill a process running on a runner's
machine, so it never pretends to.

Most agents cannot promise that a cancelled run's processes actually stop. An agent's own shell tool
starts its subprocess in a new session outside the runner's process group, and the runner has no way
to find it. docket is different: a cancellation is a call to docket's own cancel route, which stops the
running step and its process group. The Developer Guide's
[capability matrix](../developer/runner-contract.md#capability-matrix) lists what each agent promises.

The cancellation observation carries what happened, in `details`. `details.process_outcome`
says how the agent's own process stopped (`stopped`, `killed`, or `signal_failed` when the
stop signal could not be delivered, which leaves the observation ambiguous). `details.groups`
counts the process groups an agent reported and had not reported finished: `tracked` were
running when the cancellation began, `killed` are gone afterwards, and `survived` are still
running and need stopping by hand on the runner's machine. An agent that reports no process
groups always shows `tracked: 0`. That means nothing was reported, not that nothing was left
behind. Today that is every agent, so after a cancel, look for stray processes on the runner's
machine yourself.

A cancellation observation on an attempt that already reached a terminal state returns
`409 invalid_transition` with the actual terminal state named, not a false success.

---

## What this runbook does not cover

- **Database or migration recovery** (a crashed migration, a corrupted `.db` file). See
  [Backup and Restore](backup-restore.md).
