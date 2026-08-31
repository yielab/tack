# IV-A6 handoff

- Base SHA / branch / final SHA: base `c2a78b6ab61d3bb27b41ceae78be334ec9d43951` (develop
  tip at worktree creation, includes IV-A1 through IV-A5), branch
  `agent/iv-a6-standalone-smoke-docs`, final SHA — see the last commit on this branch.
- Files changed (must equal ownership list): `scripts/smoke.sh`, `docs/CONFIG.md`,
  `docs/book/src/user-guide/agent-runners.md`, `README.md`,
  `docs/agent-handoffs/part-iv/IV-A6.md`. Nothing else touched — no `.rs` file in this
  diff; `cargo build --workspace` after every doc/script edit finished in ~0.2s (a
  no-op), confirming no compiled crate changed.
- Contract fixtures consumed: none. This card never touches `docs/contracts/runner-v1/`,
  a handler, or the OpenAPI surface — it documents and smoke-tests behavior IV-A1
  through IV-A5 already implemented.

## Behavior implemented

**Task 1 — `scripts/smoke.sh` step 10 (standalone mode).** A fresh state directory,
fresh database, one `tack serve --with-runner` process on its own port (`PORT + 1`),
using the same fake-harness shim pattern (`$SHIMS`) the existing steps already use.
Asserts, in order: the server comes up; an embedded runner self-provisions and reaches
`active` with a heartbeat (`GET /api/runners`, no `tack runner enroll` call anywhere in
the step); a project/item/agent profile can be created against it; a real execution
request is queued against the self-provisioned runner using the model combination it
declared at enrollment (same pairing logic step 7 already uses); the resulting attempt
reaches `state: succeeded`. `create_execution`/`attempts_json` read `$API`/`$AGENT_PROFILE`
as globals, so the step swaps them to the standalone server for the duration of the
block and restores the originals immediately after, so no later step can address the
standalone server by accident.

**Task 2 — step 11 (off-by-default, queried not inferred).** A separate `tack serve`
with no flag and no `TACK_LOCAL_RUNNER_ENABLE`, on its own port, its own fresh database.
After a 4-second settle window (long enough for a wrongly-started self-provisioning
runner to have appeared and heartbeat at least once), `GET /api/runners` is queried
directly and asserted empty. This is the direct-query proof the card asked for, not a
"no log line appeared" inference.

**Task 3 — step 12 (non-loopback refusal).** `TACK_HOST=0.0.0.0` plus `--with-runner`
plus a real `TACK_API_TOKEN` (set specifically so the failure can only be the
`--with-runner`-specific loopback guard, not the unrelated ADR-0059
unauthenticated-non-loopback refusal, which never gets a chance to run anyway since
`ensure_loopback` fires first — see `crates/tack-cli/src/local_runner.rs`). Asserts:
non-zero exit, stderr contains "loopback", and — separately — that no TCP listener was
ever opened on the configured port (`curl` connect attempt against `127.0.0.1:<port>`
fails after the refusal).

**Task 4 — `docs/CONFIG.md`.**
- Added `TACK_LOCAL_RUNNER_ENABLE` to the main table, positioned next to
  `TACK_ORCH_ENABLE` (the other off-by-default subsystem gate). Documents the exact
  semantics read from `crates/tack-cli/src/local_runner.rs::with_runner_enabled`: `1`/
  `true` (case-insensitive) enable it, `--with-runner` is equivalent, off by default,
  read by the `tack` CLI binary rather than `AppConfig` (no `tack.toml` reader for it —
  ADR 0058 says this is deliberate).
- New "Embedded runner (`tack serve --with-runner`)" section (placed after the existing
  tables and their surrounding prose, before `## Debugging`) covering: the gate and its
  loopback rule; the state directory (`TACK_RUNNER_STATE_DIR`, default `.tack-runner`)
  and what's owner-only about it (`session.json` mode `0600`, journal entries
  `0700`/`0600`, per `journal.rs`/`transport.rs`); a dedicated "provider credentials"
  subsection mirroring a real `tack runner doctor` run's own per-harness credential
  notes verbatim rather than re-deriving them, plus the OpenRouter/local-model-endpoint
  and "Tack is never a model gateway" statement citing ADR 0050 and ADR 0058 by name;
  and the RUST_LOG log-visibility fix (below).
