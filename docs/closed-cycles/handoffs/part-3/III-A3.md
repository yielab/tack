# III-A3 handoff — migration runner and rebuild recovery

## Delivered

- Decided 037/038 are **unreleased** and retained their numbers/target schema,
  with their unsafe implementation replaced before Phase 51 numbering begins.
- Converted ordinary migrations to transaction + record + commit semantics.
- Rebuilt 037/038 as transactional copy/verify/swap operations. Copy verification
  uses count equality and explicit bidirectional projections before source drop;
  FK check rows are fetched and asserted.
- Added ordered-prefix and deterministic migration-checksum enforcement. Old
  NULL checksums are adopted once for compatibility.
- Added automatic file-backed `VACUUM INTO` snapshot at
  `<database>.before-037_orch_runs_rebuild.sqlite` before the first pending
  rebuild. In-memory pools intentionally skip this because no durable path
  exists.
- Recovered unreleased stale `orch_runs_new` / `orch_approvals_new` state rather
  than leaving a boot refusal loop.

## Verification

`cargo test -p tack-db --test orch_migrations_test` passes 31 tests, including:

- failure injection before every SQL, copy-verification, and FK-check boundary
  of both rebuilds, followed by a successful retry;
- lossy-copy-equivalent projection protection, corrupted-source FK rejection,
  migration-record-after-commit checks, stale-staging recovery, checksum
  tampering rejection, and file-backed automatic snapshot creation.

## Follow-up contract requests

- The backup implementation/remote retention owner should surface the local
  pre-upgrade snapshot path in operator UX and establish retention/off-host
  policy. This card intentionally did not modify backup code.
- Future migration owners must append only. Changing a released migration's
  SQL/name now fails closed by checksum; allocate a new migration instead.
- If 037/038 were found to have shipped outside this repository's release
  process, stop and treat that as a release incident: this in-place replacement
  relies on the Phase 50 unreleased decision.
