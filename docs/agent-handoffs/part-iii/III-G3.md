# III-G3 handoff

- **Base SHA / branch / final SHA:** base `5c6842f` (Wave 5 acceptance on
  `plan/harness-agnostic-agent-fleet`); branch `agent/iii-g3-operator-docs`; docs
  commit `9c16d30`; this handoff is the next commit on the same branch.
- **Files changed (must equal ownership list):**
  - `docs/book/src/user-guide/agent-runners.md` (new) — public runner/fleet/model
    docs
  - `docs/book/src/user-guide/recovery-runbook.md` (new) — recovery runbook
  - `docs/MIGRATION-GUIDE.md` (new) — migration guide
  - `docs/book/src/developer/crate-tour.md` (edit) — added `tack-orch`/`tack-runner`
    sections, fixed the stale "18 migrations" count
  - `docs/book/src/user-guide/backup-restore.md` (edit) — remote/cloud backup +
    execution-artifact storage, both previously undocumented
  - `docs/API-REFERENCE.md` (edit) — fixed the stale "68 REST operations across 43
    paths" count, added a pointer section to the runner/fleet/execution surface
    (deliberately not hand-duplicating 30+ endpoints already typed in
    `docs/openapi.json`)
  - `docs/book/src/introduction.md` (edit) — fixed the stale "68 REST endpoints"
    count, added a quick-link
  - `docs/book/src/user-guide/troubleshooting.md` (edit) — one cross-link to the new
    migration guide
  - `docs/book/src/SUMMARY.md` (edit) — registered the two new pages
  - `docs/agent-handoffs/part-iii/III-G3.md` (new) — this file
  - No Rust/TypeScript source, migration, router, OpenAPI, generated schema,
    `TODO.md`, other card's handoff, or CI workflow file was touched.
- **Contract fixtures consumed:** `docs/contracts/runner-v1/capabilities.json`,
  `protocol.json`, `lifecycle-transitions.json`, and the `errors/` directory —
  cited/quoted for the capability matrix, version-compatibility, and error-shape
  claims, never edited.

## Behavior implemented

Documentation only — no behavior changed. The "implementation" here is a set of
narrative pages that make existing, tested behavior discoverable, each written
against code read during this card and, where practical, against a live server run
during this card (see next section).

## Tests added and exact commands/results

No new automated tests — this is a docs card. Verification was direct execution and
citation, not new test files:

```
cargo build -p tack-cli -p tack-runner        # clean build, both binaries
mdbook build docs/book                        # clean, no warnings/errors
```

Live walkthrough performed by hand against a real server (`tack serve`, isolated
scratch DB/storage dir, `CARGO_TARGET_DIR=/home/ox/Sites/.cargo-targets/iii-g3`):

```
GET  /api/health                        → migrations_applied: 61
tack runner enroll walkthrough-runner --total-capacity 1 --available-capacity 1 --json
                                         → runner_id/token_id/enrollment_token returned
GET  /api/runners                       → runner listed, state "pending_enrollment"
POST /api/executions/nonexistent/requeue (bad recovery_key/reason)
                                         → 409 invalid_transition, {"from":"unknown","to":"queued"}
tack-runner (real enrollment token, real API URL)
                                         → "tack-runner: runner protocol client is not configured", exit 1
tack runner revoke <runner-id> --json   → {"state":"revoked"}
```

## Failure/adversarial case proved

The one genuinely adversarial finding of this card: I tried to run the actual
end-to-end path the card's docs would otherwise imply — enroll a runner, then start a
real `tack-runner` process against the live server it was enrolled against — rather
than transcribing intent from earlier handoffs. It failed immediately
(`tack-runner: runner protocol client is not configured`, exit code 1). Root cause
confirmed by reading `crates/tack-runner/src/main.rs`: the binary wires its runtime to
`UnavailableProtocolClient`, the only implementor of `RunnerProtocolClient` in the
tree, whose sole behavior is `Err(RunnerError::ProtocolUnavailable)`. The crate does
not depend on `reqwest` at all. This is pinned as a deliberate typed failure by
`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success`
(`crates/tack-runner/src/runtime.rs`), not a silent/hidden gap — but it had not been
surfaced in any prior Part III handoff or doc. Documented explicitly in
`agent-runners.md`'s "What actually runs today" section and in the crate tour's
`tack-runner` section, rather than writing an "install and run tack-runner" tutorial
that would silently fail for every reader.