- One more `RUST_LOG=...` line added to the existing `## Debugging` bash block for
  discoverability from where operators already look for logging recipes.

**Task 5 — `docs/book/src/user-guide/agent-runners.md`.** Read in full (383 lines)
before editing, per the card's instruction. Two changes:
1. New top-level section "## Standalone mode: `tack serve --with-runner`", inserted
   after "### Revoking a runner" (the end of the existing "## Enrolling a runner"
   block) and before "## Local credential handling" — covers the same ground as
   `docs/CONFIG.md`'s new section but from the operator-narrative angle this page
   already uses elsewhere, with citations to the specific tests/smoke steps that prove
   each claim (`embedded_runner_refuses_non_loopback_bind`, smoke steps 10/11/12).
2. **A pre-existing staleness fix, not requested verbatim by the card but necessary for
   internal consistency of the file I now own for this card:** the page's closing
   "## What actually runs today" section stated "the runner-v1 protocol client... does
   not exist yet" and that `main.rs` wires `UnavailableProtocolClient`. That was true
   when the section was last touched (`2ca2f8c`, 2026-08-19, III-G5) but has been false
   since IV-A1 (`bootstrap::build_runtime` wires the real `HttpPullProtocol`); leaving
   it would have directly contradicted the new standalone section a few hundred lines
   above it in the same page. Verified before editing:
   `grep -rn UnavailableProtocolClient crates/tack-runner/src/` shows it still exists
   as an explicit no-client fallback (`client.rs`, pinned by
   `runtime::tests::unavailable_protocol_is_a_typed_failure_not_success`) but is not
   what `bootstrap.rs` wires. Rewrote the paragraph to state the client is wired and
   proven, citing `bootstrap_entrypoint.rs`, `scripts/smoke.sh` steps 6-10, and
   `docs/agent-handoffs/part-iv/` (IV-A1 through this card) — no new claim beyond what
   those sources already prove. Also added one cross-reference sentence to the existing
   "## Non-loopback and security posture" section noting `--with-runner`'s own, stricter
   loopback gate.

**Task 6 — `README.md`.** Read in full (337 lines) first, per the card's instruction.
The "## Run it" section's lead command for every install method (Cargo, one-line
installer, release archive, Windows) changed from bare `tack` to
`tack serve --with-runner`, with one paragraph after the Cargo block explaining what the
flag buys (self-provisioned runner, no second binary, no token) and noting bare `tack`
still starts just the server/board UI. No other README section touched — V-A4's
positioning argument (the opening paragraphs, the harness-status table, the delivery
tables) is unchanged; this only slots the standalone command into the existing
"Run it" structure, as the card asked.

## Tests added and exact commands/results

- `scripts/smoke.sh` steps 10-12 (new). Full run, fake mode, this branch:
  `cd /var/tmp/tack-agent-worktrees/IV-A6 && ./scripts/smoke.sh` — exit `0`,
  `SMOKE PASSED — fake shim harnesses, pipeline real`, 36 `PASS` lines across all 12
  steps, 0 `FAIL`. Ran the full script at least four times over the course of this
  card (initial clean pass, the load-bearing break, the load-bearing restore, and a
  final pass after all doc edits) — every clean run reproduces this result; only
  generated ids differ between runs, matching the pattern IV-A1's own before/after
  table already established for this script.
