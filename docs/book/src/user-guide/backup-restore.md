# Backup and Restore

FlexPM offers two data-protection mechanisms: **hot backup** (a full SQLite copy) and **JSON
export** (a human-readable per-project snapshot). Both can be triggered via the API or the CLI.

---

## Hot Backup

Uses SQLite's `VACUUM INTO` to produce a clean, consistent copy of the database while the
server is running. No downtime required.

**Via CLI:**

```sh
flexpm backup                        # timestamped file in current directory
flexpm backup --path /backups/flexpm.db
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

- Attachment files — stored in `FLEXPM_STORAGE_DIR` (default `./storage`). Back that directory up separately.

---

## Staged Restore

Restore does not replace the live database while the server runs. You **stage** a file; the
swap happens on the next startup.

**Steps:**

1. Upload the backup:

```sh
flexpm restore /backups/flexpm.db
```

or via API:

```sh
curl -X POST http://127.0.0.1:3210/api/restore \
  -F "file=@/backups/flexpm.db"
# → {"status":"staged","message":"Restart the server to apply."}
```

2. Restart the server. On startup, FlexPM:
   - Moves `flexpm.db` → `flexpm.db.bak`
   - Moves the staged file → `flexpm.db`
   - Runs any pending migrations

The previous database is kept as `flexpm.db.bak`. Delete it once you've verified the restore.

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

Attachment files live in `FLEXPM_STORAGE_DIR` (default `./storage`) and are not part of the database backup. Back them up separately:

```sh
rsync -a ./storage/ /backups/flexpm/storage/
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
  -H "Authorization: Bearer $FLEXPM_TOKEN" \
  http://127.0.0.1:3210/api/backup \
  --output-dir /backups/flexpm/
```
