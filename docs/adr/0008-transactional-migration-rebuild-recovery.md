# ADR 0008: transactional migration rebuild recovery

## Status

Accepted for Part III / Phase 50 (2026-08-06).

## Context

Migrations 037 and 038 were committed to the planning branch but had not been
included in a released Tack build. Their original `CREATE ..._new → copy → DROP
→ RENAME` implementation ran outside a transaction, so a statement failure
could leave an original and staging table and the next boot only refused to
continue. Recovery depended on an operator backup that may not exist.

## Decision

Retain the 037/038 numbers and target schema because they are unreleased, but
replace their implementation in place before allocating any Part III migration
numbers.

- Every ordinary migration is one SQLite transaction. Its `_migrations` record
  is inserted in that same transaction immediately before commit.
- A rebuild runs its cleanup/create/copy/verify/swap/index sequence in one
  transaction. The source table is dropped only after equal row counts and
  bidirectional explicit-projection comparison prove the copy is lossless.
- Foreign keys remain enabled; `defer_foreign_keys` permits the runner to fetch
  and explicitly fail on every `PRAGMA foreign_key_check` row before commit.
- A stale `*_new` table is removed at the start of the unreleased rebuild only;
  the original source table remains authoritative. This repairs the
  non-production predecessor's leftover staging state without a boot loop.
- `_migrations` now records a deterministic checksum. At startup, recorded
  migrations must be the exact ordered prefix of the binary's list and every
  non-legacy checksum must match. Existing NULL checksums are adopted once.
- Before the first pending rebuild on a file-backed database, the runner creates
  `VACUUM INTO <db>.before-<migration>.sqlite`. It never overwrites that first
  pre-upgrade snapshot; retry reuses it. In-memory databases have no durable
  file and therefore intentionally have no snapshot.

## Consequences

An ordinary SQL error, copy mismatch, FK violation, or failure at any rebuild
statement rolls back the schema and migration record together. A normal retry
is safe. The automatic snapshot is a recovery artifact for storage/device
failure, not an excuse to weaken the atomic swap.

The migration runner cannot create a remote-backup retention policy or restore
workflow within this card's ownership. Release/operator documentation must say
where local snapshots are stored and arrange retention/off-host copies. Once a
037 or 038 build is released, their definitions become immutable under the
checksum invariant; any later correction needs a new migration.
