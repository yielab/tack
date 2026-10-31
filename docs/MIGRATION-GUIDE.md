# Migration Guide

How Tack upgrades its own database, and what to check before and after. Schema migrations are
automatic and safe by default; a snapshot is taken by itself before the one class of migration that
cannot simply be retried.

If you only use Tack as a project manager, upgrading is two steps: **back up, then start the new
binary.**

---

## Upgrading the binary

1. **Back up first**, always. See [Backup and Restore](book/src/admin-guide/backup-restore.md).
   Migrations run on the next startup; there is no separate "run migrations" command.

   ```sh
   tack backup /backups/tack-pre-upgrade.db
   ```

2. **Replace the binary** and start it. The server runs every pending migration before it accepts a
   request.

3. **What happens automatically:**
   - Every already-applied migration is checked against the binary's own ordered list by **name and by
     a deterministic checksum**. If your database's history is not an exact prefix of what this binary
     expects, or a previously applied migration's definition changed, startup refuses to proceed rather
     than guess. This is what prevents silently running an edited or reordered history.
   - Every ordinary migration runs in its own transaction; a failing statement rolls the whole
     migration back instead of leaving a half-applied schema.
   - A few migrations cannot simply be retried from the prior schema if they fail: the table rebuilds
     (037 and 038) and the ones that drop a table (063, and 064–073, which drop the legacy Docket
     control-plane bridge). **Before the first of these that your database has not yet run**, Tack
     takes one whole-database `VACUUM INTO` snapshot next to your database file, named
     `<your-db-file>.before-<migration-name>.sqlite`. One snapshot covers every such migration in that
     startup, and a retry never overwrites it. From `v0.1.0-beta.7` the first such migration is 063,
     so the file is `<your-db-file>.before-063_drop_model_profiles.sqlite`, and it is the file that
     holds your `control_planes` and `orch_*` rows, whatever its name says. A database from
     `v0.1.0-beta.9` has already run all of them, so upgrading from it takes no snapshot. The design is
     in `docs/adr/0001-transactional-migration-rebuild-recovery.md`.
   - The rebuild migrations run with `PRAGMA defer_foreign_keys=ON` inside their transaction, compare
     row counts and an explicit bidirectional column projection between the old and new table before
     dropping the original, and run `PRAGMA foreign_key_check` before committing, so a lossy or
     foreign-key-violating copy never reaches a committed state.

4. **If startup refuses to proceed,** do not force it. The error names the mismatch (unexpected entry,
   out-of-order name, or changed checksum). Confirm you are running the intended binary version; if the
   history itself is damaged, restore your pre-upgrade backup and try again.

`GET /api/health` reports `migrations_applied`; read that field rather than a hand-written count.

## What an upgrade removes

Almost every migration only adds tables and columns. These remove or reshape data:

| Migrations | What changes |
|---|---|
| 037, 038 | `orch_runs` and `orch_approvals` are rebuilt (copied, verified, swapped). |
| 063 | The unused `model_profiles` table is dropped. |
| 064–073 | The legacy Docket control-plane bridge is dropped: `control_planes` and the nine `orch_*` tables (registrations, dispatched tasks, runs, events, approvals, metrics and their daily rollups, trace cursors). |
| 074 | The unused `orchestration` column of `project_templates` is dropped. |
| 086 | A model becomes one string: a request's model provider and model id are replaced by a single `requested_model`, and stored snapshots and project and item defaults are rewritten to match. |
| 092 | Dollars per day is no longer a limit; the `usd_per_day` column of a project's budgets is dropped. Every limit is in tokens. |

The 064–073 drop means that if you had registered a Docket control plane, dispatched work through it, or
read its per-item token and cost counts, none of that survives in the running database. The rows exist
only in the automatic snapshot beside your database file. That
snapshot is a full copy of the database as it was, so it holds the control-plane tokens you had stored,
unencrypted: keep it where your database backups live, and delete it once you no longer need those rows.
Docket itself stays reachable as a harness on an execution request
([Running Agents on Tasks](book/src/user-guide/running-agents.md)); only the old bridge is gone.

Migration 094 adds a provisional token limit per project: a run whose profile sets no token limit runs
under the project's provisional one, 2,000,000 tokens unless the project changes it or turns it off.

## Settings that act after an upgrade

The execution, runner and fleet API is always mounted and has no enable switch. Creating an execution
request or enrolling a runner does nothing until something acts on it. Three settings change
behaviour, each independent of the others; their defaults and full descriptions are in
[`docs/CONFIG.md`](CONFIG.md):

| You want | Set |
|---|---|
| Answer a run's questions from the Inbox | `TACK_EXECUTION_DECISION_TOKEN`. Left unset on a loopback bind, the server makes its own file; elsewhere, with it unset, resolving a decision is refused rather than falling back to the operator token. |
| Delete old execution history, replay bookkeeping and artifact blobs | `TACK_EXECUTION_RETENTION_ENABLE=true`, with `TACK_EXECUTION_RETENTION_DAYS` (default 90). **This is the one setting that deletes data.** It is off by default. |
| Watch for stale claims and runs that need an operator | On by default (`TACK_EXECUTION_HEALTH_ENABLE`). It reads and logs only. |

For a first rollout on an existing deployment: upgrade with retention still off, confirm `/api/health`
and your board behave as before, and turn retention on only once you have decided how long execution
history and artifacts should live.

## Downgrading

Migrations are forward-only. There is no `tack migrate down`. To run an older Tack binary against a
database that has been migrated forward, restore the pre-upgrade backup from step 1; that is the only
supported path back. An old binary pointed directly at a newer schema fails the same checksum and order
check described above.

## See also

- [Backup and Restore](book/src/admin-guide/backup-restore.md): routine snapshots, staged restore and
  remote backup.
- [Troubleshooting](book/src/admin-guide/troubleshooting.md#startup-fails-with-a-migration-error): a
  startup that stops on a migration error.
- [Recovery Runbook](book/src/admin-guide/recovery-runbook.md): attempt-level recovery after a crashed
  runner or agent, a different concern from schema migration.
- [`docs/CONFIG.md`](CONFIG.md): every `TACK_*` variable.
- `docs/adr/0001-transactional-migration-rebuild-recovery.md`: why rebuild migrations snapshot, verify
  and swap.