## Schema/API/contract change requested from another owner

None. This card requested no schema, router, or contract change — every gap found is
a documentation/wiring gap, not a schema gap.

## Known limitations or `not_measured` fields

- `agent-runners.md` explicitly documents (does not paper over): no decision- or
  artifact-discovery/list endpoint; `model_profiles` unused by scheduling; no
  project-level default-model-policy storage; `agent_fleet_members` has no write
  route; `execution_requests` has no real `priority` column; webkit is
  `not_measured` in this sandbox (missing `libwoff2dec.so.1.0.2`, matching prior
  Wave 4/5 findings); `runner_time_cost.cost_usd_estimated` is always
  `not_measured` (no infra cost-rate is stored anywhere).
- **New finding this card:** the `tack-runner` binary's own HTTP transport
  (`RunnerProtocolClient`) is unimplemented — only `UnavailableProtocolClient`
  exists. This is the single largest gap between "what the docs on this page could
  describe" and "what a fresh-machine operator can actually do" — captured as its own
  documented section rather than an undocumented surprise. It affects only whether a
  live `tack-runner` process can connect; every server-side route, the CLI/API
  operator surface, and the harness-adapter code inside `tack-runner` itself are
  real, implemented, and independently tested (fake-harness fixture, fake
  `RunnerProtocolClient` implementations in `runtime.rs`'s own test module).
- The migration guide's "61 migrations" figure and the API reference's path/operation
  counts are stated as measured-at-time-of-writing with an explicit instruction to
  check the live/generated source (`/api/health`'s `migrations_applied`,
  `docs/openapi.json`) rather than trust the number — this doc card cannot make those
  figures self-updating, only avoid presenting them as evergreen.

## Secrets/logging review

No code changed, so no new logging surface exists. Doc content itself: no live
secret, credential, or token value appears anywhere — all example tokens
(`enr_57cd3592-...`, `runr_d91da686-...`) are truncated/illustrative values from a
throwaway local scratch server that was torn down at the end of this card's
walkthrough, matching this project's redaction posture (ids only, never full
credential values, and even the id examples are truncated with `...`).

## Safe merge order and likely conflicts

This card touches only documentation files with no overlap with any other Wave 6
card's ownership (`G1`: Docket bridge code; `G2`: adversarial test code; `G4`:
`.github/workflows/ci.yml`, packaging/release scripts; `G5`: status board and final
integration). No conflicts expected. Safe to merge independently of G1/G2/G4's
landing order; G5 should merge this after (or interleaved with) G4 so the
"broken-link" CI step in `.github/workflows/ci.yml` exercises the two new book pages
at least once before release.

## Checklist

- [x] No unowned files edited (`git diff --stat` against base `5c6842f` covers only
      the files listed above).
- [x] No live secret committed.
- [x] No panic stub / `unimplemented!()` introduced (no code changed).
- [x] No blind retry described or implied — the recovery runbook is explicit that
      only `safe_pre_spawn_requeue` is automatic, and even that is server-computed
      from a runner's self-report, never a timer-based blind requeue.

---

## Claim → test / evidence mapping

