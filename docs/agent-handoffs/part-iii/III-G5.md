# III-G5 handoff — Wave 6 integration

- **Base SHA / branch:** base `c295290` (`agent/iii-f6-integration`); integration merges
  `1979d32` (G1) → `6e50f75` (G2) → `c97cee4` (G3) → `04ad6f3` (G4), plus this card's own
  doc-consistency commit.
- **Scope of this card:** merge G1–G4, resolve what merging them together exposed, gate the
  integrated tree once, give every escalation an explicit outcome. **This card does not tag
  a release** — see "Release verdict" below.

## Branch set

Confirmed against the Wave 6 board row. Four branches carried unlanded work:
`agent/iii-g1-docket-bridge`, `agent/iii-g2-chaos-audit`, `agent/iii-g3-operator-docs`,
`agent/iii-g4-ci-release`.

`git cherry` also lists `agent/iii-a1-trust`, `agent/iii-c2-runner-protocol`,
`agent/iii-c3-runner-engine` and `agent/iii-e4-run-with-agent` as "unlanded". They are
**not** — those are stale Wave 0/2/4 branches whose work landed through merge commits
(patch-ids therefore do not match). Verified by file presence in `HEAD` and by their
merge bases (`1d71785`, `422b751`, `f14019b`, `d72224a`) predating the accepted wave SHAs.
Recorded as a false positive of the ancestry check so the next integrator does not re-merge
superseded code.

## Conflicts

**None.** The four branches touched 26 files with **zero overlap** — the ownership split
held exactly. No conflict resolution was required in any of the four merges.

## What merging them together exposed

One real integration defect, invisible to any single branch:

**G3's docs contradicted G1's code.** `docs/book/src/user-guide/agent-runners.md` stated
that Docket and runner-v1 "share no code path, no table, and no auth surface." True on
G3's base — but G1 (developed in parallel, never visible to G3) landed the
one-scheduling-owner dual-dispatch guard, which *is* a shared code path. Fixed here by
quoting `LEGACY_DOCKET_COMPATIBILITY_POLICY` verbatim (which is also G1's explicit request
4 to G3) and stating the guard's one-directional asymmetry, citing the test that proves the
reverse direction open. Tables and auth surfaces remain genuinely unshared; only the
"no code path" clause was wrong.

## Escalations — every one with an outcome

| # | From | Escalation | Outcome |
|---|---|---|---|
| 1 | G1 | Mirror the dual-dispatch guard in `handlers/executions::create_execution` (Docket-active should block a new runner-v1 request) | **Routed, open.** Not owned by G5; proven-open by `g1_dual_dispatch_test.rs`, which asserts today's behavior and fails loudly when closed. Now documented in `agent-runners.md` too. |
| 2 | G1 | Wire `reconcile_stale_orch_tasks`/`_approvals` into a boot-scheduled task + `TACK_ORCH_STALE_RECONCILE_DAYS` | **Routed, open.** Same not-yet-spawned posture B3's retention sweep shipped in; wiring belongs to a `server.rs`/`config.rs` owner. Sweeps are built and tested, just not scheduled. |
| 3 | G1 | Surface `LEGACY_DOCKET_COMPATIBILITY_LABEL/_POLICY` over HTTP | **Routed, open.** No `handlers/orch.rs` DTO change made here. |
| 4 | G1 | G3 should quote the compatibility policy verbatim | **Accepted and done this card** — see "What merging them together exposed". |
| 5 | G2 | F1: stale fence returns `409 conflict` instead of `stale_lease` on 5 routes when the attempt was superseded by a pre-spawn recovery | **Accepted as documented, routed.** Nothing is written either way — the code is inconsistent with itself, not unsafe. Already recorded in `CLAUDE.md`'s boundary-rules section. Not a release blocker. |
| 6 | G2 | F2: one corrupt journal file makes the whole `unresolved()` restart scan return `Err(Malformed)` — healthy attempts still loadable by id, but the batch recovery scan cannot see past it | **Routed, open, P2.** A real availability gap; no panic, no data loss, no blind respawn. Owner: the `tack-runner` journal lineage (B3/D-series). |
| 7 | G2 | F3: `submit_events` labels a byte-identical resend ambiguously in `accepted_event_ids`/`duplicate_event_ids` | **Routed, open, P3.** Idempotency itself is proven (exactly one row regardless of resends); only the response labeling is imprecise. |
| 8 | G2 | Case 19 (disk full / `ENOSPC`, `SQLITE_FULL`) is **not verified** — no fault-injection infrastructure in this repo models it | **Accepted as not verified.** Not converted to a false green. Building a bounded-size backing store (tmpfs/loop device) is the recommended approach; the existing `RAISE(ABORT)` and `fail_next_update` hooks are analogs, not disk-full simulations. Carried into the release verdict. |
| 9 | G3 | `tack-runner`'s HTTP transport is unimplemented — `UnavailableProtocolClient` is the only `RunnerProtocolClient` in the tree; a live runner process cannot connect to a live server | **Accepted as a release blocker.** See "Release verdict". |
| 10 | G4 | `cargo audit` fails: `RUSTSEC-2026-0258` (h2) + `RUSTSEC-2026-0194`/`-0195` (quick-xml). `Cargo.lock` untouched — the advisory DB moved | **Confirmed reproducible on the integrated tree; decision deferred to the user, not silently resolved.** G5 did **not** land the `h2` bump (its lockfile diff silently downgrades five `windows-sys` entries, unverifiable without a Windows release build) and did **not** add `.cargo/audit.toml` ignores. Options are stated in the report. |
| 11 | G4 | `CLAUDE.md`'s "~10 MB" binary-size figure is stale (measured 18.34 MB) | **Moot — already resolved.** The slim `CLAUDE.md` rewrite in `c295290` dropped the figure entirely; `grep -n "MB" CLAUDE.md` returns nothing. No edit needed. |
| 12 | G4 | The packaged `tack-runner` binary builds and runs but cannot perform a real enroll/claim/heartbeat cycle | **Same finding as #9**, from the packaging side. Accepted as a release blocker. |

