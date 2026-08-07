# III-C3 handoff

- Base SHA / branch / final SHA: `f14019b` / `agent/iii-c3-runner-engine` / the commit
  containing this handoff.
- Files changed (must equal ownership list): `crates/tack-runner/src/{client,journal,workspace,engine}.rs`, focused tests embedded in those owned modules, and this handoff.
- Contract fixtures consumed: enrollment, claim, heartbeat, completion, cancellation, limits,
  lifecycle and stable-error fixtures under `docs/contracts/runner-v1/`; no fixture changed.
- Behavior implemented: a typed pull-protocol seam; enrollment, claim, heartbeat,
  preparation/start, completion, cancellation and recovery-report operations; atomic owner-only
  TOML journal creation; deterministic per-attempt worktree plan/provision split; safe cleanup;
  restart recovery observation; cancellation coordination through a fake adapter; and no-retry
  stale/failed-fence quarantine.
- Tests added and exact commands/results:
  - `cargo test -p tack-runner` — 17 tests passed: atomic private journal/restart scan;
    deterministic isolated workspace; root, unresolved-path and symlink cleanup refusal;
    journal-before-provision/start; cancellation; post-spawn-before-ack ambiguity;
    cancellation-report ambiguity; stale completion fence; and restart observation.
  - `cargo clippy -p tack-runner --all-targets -- -D warnings` — passed.
  - `cargo fmt --all -- --check` — passed.
  - `git diff --check` — passed.
- Failure/adversarial case proved: once an adapter has started, a failed start ack, heartbeat,
  completion or cancellation report first preserves a quarantined journal, best-effort reports
  `RecoveryObservation::Ambiguous`, then best-effort cancels; it does not retry or report again.
  A journal intent is fsynced before worktree provisioning, and cleanup rejects a symlink before
  resolving it so it cannot delete another workspace.
- Schema/API/contract change requested from another owner: no C3 schema/API/fixture edit.
  The frozen protocol establishes only the `/api/runner/v1` base path, not per-operation route
  paths, so C3 deliberately exposes `PullProtocol` rather than inventing an HTTP route map.
  C2/C5 should supply the concrete authenticated transport. To consume B1's richer shared
  domain objects directly, the B3 dependency owner/integrator must explicitly approve adding
  `tack-orch`, `chrono`, and `serde_json` to `tack-runner`; this card was instructed not to edit
  its Cargo manifest and therefore uses narrow C3-local opaque runtime IDs at the client seam.
- Known limitations or `not_measured` fields: no HTTP implementation, credential persistence,
  real Git worktree provisioner, event/artifact streaming, or real harness adapter is included.
  `UnavailableWorktreeProvisioner` is an explicit typed failure, not an empty-directory success.
  Completion outcome carries terminal state/reason/checkpoint only until C2/C5 supplies the
  fixture-authoritative DTO transport.
- Secrets/logging review: runner and enrollment credentials redact `Debug`/`Display`; journal
  records have no credentials or complete environment/prompt values and are owner-only on Unix;
  errors/logging do not format credentials, prompts, URLs with queries, or raw event payloads.
- Safe merge order and likely conflicts: C4 should rebase runner-side crash tests onto this
  commit. C2/C5 may implement `PullProtocol` without changing engine ownership; D1–D3 implement
  `HarnessAdapter` in adapter-owned files. Do not alter root Cargo or `lib.rs` as part of C3.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Follow-up hardening

- Scope: `client`, `journal`, `workspace`, and `engine` only; no Cargo, fixtures, API, or C4
  test edits.
- Recovery delivery is now ordered: keep the unresolved local record → report the exact safe or
  ambiguous observation → move to quarantine only after the server accepts ambiguity. A failed
  report yields `RecoveryPending`, leaving evidence in the restart scan; it never becomes an
  unscanned local quarantine by itself. `ProcessRunning`, `Ambiguous`, and reconcile failure all
  use the ambiguous path. `ProcessStopped` alone may record a safe recovery observation.
- Every post-spawn journal/report/heartbeat/cancellation failure attempts ambiguous recovery and
  process cancellation. A failed post-spawn journal update cannot escape directly while a local
  process may still live.
- Journal hardening: duplicate quarantine destinations are rejected; a quarantine move syncs both
  sibling directories; a quarantined attempt blocks a later pre-spawn create; Unix temporary
  journal files are opened with mode `0600`; and symlinked state-root/journal/quarantine paths
  are rejected before use.
- Workspace hardening: an existing attempt directory symlink is rejected before provisioning,
  and cleanup requires both normal root/path checks and a matching `.tack-attempt` marker.
- Additional adversarial tests cover failed recovery delivery then retry with no respawn,
  running-process recovery quarantine, duplicate quarantined claim prevention, post-spawn
  journal-update ambiguity/cancel, directory symlinks, workspace-path symlink, and marker
  tampering.