| Claim (doc, section) | Evidence |
|---|---|
| Enrollment issues a one-time token, hashed at rest | `crates/tack-api/src/handlers/runners.rs`; reproduced live (see walkthrough log above) |
| `--out` writes the enrollment response atomically, owner-only | `crates/tack-cli/src/secure_fs.rs::write_owner_only_atomic`, called from `cmd_runner_enroll` |
| `EnrollmentCredential`'s `Debug`/`Display` are hardcoded `[REDACTED]` | `crates/tack-runner/src/config.rs` (read directly) |
| Revocation is immediate; credential stops authenticating | `tack runner revoke` reproduced live, returned `{"state":"revoked"}` |
| One isolated workspace per attempt, journal written before spawn | `crates/tack-runner/src/workspace.rs`, `journal.rs` (read directly) |
| Execution artifacts live under `<TACK_STORAGE_DIR>/execution-artifacts`, apart from attachments | `crates/tack-api/src/router.rs::with_artifact_storage_root`, `execution_runtime.rs` |
| Artifact download is byte-verified end to end | `frontend/e2e/execution-attempt-detail.spec.ts` (III-F4 handoff) |
| `cancel` capability is `advisory`, never `supported`, on every in-tree adapter | `crates/tack-runner/src/harness/mod.rs::register_probe` + `harness::tests::registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists`; III-D5 handoff finding 1 |
| Every runner-v1 request requires literal `protocol_version: 1`, else `unsupported-protocol` | `crates/tack-api/src/handlers/runner_protocol.rs::check_protocol_version`; `docs/contracts/runner-v1/errors/unsupported-protocol.json` |
| `needs_operator`/`lost` are recovery-service-only transitions | `crates/tack-orch/src/execution/lifecycle.rs::validate_transition` (read directly) |
| `safe_pre_spawn_requeue` fires only when `process_stopped` + never-started + journal `prepared` | `crates/tack-db/src/repo/execution.rs::recover_attempt` (read directly) |
| `POST .../requeue` rejects a non-recovered request with `invalid_transition`, naming the state | Reproduced live: `{"code":"invalid_transition","details":{"from":"unknown","to":"queued"}}` |
| Remote backup bundle walks `storage_dir` recursively (includes execution-artifacts) | `crates/tack-api/src/remote_backup.rs::create_bundle`/`build_tar` (read directly) |
| Local `GET /api/backup` is DB-only (`VACUUM INTO`) | `crates/tack-api/src/handlers/backup.rs::get_backup` (read directly) |
| Migrations run one-per-transaction with rollback on failure; order+checksum invariant enforced at boot | `crates/tack-db/src/migrations.rs::apply_ordinary_migration`, `verify_applied_migration_invariant` (read directly) |
| Pre-upgrade `VACUUM INTO` snapshot only before a rebuild-class migration (037/038) | `crates/tack-db/src/migrations.rs::create_pre_upgrade_backup_if_needed` (read directly) |
| `TACK_EXECUTION_RETENTION_ENABLE` defaults `false`; `TACK_EXECUTION_HEALTH_ENABLE` defaults `true` | `docs/agent-handoffs/part-iii/III-F6.md` (integrator decision record) |
| `tack-runner` binary has no wired `RunnerProtocolClient` implementation | `crates/tack-runner/src/main.rs`, `client.rs::UnavailableProtocolClient`, `runtime.rs::unavailable_protocol_is_a_typed_failure_not_success`; reproduced live (walkthrough log above) |
| `61` migrations applied in this build | `GET /api/health` reproduced live: `"migrations_applied":61` |
| `90` OpenAPI paths / `124` operations | `python3 -c "json.load(open('docs/openapi.json'))"`, counted directly |

## Proposed status-board row text (G5 to apply, not applied here)

> **III-G3 — complete.** Operator/migration/recovery docs delivered:
> `agent-runners.md` (install/enroll/revoke, credentials, workspace/artifact storage,
> capability matrix, version compatibility, Docket relationship, non-loopback
> security), `recovery-runbook.md` (`needs_operator`/lease-recovery mechanics),
> `MIGRATION-GUIDE.md` (schema upgrade + execution-feature enablement gates),
> `crate-tour.md` updated with `tack-orch`/`tack-runner`, `backup-restore.md`
> extended with remote/cloud backup and execution-artifact coverage, stale
> hand-written counts fixed in `API-REFERENCE.md`/`introduction.md`/`crate-tour.md`.
> Every public claim on the new pages cites a test path or a live-server
> reproduction performed during this card (mapping table in `III-G3.md`).
> **Genuinely new finding, not previously documented anywhere in Part III:** a live
> fresh-machine walkthrough found `tack-runner`'s own HTTP transport unimplemented
> (`UnavailableProtocolClient` is the only `RunnerProtocolClient` in the tree) — a
> running `tack-runner` process cannot connect to a live Tack server in this build.
> Documented plainly rather than assumed away; does not block this card's docs
> acceptance (which describes what exists honestly) but is a release blocker for
> Part III's definition of done ("an operator can create separate attempts through
> Codex, Claude Code and OpenCode") and should be triaged before G5's final
> integration claims that goal met.