- `cargo build -p tack-cli -p tack-runner` (smoke.sh step 2, every run) → builds clean.
- `cargo build --workspace` (standalone, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/IV-A6`,
  run once after all edits were complete) → `Finished ... in 0.21s` — a no-op rebuild,
  confirming this diff touches no compiled crate.
- `bash -n scripts/smoke.sh` → clean (syntax check after every edit).

## Failure/adversarial case proved (Task 1's load-bearing requirement)

**Exact break-it method:** temporarily changed one line in step 10,
`SA_DB_URL="sqlite:$SA_WORK/tack.db?mode=rwc"` →
`SA_DB_URL="sqlite:/nonexistent-dir-for-smoke-break/tack.db?mode=rwc"` — pointing the
standalone server's own database at a path whose parent directory does not exist and
is never created, so SQLite's `mode=rwc` cannot open it. This is a direct edit to the
committed script, not an environment trick invisible to a reader of the diff, so the
exact break is reproducible from this handoff alone.

**Observed result:** ran `./scripts/smoke.sh`. Step 10 reported:
```
[FAIL] standalone server never came up
  | Error: error returned from database: (code: 14) unable to open database file
[FAIL] no embedded runner reached active — standalone mode never got off the ground
[FAIL] could not set up the standalone project/item/agent profile
```
Steps 1-9, 11, 12 were unaffected and still `PASS` (proving the break was scoped to
step 10 only, not a global fault). Overall result:
`SMOKE FAILED — fake shim harnesses, pipeline real; see the failing step above`, process
exit code `1` (captured directly: `./scripts/smoke.sh; echo "EXIT CODE: $?"` →
`EXIT CODE: 1`). No `SKIPPED` output appeared anywhere — the step reported a real `FAIL`
against a real, product-shaped break, which is the exact defense the card asked for
against this file's prior false-green history.

**Restored:** reverted the line to `SA_DB_URL="sqlite:$SA_WORK/tack.db?mode=rwc"`, reran
`./scripts/smoke.sh` → exit `0`, all 12 steps `PASS` including step 10's
`PROOF: standalone mode reached a real completed attempt`.

Tasks 2 and 3's assertions are direct queries/process-level checks (`GET /api/runners`
emptiness, process exit code + stderr text + a connect attempt against the refused
port) rather than log-line inference, per the card's own instruction — see "Loopback/gating
proof" below for exactly which run demonstrated each.

## RUST_LOG log-visibility gap — verified myself, fix documented

Per the card's instruction not to just assert IV-A4's finding, I reproduced it
independently before writing anything down. Built `tack`/`tack-runner` fresh
(`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/IV-A6`), ran `tack serve --with-runner`
on a fresh state directory twice, once under default logging (`RUST_LOG`/`TACK_LOG_LEVEL`
unset) and once with `RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info`:

- **Default logging:** `grep -c tack_runner default.log` → `0`;
  `grep -c "tack::local" default.log` → `0`. The server-side handler line
  (`tack_api::handlers::runner_protocol`'s own `runner enrolled runner_id=...`, target
  `tack_api`) *does* appear, since `tack_api` is in the default filter — but nothing
  from the runner's own code (`tack_runner::*`) or the CLI's embedding code
  (`tack::local_enrollment`, `tack::local_runner`) does. Root cause confirmed by reading
  `crates/tack-api/src/server.rs::init_tracing`: the fallback `EnvFilter` is the literal
  string `"tack_api={level},tack_db={level},tack_core={level},tower_http=debug"` —
  `TACK_LOG_LEVEL` only changes `{level}` for those three named crates; it cannot add a
  target the string never names, so raising `TACK_LOG_LEVEL` alone cannot fix this.
- **With the `RUST_LOG` override:** the same run produced
  `tack::local_enrollment: ... self-provisioned a local runner ...`,
  `tack_runner::runtime: runner runtime started ...`, and
  `tack_runner::client::transport: runner enrolled ...` — all three previously-silent
  targets now visible.

Documented in `docs/CONFIG.md`'s new "Embedded runner" section (the "Log visibility"
bullet) and cross-referenced from `## Debugging`, and mentioned briefly in
`agent-runners.md`'s new standalone section pointing back to `docs/CONFIG.md` for the
exact setting — not duplicated as a second source of truth.

## Schema/API/contract change requested from another owner

None. This card touches no `.rs` file, no contract fixture, no OpenAPI surface.

## Known limitations or `not_measured` fields

- Binary-size delta: **N/A — no compiled code changed.** `cargo build --workspace`
  after this diff is a ~0.2s no-op; there is nothing to measure.
- The two known gaps the card flagged going in: gap 1 (the `build_runtime` placeholder
  credential precondition) is explicitly **not** documented as a limitation anywhere in
  this diff, per the card's own instruction that it has no observable effect. Gap 2 (the
  RUST_LOG visibility issue) is documented and its fix verified live, above.
