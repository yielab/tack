# Merge Readiness Pack (MRP)

The **Merge Readiness Pack** (`mrp-v1`) is a JSON document that certifies whether an attempt is ready to merge.

## Who produces it

One producer: an independent verifier, working name **Assay**, that Tack never contains. The
runner runs it after an attempt, over the attempt's evidence, and uploads the pack as an
artifact. Tack drafts this contract as the consumer because Assay does not exist yet and a
pinned fixture is the only thing both sides can test against. Assay adopts `mrp-v1` or
supersedes it with a later version (`v`); Tack reads the versions it knows and never guesses
at one it does not.

## Who reads it

1. **The attempt's pack route**, which serves a stored pack and records a human's review of it.
2. **The review panel** on an attempt, which shows the criteria, the checks and the
   recommendation to the person deciding.
3. **The pull-request body**, rendered by `tack_core::mrp::render_markdown`.

## Schema

The pack is defined by `schema.json` (JSON Schema 2020-12) and contains:

- **v**: Schema version (always "1")
- **attempt_id**: the attempt being certified, copied from the evidence's `attempt_id` (`att_…`)
- **evidence_sha256**: Hash of the evidence directory
- **brief_sha256**: Hash of the brief (null if no brief)
- **criteria**: Array of criterion evaluations (each mapped to passed|failed|manual|skipped)
- **verify**: Verifier execution details (command, exit code, output, duration), null if not run
- **mutation**: Mutation testing results (score, killed, survived), null if not run
- **static_analysis**: Static analysis findings (SARIF hash, error/warning/note counts), null if not run
- **judge**: A blind model judge's verdict (model family, rubric with verdicts per criterion), null if not run
- **risk**: Risk tier (low|medium|high) and reasons
- **recommendation**: Merge decision (merge|review|reject) and rationale
- **usage**: Verifier token consumption (tokens_in, tokens_out) — not a dollar field
- **signature**: `{"kind": "none"}` today, or an in-toto statement hash once packs are signed
- **produced_by**: Producer identification (program, version)

## Relationship to the brief

Each item in the **criteria** array maps one brief criterion (by `id` and `kind`) to a status:
- `passed`: Criterion was evaluated and passed
- `failed`: Criterion was evaluated and failed
- `manual`: Criterion requires human judgment
- `skipped`: Criterion was not evaluated (due to earlier failures, user request, or verifier failure)

A criterion's status is never "passed" to mean "not run" — the `null` value in the pack itself (for optional sections like `verify`, `mutation`, etc.) carries that meaning.

## Fixtures

- **ready.json**: An attempt that passes all criteria, has low risk, and is recommended for merge
- **not-ready.json**: An attempt with build and test failures, high risk, rejected
- **manual-only.json**: An attempt where all automated checks were skipped, awaiting manual review
- **verifier-failed.json**: An attempt where the verifier itself failed (e.g., workspace not found)

`crates/tack-core/tests/mrp_contract.rs` reads every fixture as a `MergeReadinessPack`, writes
it back byte-for-byte and pins its FNV-1a 64 hash, so a change to a fixture and to the types
lands together with its new pin.
