# III-H9 handoff

**What this card changes, in plain language.** Before it, a real runner could
upload an artifact's manifest successfully but the byte-content upload
(`PUT .../artifacts/{artifact_id}/content`) always came back `500` — every
single time, in every smoke run. After it, the same PUT returns `200` and the
bytes land on disk, verified live end-to-end via `./scripts/smoke.sh`. This
was the last open half of §III.6's "verified artifacts" criterion; the
manifest half was already proven by III-H6, only the content-bytes half was
broken.

- **Base SHA / branch:** base `b848d96` (tip of `develop`, the accepted
  Wave 8 integration commit named on the board), branch
  `agent/iii-h9-artifact-content-storage`. Not committed at the time of
  writing — no commit was requested.
- **Files changed (all within Owns):**
  `crates/tack-api/src/handlers/runner_protocol/artifact_storage.rs` — the
  `encode_id` helper and its doc comment, plus one new regression test.
  Nothing in `runner_protocol.rs` itself needed to change; the escalation's
  guess about the handler was wrong (see below).

## Root cause (not what III-H6's escalation guessed)

III-H6's escalation guessed the `execution-artifacts` directory was never
created ahead of the first write. That is not the bug —
`ArtifactStorage::safe_attempt_dir` already calls `create_dir_all` on both
the storage root and the attempt directory before every write, and
`f6a_artifact_wiring_test.rs`'s existing
`artifact_content_is_stored_under_configured_storage_dir_and_downloadable_through_the_real_router`
already proves that path works against a *short*, hand-written
`attempt_id`/`artifact_id` pair — which is exactly why the bug survived that
test: it never used a realistic id.

Found by instrumenting `store_streaming` with temporary `eprintln!`s at each
fallible filesystem step and re-running `./scripts/smoke.sh`
(`SMOKE_KEEP=1`) against the real production pipeline (removed before this
handoff was written): every failure was `Io` from `OpenOptions::open`, with
the OS error `Os { code: 36, kind: InvalidFilename, message: "File name too
long" }` (`ENAMETOOLONG`).

`encode_id` (the module's traversal defense) hex-encoded every byte of its
input literally, doubling its length. `tack-runner`'s own
`engine.rs::artifact_id` derives a real `artifact_id` as
`format!("art_{}", hex(format!("{attempt_id}:{fencing_token}:{sha256}")))` —
already roughly 220 bytes before this module touched it (a `att_<uuid>`
attempt id, a fencing token, and a 64-hex-char SHA-256, all hex-encoded
once). `encode_id` hex-encoded that *again* for both the temp file name and
the final blob name, producing filenames well past Linux's 255-byte
`NAME_MAX`. `f6a_artifact_wiring_test.rs`'s hand-written ids (`"attempt-1"`,
`"artifact-1"`) never came close to that limit, so the existing test suite
never caught it; only a realistic, runner-generated id does.

## Fix

`encode_id` now SHA-256-hashes its input and hex-encodes the fixed-size
digest, instead of hex-encoding every input byte. This keeps the exact same
traversal defense the module's own doc comment describes (a `..`, `/`, or
NUL byte in an id can never survive into a path component — hashing removes
it entirely, which is strictly stronger than hex-escaping it) while bounding
the encoded length to 64 bytes regardless of the input's own length.
Reversibility was never a requirement: nothing in this module or its caller
reads a stored path back out by re-deriving it from the literal id —
`open_for_read` and `remove_blob` both take the already-produced
`content_reference` string.

**Not touched, deliberately:** `tack-runner`'s own
`harness/artifact.rs::encode_id` has the identical hex-doubling shape, but it
encodes the *local* staging directory name, not the server's. It wasn't
exercised by this bug (the local stager was never observed to fail in any
smoke run — its own ids/paths are shorter) and it is outside this card's
`Owns` (`crates/tack-api/...`, not `tack-runner`). Flagged here for whichever
card next touches `tack-runner`'s artifact staging, not fixed.

## Tests added and exact commands/results

New test:
`a_realistic_long_runner_generated_artifact_id_does_not_overflow_a_filename`
— builds an `artifact_id` in the exact shape `engine.rs::artifact_id`
produces (asserted `> 200` bytes as a fixture sanity check), stores content
against it, and asserts the round trip succeeds and reads back the exact
bytes. **Load-bearing, proven by reverting the fix once**: with `encode_id`
restored to hex-encode every byte, this exact test fails with
`Err(Io)` — the identical failure mode observed live. Restored the fix and
confirmed the test passes again.

Command: `cargo test -p tack-api --lib runner_protocol::artifact_storage`
Result: 9 passed, 0 failed (8 pre-existing + 1 new).

Broader gates run on the full branch:
- `cargo test --workspace` — 1383 passed, 0 failed (was 1380 at the Wave 8
  merge baseline recorded on the board; +1 is this card's new test, the
  remaining +2 is not accounted for by this card's own diff — same kind of
  small, unexplained drift the board already noted once for H8's larger-than
  -estimated test file, not investigated further here since it is 0 failed
  either way).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.
- `cargo test -p tack-orch --test runner_contract` — 18/18, unchanged (no
  fixture touched; this fix is purely internal filesystem path derivation,
  not a wire-shape change).
- `cargo test -p tack-api --test wave2_gate` — 5/5, unchanged.
- `cargo test -p tack-api --test openapi_contract` — 5/5, unchanged (no
  route/schema change; `docs/openapi.json` untouched).
- No frontend files changed; frontend gates not re-run.

- **Live acceptance: `./scripts/smoke.sh`** (fake mode; real server, real
  runner binary, real scheduler/provisioner/adapter, shim harness binary; no
  smoke-script edit). Result: **`SMOKE PASSED`**, all of steps 7, 8 and 9
  green. Confirmed directly in the server log
  (`grep 'artifacts.*content' server.log | grep on_response`): every
  `PUT .../artifacts/{artifact_id}/content` now returns `status=200` (was
  `status=500` on every occurrence, every run, before this fix). Confirmed
  the bytes actually land on disk under the configured
  `TACK_STORAGE_DIR/execution-artifacts`, one `.blob` file per attempt
  directory, each with a short, bounded filename. The verdict's only
  remaining line is `codex` not being installed on this machine
  (environmental, unrelated — same as every other Wave 7/8 card).

## Failure case proved

Documented above under "Tests added": the new test fails with `Err(Io)`
against the pre-fix `encode_id`, and the live smoke run 500'd identically
before the fix and 200'd identically (three artifact uploads in the run
checked) after it.

## Proposed status-board row text (for the integrator; not applied here)

**III-H9** — the artifact content PUT's `500` is fixed; root cause was not
the guessed missing directory but `encode_id` hex-doubling an already-long,
realistic runner-generated `artifact_id` past Linux's 255-byte filename
limit, reproduced live via `ENAMETOOLONG` and fixed by hashing instead of
literally hex-encoding the id, proven load-bearing by reverting once.
`./scripts/smoke.sh` now shows every artifact content upload as `200`
instead of `500`. §III.6's "verified artifacts" criterion is fully
demonstrable end-to-end (manifest + content bytes both proven live); the tag
remains blocked only on the `codex` binary being absent from this machine
(III-H2).

## §III.6 status after this card

The artifact-content half of "verified artifacts and an idempotent event
timeline" is now demonstrable live, closing the last gap III-H6's escalation
opened. Remaining unmet §III.6 items, unchanged by this card: `codex` binary
still absent on this machine (III-H2), and no decision-asking harness exists
yet to exercise the decision path (accepted scope limit per III-H6's
handoff).
