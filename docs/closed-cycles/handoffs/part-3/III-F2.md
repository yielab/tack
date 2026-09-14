# III-F2 handoff

- **Base SHA / branch / final SHA:** base `cbdd4a325a89df3f97bd8bc3009f51024df065fb`
  (`cbdd4a3`, tip of `plan/harness-agnostic-agent-fleet` — "docs: close out Wave 4 with
  the III-E6 handoff and accepted integration SHA") / `agent/iii-f2-artifacts` / final
  SHA recorded in the commit that follows this handoff in the same worktree. One commit.

## Files changed (must equal ownership list)

Card charter: "new event/artifact service/storage modules, retention tests and F2
handoff." Everything below is either a new module/test file or an extension to the two
files the card brief itself named as "existing code you are extending."

- **New modules** (all nested under `crates/tack-api/src/handlers/runner_protocol/` —
  see the "nesting" note below):
  - `artifact_storage.rs` — the safe, streamed artifact-content storage module. Owns
    path safety (hex-encoded ids, symlink/containment checks), the streaming
    write-with-verification (`store_streaming`), and streaming read (`open_for_read`).
    Colocated tests.
  - `retention.rs` — pure event/artifact retention sweep *behavior* (`sweep_events`,
    `sweep_artifacts`). No background task, no scheduling — that is III-F5's charter.
  - `artifact_download.rs` — the operator-facing verified-artifact download handler.
    Not merged into `runner_protocol`'s own runner-only router; proven only via its own
    `routes(state) -> Router` constructed locally in this card's test file. See "Schema/
    API/contract change requested" below for the real mounting request.
- **New tests:**
  - `crates/tack-api/tests/f2_artifact_events_test.rs` — HTTP-level: artifact
    manifest→PUT-content→download round trip, checksum mismatch, oversize/
    compression-bomb shape, path traversal, symlink escape (via the module's own
    colocated tests, loaded through this binary too), immutability, fencing, content-type
    mismatch, a >4 MiB upload proving the per-route body-limit override, and log
    redaction.
  - `crates/tack-db/tests/f2_event_artifact_retention_test.rs` — repository-level: event
    batch insert-failure atomicity (two variants), artifact content-reference
    immutability/fencing, and retention sweep bounded-batch behavior.
- **Extended (both explicitly named in the card brief as files to extend):**
  - `crates/tack-api/src/handlers/runner_protocol.rs` — new `mod artifact_storage;` /
    `mod retention;` / `mod artifact_download;` declarations (nested the same way
    `runner_auth` already is, see below); `RunnerProtocolState` gains an additive
    `artifact_storage: Arc<ArtifactStorage>` field + `with_artifact_storage_root`
    builder (`new`'s two-argument signature is unchanged); a new route,
    `PUT /attempts/{attempt_id}/artifacts/{artifact_id}/content`
    (`put_artifact_content`), added to this file's own already-mounted `routes()`
    function with its own per-route `DefaultBodyLimit` override; `submit_artifacts`'s
    returned `upload.path` is now attempt-scoped (see "A resolved ambiguity" below) and
    gained a `media_type` format check.
  - `crates/tack-db/src/repo/execution.rs` — new `ExecutionArtifactRow`,
    `ArtifactContentCommitResult`, `get_execution_artifact`,
    `get_execution_artifact_by_attempt_number`,
    `set_execution_artifact_content_reference`, `purge_execution_events_older_than`,
    `list_execution_artifacts_older_than`, `delete_execution_artifacts_by_row_ids`. No
    migration — every column already exists (046/047).
- **Not touched:** `router.rs`, `openapi.rs`, `handlers/mod.rs`, `migrations.rs`,
  `repo/mod.rs`, `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
  `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`, root
  `Cargo.toml`/`Cargo.lock`, any other card's handoff, anything under `frontend/`.

**On the nesting:** `artifact_storage.rs`/`retention.rs`/`artifact_download.rs` are
declared as submodules of `runner_protocol.rs` (`#[path = "runner_protocol/…"] pub mod
…;`), exactly mirroring how `runner_auth.rs` is already nested there. This is not
stylistic — it is what keeps every new module reachable without touching
`handlers/mod.rs`, which this card must not edit. `artifact_download` is genuinely
operator-facing (reads `x-tack-principal`, never a runner bearer credential) despite the
nesting; the doc comment on its `mod` line explains why it lives there anyway.

## Contract fixtures consumed

`docs/contracts/runner-v1/event-batch.request.json`/`.response.json`,
`artifact.request.json`/`.response.json`, `limits.json`, `protocol.json`,
`errors/artifact-checksum-mismatch.json`, `errors/payload-too-large.json`,
`errors/invalid-request.json`, `errors/stale-lease.json`, `errors/conflict.json`. All
consumed read-only; none edited. `cargo test -p tack-orch --test runner_contract` —
**18/18**, unmodified.

One important finding while reading them: **the artifact content-upload endpoint has no
frozen URL.** `docs/contracts/runner-v1/` fixes payload *shapes*, not URLs, for every
canonical exchange except the one named explicitly in `protocol.json`
(`recovery-observation`). `runner_protocol.rs`'s own top-of-file doc comment says as much
("Route layout is this card's own choice"). The pre-F2 `submit_artifacts` code left a
placeholder path (`/api/runner/v1/artifacts/{artifact_id}/content`) with a comment
admitting "no artifact-content-upload endpoint is part of this card." I did not treat
that placeholder string as binding: it under-specifies the resource (two different
attempts can choose the same runner-supplied `artifact_id`, and this endpoint needs an
attempt to authenticate/fence against), so I mounted the real endpoint at
`/attempts/{attempt_id}/artifacts/{artifact_id}/content` instead and updated
`submit_artifacts`'s returned `upload.path` to match. No test anywhere asserted the old
literal string (checked `c2_handlers_test.rs`, which only asserts
`upload.method == "PUT"`), so this is not a breaking change to any other card's tests.

## Behavior implemented

### 1. Atomic event batch/checkpoint

This was already correct, inherited from B2's `append_execution_events_result`
(`BEGIN IMMEDIATE`, event inserts and the checkpoint `UPDATE` in one transaction, no
intermediate commit). My job here was proof, not a fix: `f2_event_artifact_retention_test.rs`
forces a mid-batch INSERT failure via a deterministic `BEFORE INSERT ... RAISE(ABORT,
…)` trigger (mirroring `execution_repo_test.rs`'s own precedent for an analogous
completion-replay claim) and asserts both that the checkpoint stays at its last
*successfully committed* value (not just "unchanged from NULL" — a second test proves
the fresh-attempt/NULL case too) and that the row count is unchanged, including for an
event whose own insert did *not* trigger the abort.

### 2. Bounded payload / truncation

Already enforced by the existing `submit_events` (per-event `event_payload_bytes_max`,
whole-batch `event_batch_count_max`, validated before any repository call — a rejected
batch writes nothing, already tested in `c2_handlers_test.rs`/`wave2_gate.rs`). Nothing
new needed here; I did not find a gap. `tack-runner`'s own event sink (`event_sink.rs`)
truncates oversized payloads client-side with an explicit marker before they are ever
sent, which is why the server-side posture (hard reject rather than truncate) is
consistent, not competing, with the client-side one.

### 3. Artifact manifest, checksum/size/content-type validation

`submit_artifacts` (pre-existing) already validated `size_bytes` and `sha256` shape. I
added:

- `is_plausible_media_type` — `media_type` had *no* shape or length check before this
  card (any string, any length). Now requires a plausible `type/subtype` shape and a
  255-byte cap. Not fixed by `limits.json` (no field bounds it); an explicitly-chosen,
  documented convention, matching the style of `ARTIFACT_UPLOAD_WINDOW_SECONDS`'s own
  "not fixed by any fixture" precedent in this file.
- Content-type validation at *upload* time: `put_artifact_content` compares the PUT
  request's `Content-Type` header (when present) against the manifest's declared
  `media_type` (when present), case-insensitively and parameter-stripped (`text/x-diff`
  matches `text/x-diff; charset=utf-8`). A mismatch is rejected as `invalid_request`
  *before* any byte of the body is read.

### 4. Safe reference/path storage

`artifact_storage.rs`'s `ArtifactStorage`. Every path is built by hex-encoding
`attempt_id`/`artifact_id` (mirroring `tack-runner`'s own `harness/artifact.rs#encode_id`
convention) — a `..`, `/`, or NUL byte in either id becomes two harmless hex digits, so
traversal via id *content* is structurally impossible, not merely string-checked. Every
directory is canonicalized and checked for containment inside the canonicalized storage
root before any write; a pre-existing symlink at either the storage root or the
per-attempt directory is explicitly rejected (`reject_symlink`, checked via
`symlink_metadata` *before* `create_dir_all`, which would otherwise silently follow it).
Every temp file is opened with `create_new(true)`, which refuses to follow an existing
symlink or overwrite an existing file — no TOCTOU window between checking and creating
the temp file itself.

`content_reference` (the value persisted to `execution_artifacts`) is a path *relative*
to the storage root (`<hex(attempt_id)>/<hex(artifact_id)>-<sha256-prefix>.blob`),
mirroring `attachments.rs`'s own `storage_path` convention for portability.

### 5. Streaming content

**Write side** (`ArtifactStorage::store_streaming`): consumes `axum::body::Body`'s own
`into_data_stream()` chunk by chunk (no new dependency — this is an inherent axum
method). Each chunk is hashed and written to a temp file, then dropped, before the next
chunk is pulled — never buffered as a `Vec<Bytes>`. Aborts (deletes the temp file,
returns `OversizeStream`) the instant more bytes have arrived than the manifest's own
declared `size_bytes` — enforced *while consuming*, not after. This is the "compression
bomb" defense: a manifest can declare an innocuous small size while the real body tries
to deliver arbitrarily more (or, in the test, never stops at all).

**Read side** (`artifact_download.rs::chunked_read_stream`): a hand-rolled
`futures::stream::unfold` over a `tokio::fs::File`, 64 KiB per chunk, again no new
dependency (`tokio-util::io::ReaderStream` would have been the obvious crate — see
"Dependency requests" below for why I did not add it).

**A route-level gotcha this card had to fix itself:** `runner_protocol.rs`'s existing
router-wide `DefaultBodyLimit` (a fixed 4 MiB ceiling meant for JSON control-plane
bodies) would have rejected any real artifact upload (up to 50 MB per
`artifact_content_bytes_max`) before `put_artifact_content` ever ran. Fixed with a
more-specific, per-route `DefaultBodyLimit::max(artifact_content_bytes_max)` layered
directly on this one route (axum applies whichever `DefaultBodyLimit` is closest to the
handler — the same precedence this file's own `effective_body_limit_bytes` doc comment
already documents for the router-wide case). Proved end-to-end with a real 6 MiB upload
through the real router (`an_upload_larger_than_the_default_json_body_ceiling_still_succeeds`).

### 6. Retention behavior

`retention.rs`'s `sweep_events`/`sweep_artifacts`, backed by three new `tack-db`
methods. `sweep_artifacts` is two-phase by construction: fetch expired rows (need
`content_reference` to unlink the right blob), unlink each blob (best-effort — a
already-missing blob or a manifest-only row with no blob is not an error), only then
delete the DB rows by their row ids. Both sweeps take a `batch_limit` (bounded pass; a
caller loops until it sees fewer rows than the limit) via a `WHERE … LIMIT ?` **subquery**
(`DELETE … WHERE id IN (SELECT id … LIMIT ?)`), not a top-level `DELETE … LIMIT`, which
is a nonstandard SQLite extension not guaranteed to be compiled in.

**This card owns behavior and tests only** — no background task, no cancellation, no
metrics, no startup/shutdown wiring. That is III-F5's explicit charter per the Wave 5
board. `sweep_events`/`sweep_artifacts` are plain `async fn`s F5 can call from whatever
recurring-task shape it builds.

## Tests added and exact commands/results

- `cargo test -p tack-api --lib runner_protocol` — **19 passed, 0 failed** (8
  `artifact_storage` + 1 `retention` + the pre-existing 10 `runner_protocol`/
  `runner_auth` tests, unmodified).
- `cargo test -p tack-api --test f2_artifact_events_test` — **33 passed, 0 failed**
  (14 new HTTP-level tests + the 19 module tests, reproduced in this binary the same
  way `c2_handlers_test.rs` reproduces them — see "Known limitations" for why).
- `cargo test -p tack-api --test c2_handlers_test` — **32 passed, 0 failed** (up from
  E6's own recorded baseline of 23 — the +9 is exactly this card's 9 new
  `artifact_storage`/`retention` module tests, reproduced here because
  `c2_handlers_test.rs` also loads `runner_protocol.rs` via `#[path]`; every one of C2's
  own 23 original tests still passes unmodified, confirming this card's edits did not
  disturb that suite's own behavior).
- `cargo test -p tack-db --test f2_event_artifact_retention_test` — **7 passed, 0
  failed**: event-batch atomicity (partial-batch rollback + fresh-attempt/NULL case),
  content-reference immutability + stale-fence rejection, event retention bounded-batch
  purge, artifact retention list+delete.
- `cargo test -p tack-db --test execution_repo_test` — pre-existing suite, unchanged,
  confirms no regression from the two new `repo/execution.rs` structs/methods added
  alongside its own.
- `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed**.
- `cargo test -p tack-orch --test runner_contract` — **18 passed, 0 failed**.
- `cargo test --workspace` — **1192 passed, 0 failed, 6 ignored** (Wave 4's own
  baseline before this card was 1134 passed, 6 ignored; this card added 58 new Rust
  test executions net — the exact per-binary breakdown above accounts for all of them,
  including the module-test duplication across `f2_artifact_events_test.rs`/
  `c2_handlers_test.rs`, an established pre-existing pattern in this codebase, not one
  this card introduced).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean, workspace-wide; confirmed via the pre-format `cargo fmt
  --check` output that every flagged diff was inside a file this card owns (`runner_protocol.rs`
  and its three new submodules, plus this card's two new test files) before running
  `rustfmt` on exactly those seven files.

## Failure/adversarial case proved

Every guard below was proved load-bearing by hand: temporarily disabling it, re-running
the exact test, observing the failure, then restoring the guard and re-confirming green.

1. **Checkpoint never advances after a failed insert.** Changed the per-event `INSERT`
   in `append_execution_events_result` to execute against `self.pool()` instead of `&mut
   *tx` (detaching it from the shared `BEGIN IMMEDIATE` transaction). Re-running the test
   did not just fail an assertion — **the test process hung indefinitely**: the outer
   transaction still held the (single, in-memory-SQLite) pool connection/write lock, and
   the detached insert's own attempt to check out a connection from the same pool
   deadlocked against it — exactly the class of self-deadlock CLAUDE.md's "`BEGIN
   IMMEDIATE` is mandatory for read-then-write transactions" warning describes. Killed
   the hung process, reverted, confirmed the test passes again promptly. A stronger
   confirmation than a clean assertion failure would have been.
2. **Checksum mismatch stages nothing.** Commented out the `if digest != declared_sha256`
   early return in `store_streaming`. Test failed: `Ok(...)` with a real committed blob
   instead of `Err(ChecksumMismatch)`. Restored, re-confirmed green.
3. **Size mismatch (short stream) stages nothing.** Commented out the `if total_written
   != declared_size_bytes` early return. Test failed the same way (a blob committed for
   a stream that delivered only 5 of its declared 105 bytes). Restored, re-confirmed.
4. **Oversize / "compression bomb" rejected before exhausting memory.** Commented out the
   `if total_written > declared_size_bytes { break }` guard inside the read loop.
   Re-running the test against an infinite synthetic stream: it failed with
   `Elapsed(())` — the surrounding 5-second `tokio::time::timeout` had to kill it,
   because without the guard the loop happily keeps consuming the unbounded stream and
   writing to disk forever (confirmed ~39 MB had accumulated in under 5 seconds before
   the harness killed it). Restored, re-confirmed the test passes promptly.
5. **Path traversal via id content is structurally impossible.** Changed `encode_id` to
   return the raw id unencoded. Test failed (`Io` error — the raw id containing `..` and
   a NUL byte is not even a valid path component). Restored, re-confirmed.
6. **Symlink escape (tested explicitly, not just string matching).** Disabled *both*
   `safe_attempt_dir` guards at once (the explicit `reject_symlink` pre-check and the
   final canonicalize+`starts_with` containment check — disabling only one at a time
   left the other still catching it, correct defense-in-depth but not proof this
   *specific* test is load-bearing). With both disabled: the test failed — real bytes
   were written through the planted symlink into a directory outside the storage root.
   Restored both, re-confirmed.

All six are documented in the source itself, next to the guard/test each one covers, not
only here.

## Schema/API/contract change requested from another owner

No schema change was needed — every field used already exists in migrations 046/047, as
instructed.

Two real production-wiring requests for the Wave 5 integrator (neither touches a file
this card owns):

1. **Point artifact storage at `TACK_STORAGE_DIR`.** Until wired,
   `RunnerProtocolState::new` defaults `artifact_storage` to a hardcoded,
   process-CWD-relative path (`./storage/execution-artifacts`) rather than the
   operator-configured `TACK_STORAGE_DIR` — because `router.rs#runner_protocol_routes`
   (the one production call site) is off-limits to this card. The fix is additive and
   one line:

   ```rust
   let runner_state = runner_protocol::RunnerProtocolState::new(state.repo.clone(), clock)
       .with_artifact_storage_root(format!("{}/execution-artifacts", state.config.storage_dir));
   ```

2. **Mount the operator artifact-download route.** `artifact_download.rs`'s
   `download_artifact_content` handler and its `routes(state)` constructor are complete
   and tested (via a locally-constructed router only — see "Known limitations"). The
   integrator should mount it under the real operator router, e.g. in
   `router.rs#operator_execution_routes`, matching the addressing style
   `executions.rs`'s own attempts/events routes already use:
   `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`,
   constructing `artifact_download::ArtifactDownloadState { repo: state.repo.clone(),
   artifact_storage: Arc::new(artifact_storage::ArtifactStorage::new(format!("{}/execution-artifacts",
   state.config.storage_dir))) }` (same storage root as request 1, for consistency).
   This is the endpoint III-F4's "verified artifact download" task will need.

**Dependency requests: none.** Streaming (both directions) was achieved with
already-available crates only — `axum::body::Body::into_data_stream()` (an inherent
method, no new dependency) for the write side, and `futures::stream::unfold` (already in
`futures = "0.3"`) over a `tokio::fs::File` for the read side. I deliberately did not
reach for `tokio-util::io::ReaderStream`, the more idiomatic crate for exactly this,
because it is not a direct dependency of `tack-api` today (only a transitive one via
other crates) and the card brief is explicit that new dependencies are a request, not
something to add. If a future card finds the hand-rolled `chunked_read_stream` worth
replacing, `tokio-util` (already pinned in `Cargo.lock` transitively, so no new major
version to vet) would be the natural choice — stating this rather than adding it.

## Known limitations or `not_measured` fields

- **"Large fixture is streamed, not buffered as a whole" — proved by construction plus
  an unbounded-input timeout, not by a direct peak-memory measurement.** I did not
  instrument actual process RSS (would need a custom global allocator, invasive and
  without precedent elsewhere in this codebase). What is proved: (a) by code inspection,
  `store_streaming`'s loop holds only the current chunk plus a running `Sha256` digest
  state — never a `Vec<Bytes>` accumulator; (b) empirically, feeding an *infinite*
  synthetic stream causes a prompt, bounded-time rejection (`OversizeStream`) rather than
  an unbounded hang — which is only possible if the function is not first collecting the
  stream to completion before inspecting it (a `.collect()`-based implementation could
  never finish against an infinite stream, so it could never reach the size check at
  all). I consider this an honest, if indirect, proof of the "not buffered as a whole"
  claim — flagging the specific thing it does *not* directly measure (peak bytes
  resident) rather than overclaiming.
- **`content_disposition` is not re-validated at content-upload time.** `submit_artifacts`
  accepts any string for `content_disposition` (pre-existing behavior, unchanged by this
  card); `put_artifact_content` does not check it before accepting a content upload — any
  manifested artifact, regardless of its declared disposition, can receive a `PUT
  .../content`. The only documented value in the frozen fixtures is `inline_upload`; no
  other value is exercised anywhere in this repository today, so this is a real but
  currently-unreachable gap, not a demonstrated one.
- **`f2_artifact_events_test.rs` reproduces the 19 `runner_protocol`/`artifact_storage`/
  `retention` module tests** (via the same `#[path]` mechanism `c2_handlers_test.rs`
  already uses on itself) — pre-existing pattern in this codebase (every `#[path]`-loaded
  integration test binary recompiles and re-runs that file's own `#[cfg(test)]` modules),
  not something this card introduced, but noted here so the raw "33 passed" figure for
  that binary isn't mistaken for 33 independent new tests.
- **Aggregate `artifact_attempt_total_bytes_max` (500 MB per attempt) is enforced only at
  manifest time**, not re-checked at content-upload time. This is sufficient, not a gap:
  content upload can never exceed its own manifest's declared `size_bytes` (checksum/size
  verification guarantees this), and the manifest-time aggregate check already sums
  declared sizes against the same column — so the existing check is a valid upper bound
  on real committed bytes too. Documented here because it is not obvious without tracing
  through both code paths.
- **`retention.rs` has no scheduling** — by design, per the card/wave split (see
  "Behavior implemented" §6). F5 needs to decide batch size, poll interval, and how
  `RetentionPolicy` (currently `Default`, matching `limits.json`'s 30/30-day defaults) is
  configured/overridden at runtime.

## Secrets/logging review

- No `tracing::*!` call of any kind exists in `artifact_storage.rs`, `retention.rs`,
  `artifact_download.rs`, or the new `put_artifact_content` handler — nothing is logged,
  so nothing can leak. Matches the same posture E6 documented for
  `tack_orch::scheduler::wiring` (pure/IO-only code with no need for its own logging
  layer).
- `logs_never_leak_event_payloads_or_artifact_content_only_ids` (new,
  `f2_artifact_events_test.rs`) captures real `tracing_subscriber::fmt` output (the same
  process-global-subscriber-plus-thread-local-capture rig `c2_handlers_test.rs`'s
  `logs_never_contain_raw_credentials_only_ids` already established, reproduced here
  because each integration-test file is its own compiled binary and a `tracing` global
  default is process-wide) across an event batch carrying a distinctive payload marker,
  an artifact upload carrying distinctive content bytes, and a forced checksum-mismatch
  rejection (proving the *error path's* `details` field — `{"artifact_id": …}` — stays
  id-only too). Asserts all three markers are absent from captured output, and
  non-vacuously asserts the runner id *is* present (proving the rig observed real
  production log lines, not an empty/unreached subscriber).
- `ExecutionArtifactRow`/`SweepOutcome`/`StoredArtifactContent` carry no credential-shaped
  field — ids, sizes, hashes, and storage-relative paths, the same category of data the
  frozen fixtures themselves expose on the wire.
- `X-Tack-Fencing-Token` (the new header this card introduces, since a raw-bytes PUT body
  cannot carry a JSON `fencing_token` field) is an integer, never secret-shaped; not
  redacted, not logged, same as every other fencing token in this file's existing JSON
  bodies.

## Safe merge order and likely conflicts

- This branch never touched `router.rs`, `openapi.rs`, `handlers/mod.rs`,
  `migrations.rs`, `repo/mod.rs`, `docs/openapi.json`, `schema.gen.ts`,
  `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`, root
  `Cargo.toml`/`Cargo.lock`, or any other card's handoff — no conflict expected there.
- `crates/tack-api/src/handlers/runner_protocol.rs` is also III-F1's likely target
  (decisions live in the same file, per the existing `create_decision`/`poll_decisions`
  handlers). This card's edits are additive (new `mod` declarations near the top, a new
  route in `routes()`, a new handler appended after `submit_artifacts`, a small
  `PreparedArtifact` validation addition, and the `upload.path` string change) — a
  same-file merge with F1 should be a straightforward adjacent-line merge, not a logical
  conflict, but is worth flagging since both cards touch this file's route table and
  import block.
- `crates/tack-db/src/repo/execution.rs` is likely also touched by F1/F3/F5 (decisions,
  usage, retention are all natural extensions of this file). This card's additions are
  appended after `record_execution_artifact` and are self-contained (no existing method
  signature changed) — same expectation: adjacent-line merge, not logical conflict.
- `artifact_storage.rs`/`retention.rs`/`artifact_download.rs` are brand-new files; no
  conflict possible.

## Checklist

- [x] No unowned files touched — `git diff --stat` against base: exactly
      `runner_protocol.rs` and `repo/execution.rs` modified, five new files (three
      modules, two test files), nothing else.
- [x] No live secret committed, logged, or reachable via `argv`/`ps`/trace (see
      "Secrets/logging review").
- [x] No panic stub / `unimplemented!()` / fake success — every genuinely-unfinished
      piece (production storage-root wiring, the operator download route's real
      mounting, F5's retention scheduler) is a named, typed gap in this handoff, never a
      placeholder standing in for success. `ArtifactContentCommitResult::AlreadySet` is
      a real, distinct outcome, not collapsed into a bare `bool`.
- [x] No blind retry — every failure path in `put_artifact_content` (checksum mismatch,
      oversize, stream error, stale fence, immutable-already-set) returns a stable,
      named error and stages nothing; nothing in this card's new code automatically
      retries a failed operation.