Neither G2 nor G3 nor G4 requested any schema, router or contract change. G1's requests are
all follow-ups, none of which blocked its own delivery.

## Gate — run once on the integrated tree

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, exit 0 |
| `cargo test --workspace` | **1315 passed / 0 failed / 7 ignored**, 78 targets (Wave 5 accepted at 1289; G1 claimed 1304 on its own branch — no count drop) |
| `runner_contract` | 18/18, all 46 fixtures byte-pinned, unchanged |
| `wave2_gate` | 5/5 |
| `openapi_contract` | 5/5, drift-free |
| `e6_scheduler_e2e_test` | 5/5 |
| `g1_dual_dispatch_test` / `g1_stale_reconcile_test` | 4/4 · 6/6 |
| `g2_chaos_security_test` / `g2_journal_corruption_test` | 8/8 · 3/3 |
| Frontend `tsc -b` · Vitest · build · token-lint | clean · **726 passed / 85 files** (Wave 5: 724; G2 added 2) · clean · 0 raw color literals, 0 inline hex |
| Playwright chromium | **65/65 passed** (43.2s), matching the Wave 5 count |
| `mdbook build` | clean, HTML written |
| `cargo audit` | **3 vulnerabilities** — see escalation 10 |
| `npm audit` | 7 (1 moderate, 6 high), all dev-tooling (redocly/js-yaml, postcss, nanoid, brace-expansion, undici). `package-lock.json` untouched since `372fa6a` — advisory-DB drift, not introduced by this wave |

**Not run / not verified, stated rather than hidden:**
- **Playwright firefox and webkit.** Webkit remains blocked by the missing
  `libwoff2dec.so.1.0.2` documented since III-E6 — unchanged, still genuinely unverified.
  Firefox was not run this pass; no route, journey or response shape changed in Wave 6
  (G1 is dispatcher/repo internals, G2 is tests, G3 is docs, G4 is CI), so chromium plus the
  unit suite was judged sufficient evidence for this diff.
- **`actionlint`** is not installed on this machine; G4 recorded 0 findings on its branch and
  the workflow files are unchanged since.
- **The live three-harness smoke, two-runner capacity/fencing, backup/restore-with-artifacts
  and Docket-absent-startup runs that III-G5's task list calls for were not performed** —
  escalation 9 makes the first of them impossible in this build, and the rest are release
  evidence that should be gathered against a runner that can actually connect.

## Release verdict — do NOT tag

Part III's definition of done requires that "from the same Tack item, an operator can create
separate attempts through Codex, Claude Code and OpenCode." **That is not true of this
build.** `crates/tack-runner/src/main.rs` wires the runtime to `UnavailableProtocolClient`,
the only `RunnerProtocolClient` in the tree, whose sole behavior is
`Err(RunnerError::ProtocolUnavailable)`; the crate does not depend on `reqwest` at all. The
gap is pinned as a deliberate typed failure by
`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success`, not hidden — but it
means the packaged runner binary cannot enroll, claim, heartbeat or report against a live
server.

Everything server-side is real: routes, scheduler, fencing, decisions, artifacts, retention,
the operator API/CLI/UI surface, and the harness-adapter code inside `tack-runner` are all
implemented and independently tested against fake protocol clients and a fake harness binary.
The missing piece is one HTTP transport implementation at the binary's entry point.

**A P0 therefore stands open, and the acceptance bar "no open P0/P1" is not met.** Tagging a
release now would ship a runner that cannot run. The next card is a `tack-runner` HTTP
transport implementation, after which the live-smoke evidence above becomes collectible and
the tag becomes honest.

## Files changed by this card

- `docs/book/src/user-guide/agent-runners.md` — Docket-compatibility section reconciled with
  G1's landed guard (integration defect above).
- `docs/agent-handoffs/part-iii/III-G5.md` — this file.
- `TODO.md` — Wave 6 status-board row.

No production code, schema, contract fixture, OpenAPI spec or generated client was touched by
this card. No gate was waived by editing its test.
