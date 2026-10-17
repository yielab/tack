# Backup and Restore

This page is for the person who protects Tack's data. After reading it you can take a backup while the
server runs, restore one, send backups to cloud storage, and tell which copy to use when.

Tack offers three data-protection mechanisms: **hot backup** (a database-only SQLite copy),
**remote/cloud backup** (a full bundle including on-disk files), and **JSON export** (a
human-readable per-project snapshot). All three can be triggered via the API or the CLI. The commands
below assume the token that every API call needs; see
[Administration & Security](administration.md#authentication). The `tack` command finds the token of a
local server by itself.

| You want | Use |
|---|---|
| A quick copy before an upgrade | [Hot backup](#hot-backup) |
| Protection against losing the machine | [Remote/cloud backup](#remotecloud-backup) |
| To archive or move one project | [JSON export](#json-export-project-snapshot) |

---

## Hot Backup

Uses SQLite's `VACUUM INTO` to produce a clean, consistent copy of the database while the
server is running. No downtime required.

**Via CLI:**

```sh
tack backup                        # saves tack-backup.db in the current directory
tack backup /backups/tack.db       # saves it where you say
```

**Via API:**

```sh
curl -O -J -H "Authorization: Bearer <token>" http://127.0.0.1:3210/api/backup
```

**What the backup includes:**

- Everything in the database: projects, items, sprints, roles, comments, dependencies, runs and their
  history
- Attachment metadata (filenames, sizes, MIME types)
- Workflow configs and vocabulary maps
- Migration history

**What it does NOT include:**

- Attachment files and execution artifacts, both stored under `TACK_STORAGE_DIR`
  (default `./storage`; execution artifacts specifically in
  `TACK_STORAGE_DIR/execution-artifacts`). Back that directory up separately, or use
  [Remote/Cloud Backup](#remotecloud-backup) below, which includes it automatically.
- Machine-local secrets. The backup is made without the cloud-backup secret key and without this
  install's identity, so that neither leaves the machine in a file. After you restore on a new
  machine, enter the cloud-backup settings again under **Settings → Cloud Backup**.
- The runner's own state. A runner keeps its credential, journal and secrets in its state directory;
  see [Installing and Operating Runners](runners.md#workspace-and-artifact-storage).

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
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @/backups/tack.db
# → {"status":"staged","restore_path":"…","message":"Restore staged. Stop the server and restart to apply."}
```

The server checks that the file is a SQLite database and that its schema is not newer than the running
Tack. A newer one is refused with `409`, because it would stop the server from starting; upgrade Tack
first. If cloud backup is configured, the server also uploads a snapshot of the current state before it
stages the restore, and carries on with the restore if that upload fails.

2. Restart the server. On startup, Tack:
   - Removes the old database's write-ahead files
   - Moves `tack.db` → `tack.db.bak-<timestamp>`
   - Moves the staged file → `tack.db`
   - Runs any pending migrations

If a step fails, Tack puts the original files back and starts with the database you had. The previous
database is kept as `tack.db.bak-<timestamp>`. Only the newest such copy is kept, so copy it elsewhere
if you want to keep it, and delete it once you have verified the restore.

---

## Remote/Cloud Backup

Unlike the hot backup above, a remote backup bundle includes **the database and every
file under `TACK_STORAGE_DIR`**: item attachments and `execution-artifacts/` (what a runner uploaded
for an attempt, such as logs, diffs and generated files; see
[Installing and Operating Runners](runners.md#workspace-and-artifact-storage)).
This is the one backup mechanism that captures artifacts without a separate `rsync`
step. It requires cloud object storage to be configured, and is otherwise inert. See
[Cloud Backup](administration.md#cloud-backup-s3-compatible) for the S3-compatible setup.

```sh
tack backup --remote                 # push a bundle now
tack backups                         # list bundles in the bucket
```

The bundle is a `zstd`-compressed tar (`database.db`, the whole storage
tree, and a `manifest.json` with a sha256 of the database and an item count). The same machine-local
secrets as above are removed from the database copy before it is bundled.

Restore is staged the same way as local restore: download, stage, restart.

```sh
tack restore --remote --key <object-key>   # omit --key to restore the latest bundle
```

On the next startup, the staged database swaps in and the staged storage tree replaces
`TACK_STORAGE_DIR`, with the old one kept beside it as a timestamped `.bak`. So a remote restore
recovers execution artifacts and attachments together with the database, in one step, unlike the local
hot-backup path.

Bundles carry a generation counter and an install ID. An upload that would overwrite newer work from a
different install is refused, and so is a restore that would clobber newer local work; both ask for
explicit confirmation.

---

## JSON Export (Project Snapshot)

A human-readable snapshot of a single project. Not a substitute for a full backup, but useful for archiving completed projects or migrating between instances.

```sh
# Full project snapshot (all items, sprints, roles, comments, dependencies)
curl -H "Authorization: Bearer <token>" \
  "http://127.0.0.1:3210/api/projects/{id}/export?format=json" -o project.json

# Item list as CSV (for spreadsheets)
curl -H "Authorization: Bearer <token>" \
  "http://127.0.0.1:3210/api/projects/{id}/export?format=csv" -o items.csv
```

`scope=bundle` exports the project file instead, which carries a project's settings, roles, profiles
and automation so that it can be set up again somewhere else; see
[Import, Export & Project Files](../user-guide/import-export.md).

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
  --output-dir /backups/tack/ \
  http://127.0.0.1:3210/api/backup
```
