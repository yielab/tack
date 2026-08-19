# Backup and Restore

Tack offers three data-protection mechanisms: **hot backup** (a database-only SQLite copy),
**remote/cloud backup** (a full bundle including on-disk files), and **JSON export** (a
human-readable per-project snapshot). All three can be triggered via the API or the CLI.

---

## Hot Backup

Uses SQLite's `VACUUM INTO` to produce a clean, consistent copy of the database while the
server is running. No downtime required.

**Via CLI:**

```sh
tack backup                        # timestamped file in current directory
tack backup --path /backups/tack.db
```

**Via API:**

```sh
curl -O -J http://127.0.0.1:3210/api/backup
# With token:
curl -O -J -H "Authorization: Bearer <token>" http://127.0.0.1:3210/api/backup
```

**What the backup includes:**

- All projects, items, sprints, roles, comments, dependencies
- Attachment metadata (filenames, sizes, MIME types)
- Workflow configs and vocabulary maps
- Migration history

**What it does NOT include:**

- Attachment files and execution artifacts — both stored under `TACK_STORAGE_DIR`
  (default `./storage`; execution artifacts specifically in
  `TACK_STORAGE_DIR/execution-artifacts`). Back that directory up separately, or use
  [Remote/Cloud Backup](#remotecloud-backup) below, which includes it automatically.

---

## Staged Restore

Restore does not replace the live database while the server runs. You **stage** a file; the
swap happens on the next startup.

**Steps:**

1. Upload the backup:

```sh
tack restore /backups/tack.db
```

or via API:

```sh
curl -X POST http://127.0.0.1:3210/api/restore \
  -F "file=@/backups/tack.db"
# → {"status":"staged","message":"Restart the server to apply."}
```

2. Restart the server. On startup, Tack:
   - Moves `tack.db` → `tack.db.bak`
   - Moves the staged file → `tack.db`
   - Runs any pending migrations

The previous database is kept as `tack.db.bak`. Delete it once you've verified the restore.

---

## Remote/Cloud Backup

Unlike the hot backup above, a remote backup bundle includes **the database and every
file under `TACK_STORAGE_DIR`** — item attachments and, since Part III,
`execution-artifacts/` (artifacts a runner uploaded for an execution attempt — logs,
diffs, generated files; see [Agent Runners](agent-runners.md#workspace-and-artifact-storage)).
This is the one backup mechanism that captures artifacts without a separate `rsync`
step. It requires cloud object storage to be configured — see
[Cloud Backup](administration.md#cloud-backup-s3-compatible) for the S3-compatible
setup — and is otherwise inert.

```sh
tack backup --remote                 # push a bundle now
tack backups                         # list bundles in the bucket
```

The bundle is a `zstd`-compressed tar (`database.db`, the recursively-walked storage
tree, and a `manifest.json` with a sha256 of the database and an item count). Secrets
are scrubbed from the embedded database snapshot before it's bundled — see
`scrub_snapshot_secrets` in `crates/tack-api/src/remote_backup.rs`, kept in sync with
every secret-bearing column in the same commit that adds one.

Restore is staged the same way as local restore — download, stage, restart:

```sh
tack restore --remote --key <object-key>   # omit --key to restore the latest bundle
```

On the next startup, the staged database swaps in and the staged storage tree is
merged into `TACK_STORAGE_DIR` — so a remote restore recovers execution artifacts and
attachments together with the database, in one step, unlike the local hot-backup path
below.

---

## JSON Export (Project Snapshot)

A human-readable snapshot of a single project. Not a substitute for a full backup, but useful for archiving completed projects or migrating between instances.

```sh
# Full project snapshot (all items, sprints, roles, comments, dependencies)
curl "http://127.0.0.1:3210/api/projects/{id}/export?format=json" -o project.json

# Item list as CSV (for spreadsheets)
curl "http://127.0.0.1:3210/api/projects/{id}/export?format=csv" -o items.csv
```

---

## Attachment Files

Attachment files live in `TACK_STORAGE_DIR` (default `./storage`) and are not part of the database backup. Back them up separately:

```sh
rsync -a ./storage/ /backups/tack/storage/
```

For a complete restore you need both the database backup and the storage directory snapshot taken at approximately the same time.

---

## Recommended Frequency

| Situation | Action |
|---|---|
| Before a server upgrade | Full backup first |
| Before bulk import or schema changes | Full backup first |
| Routine protection | Daily cron (see below) |
| Completing a project phase | JSON export for archival |

**Daily cron example:**

```sh
0 2 * * * curl -s -O -J \
  -H "Authorization: Bearer $TACK_TOKEN" \
  http://127.0.0.1:3210/api/backup \
  --output-dir /backups/tack/
```