- `docs/book/src/user-guide/agent-runners.md`'s "What actually runs today" section was
  corrected as part of this diff (see Task 5 above) because it directly contradicted
  the new content this card added to the same file; this went slightly beyond a literal
  reading of the card's Task 5 instruction ("alongside whatever it currently
  documents"), but stayed within the file this card already owns and touched no
  compiled code — flagged here explicitly rather than left silent.

## Secrets/logging review

No new logging or credential-handling code was added anywhere (no `.rs` file in this
diff). The new smoke steps reuse the existing enrollment/credential machinery exactly
as steps 3-9 already do — no raw token or credential value is echoed by any new `ok`/
`bad`/`note` call; only ids (`runner_id`, `request_id`, `attempt_id`) are printed,
matching the existing steps' convention. The new docs sections describe credential
*mechanisms* (OAuth session locations, env-var conventions, hashed-at-rest storage) —
mirrored from `tack runner doctor`'s own static, hardcoded prose and from
`docs/CONFIG.md`'s pre-existing redaction language — never a literal secret value from
this machine.

## Safe merge order and likely conflicts

No conflicts expected. This card's entire diff is four files (`scripts/smoke.sh`,
`docs/CONFIG.md`, `docs/book/src/user-guide/agent-runners.md`, `README.md`) plus this
handoff — exactly its ownership list. It is the last card of Part IV Wave 10; no other
Part IV card is in flight. The cross-Part note in the card text (V-A2/V-A4 already
landed on `develop` touching `scripts/smoke.sh`/`README.md` weeks ago) was read and
built on top of, not re-litigated — the false-green pattern V-A2 fixed was not
reintroduced (every new step's assertions are direct queries/process checks that can
fail, proved above), and the "Run it" edit slots into V-A4's existing structure without
touching its positioning prose.

## Checklist

- No unowned files touched: exactly `scripts/smoke.sh`, `docs/CONFIG.md`,
  `docs/book/src/user-guide/agent-runners.md`, `README.md`, this handoff. `TODO.md` and
  `docs/book/src/roadmap.md` were not touched, per the card's explicit instruction that
  the wave integrator updates those.
- No live secret introduced or logged: see "Secrets/logging review" above.
- No panic stub: no `.rs` file in this diff; the new bash steps use the same
  `ok`/`bad`/`wait_for` idioms as the existing 9 steps, no new `set -e`/trap logic.
- No blind retry: no retry loop added anywhere in this diff.

## Part IV additions

- **Binary-size delta:** N/A — no compiled code changed (see "Known limitations" above).
- **Which role executed what (live runs in this card):** every live run in this card
  used **either the embedded role** (`tack serve --with-runner`, smoke step 10 and the
  RUST_LOG verification runs — one process, one binary, self-provisioned runner
  claiming and completing its own attempts) **or no runner role at all** (steps 11's
  plain `tack serve`, and step 12's refused `--with-runner` start, which never reaches
  the point of starting any role). No standalone `tack-runner`/`tack runner start`
  process was exercised by this card's own new work — that shape was already proven by
  IV-A3's and earlier smoke steps' (6-9) own separately-enrolled runner, unchanged by
  this diff.
- **Loopback/gating proof (named explicitly):**
  - Off-by-default: `scripts/smoke.sh` step 11, live — plain `tack serve` (no flag, no
    `TACK_LOCAL_RUNNER_ENABLE`), `GET /api/runners` queried directly after a 4-second
    settle window, asserted empty.
  - Non-loopback refusal: `scripts/smoke.sh` step 12, live — `TACK_HOST=0.0.0.0` plus
    `--with-runner` exits non-zero with a "loopback"-naming stderr message, and a
    subsequent connect attempt against the configured port fails (no listener was ever
    opened). Both are the same `ensure_loopback`/`with_runner_enabled` mechanisms
    IV-A3's own unit tests (`embedded_runner_refuses_non_loopback_bind`,
    `with_runner_enabled_reads_the_environment_gate`) already pin at the unit level;
    this card adds the live, process-level demonstration inside the standing smoke
    suite rather than a one-off proof script, so it now runs on every future smoke pass.
