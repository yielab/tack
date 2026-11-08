# Migration Guide

How Tack upgrades its own database, and what to check before and after. Migrations run by
themselves when the server starts. A snapshot is taken first for the one kind that cannot simply be
retried.

If you only use Tack as a project manager, upgrading is two steps: **back up, then start the new
binary.**

---

## Upgrading the binary

1. **Back up first**, always. There is no separate "run migrations" command: they run on the next
   start. See [Backup and Restore](book/src/admin-guide/backup-restore.md).

   ```sh
   tack backup /backups/tack-pre-upgrade.db
   ```

2. **Replace the binary** and start it. The server runs every pending migration before it accepts a
   request.

3. **What happens automatically:**
   - Every applied migration is checked against the binary's own ordered list by **name and by a
     checksum**. If your database's history is not an exact prefix of what this binary expects, or
     an applied migration's definition changed, startup refuses to proceed rather than guess.
   - Every ordinary migration runs in one transaction with the record of its name; a failing
     statement rolls the whole migration back.
   - Two kinds cannot be retried from the prior schema if they fail: the table rebuilds (037 and
     038) and every migration that drops a table (063, 064 to 073 and 109). **Before the first of these
     that your database has not run**, Tack takes one whole-database `VACUUM INTO` snapshot next to
     the database file, named `<your-db-file>.before-<migration-name>.sqlite`. One snapshot covers
     every such migration in that start, and a retry never overwrites it. A database that has
     already run all of them takes no snapshot. The design is in
     `docs/adr/0001-transactional-migration-rebuild-recovery.md`.
   - A rebuild migration compares row counts and the copied columns between the old and the new
     table before it drops the original, and runs `PRAGMA foreign_key_check` before it commits, so a
     lossy copy never reaches a committed state.

4. **If startup refuses to proceed,** do not force it. The error names the mismatch (an unexpected
   entry, an out-of-order name or a changed checksum). Check that you are running the binary you
   meant to; if the history itself is damaged, restore the pre-upgrade backup and try again.

`GET /api/health` reports `migrations_applied`; read that rather than a count written here.

## What an upgrade removes

Most migrations only add tables and columns. These remove or reshape data:

| Migrations | What changes |
|---|---|
| 037, 038 | `orch_runs` and `orch_approvals` are rebuilt (copied, verified, swapped). |
| 063 | The unused `model_profiles` table is dropped. |
| 064 to 073 | The legacy docket control-plane bridge is dropped: `control_planes` and the nine `orch_*` tables. |
| 074 | The unused `orchestration` column of `project_templates` is dropped. |
| 086 | A model becomes one string: a request's model provider and model id are replaced by `requested_model`, and stored snapshots and project and item defaults are rewritten to match. |
| 092 | Dollars per day is no longer a limit: the `usd_per_day` column of a project's budgets is dropped. Every limit is in tokens. |
| 103 | The automatic status moves are gone: `projects.on_start_status`, `projects.on_finish_status` and `execution_requests.status_map_policy_id` are dropped. |
| 109 | A person's review of an attempt is one record: the merge-readiness pack's `mrp_reviews` table is folded into `attempt_reviews` (reason, pack and, from now on, reviewer) and dropped; the time a pack was opened moves onto the pack's artifact. A review given before names no reviewer. |
| 112 | No price in money remains in Tack: a rate card entered before is deleted. Usage already recorded keeps the figures it was stored with; nothing reads them. |

If you had registered a docket control plane through the old bridge, none of it survives in the
running database. The rows exist only in the automatic snapshot beside your database file. That
snapshot is a full copy of the database as it was, so it holds the control-plane tokens you had
stored, unencrypted: keep it where your database backups live, and delete it once you no longer
need those rows. docket itself stays reachable as an agent on a run
([Running Agents on Tasks](book/src/user-guide/running-agents.md)); only the old bridge is gone.

Migration 094 gives each project a provisional token limit: a run whose profile sets no token
limit runs under the project's, 2,000,000 tokens unless the project changes it or turns it off.

## Settings that act after an upgrade

The execution, runner and fleet API is always mounted and has no enable switch. Three settings
change behaviour, each independent of the others; their defaults and full descriptions are in
[`docs/CONFIG.md`](CONFIG.md):

| You want | Set |
|---|---|
| Answer a run's questions from the Inbox | `TACK_EXECUTION_DECISION_TOKEN`. Left unset on a loopback bind, the server makes its own file; elsewhere, resolving a decision is refused rather than falling back to the operator token. |
| Delete old execution history, replay bookkeeping and artifact blobs | `TACK_EXECUTION_RETENTION_ENABLE=true`, with `TACK_EXECUTION_RETENTION_DAYS` (default 90). **This is the one setting that deletes data.** It is off by default. |
| Watch for stale claims and runs that need an operator | On by default (`TACK_EXECUTION_HEALTH_ENABLE`). It reads and logs only. |

On an existing deployment, upgrade with retention still off, confirm `/api/health` and your board
behave as before, and turn retention on only once you have decided how long execution history and
artifacts should live.

## Downgrading

Migrations only go forward. There is no `tack migrate down`. To run an older binary against a
database that has been migrated forward, restore the pre-upgrade backup; that is the only supported
way back. An old binary pointed at a newer schema fails the name and checksum check above.

## See also

- [Backup and Restore](book/src/admin-guide/backup-restore.md): snapshots, staged restore and remote
  backup.
- [Troubleshooting](book/src/admin-guide/troubleshooting.md#startup-fails-with-a-migration-error): a
  startup that stops on a migration error.
- [Recovery Runbook](book/src/admin-guide/recovery-runbook.md): recovery after a crashed runner or
  agent, a different concern from schema migration.
- [Adding a migration](book/src/developer/adding-features.md#adding-a-migration), for contributors.
