# V-B2 handoff

- Base SHA / branch / final SHA: base `81e66e51e8d6b704165a989cb2a2957bb28ebde5` on
  `agent/v-b2-docket-fate`; final SHA recorded in the commit that carries this file.
- Files changed (must equal ownership list): `docs/adr/0060-docket-control-plane-disposition.md`
  (new), `docs/agent-handoffs/part-v/V-B2.md` (new). No code, schema, or `docs/CONFIG.md`
  change — the ADR's decision is **keep**, and task 3 is explicit that gating code is only
  written when the decision is **gate**. `crates/tack-orch/src/adapters/**` and the frontend
  UI routes named in this card's ownership are unmodified in the final diff.
- Contract fixtures consumed: none — `docs/contracts/runner-v1/` is unaffected by this
  decision (stated in the ADR's header).
- Behavior implemented: none. This card decides and gates; the decision was **keep**, so
  no gating was implemented (per task 3's own instruction not to write gating code unless
  the decision is gate).
- Tests added and exact commands/results: none added. Verification run against the
  unmodified tree:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B2 cargo fmt --check` — clean, no output.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B2 cargo clippy --workspace --all-targets -- -D warnings` — clean, `Finished` with zero warnings/errors.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B2 cargo test --workspace` — **1,385 passed, 0 failed, 5 ignored** across 80 test binaries, single feature state (no gating shipped, so no second state to test — the acceptance criterion's "both feature states" clause is conditional on gating shipping).
  - No frontend files were touched, so `npm run type-check` was not required and was not run.
  - A throwaway migration-probe test (`crates/tack-db/tests/zz_probe_schema_test.rs`,
    written, run once via `cargo test -p tack-db --test zz_probe_schema_test -- --nocapture`,
    then deleted) established the ADR's schema numbers by migrating a fresh in-memory DB and
    reading `sqlite_master` directly — not left in the tree.
- Failure/adversarial case proved: n/a — no behavior shipped. The one adversarial check
  this card asked for (measure the binary-size delta with the adapter compiled out) was
  done as a **reverted experiment**: `crates/tack-orch/src/adapters/mod.rs`'s
  `docket`/`github_actions`/`prometheus` module lines and `registry.rs`'s `"docket"` match
  arm were commented out, `cargo build --release -p tack-cli` was run before (baseline,
  18,622,360 bytes) and after (18,479,256 bytes; delta 143,104 bytes / 0.77%), then
  `git checkout -- crates/tack-orch/src/adapters/mod.rs crates/tack-orch/src/adapters/registry.rs`
  restored the tree — confirmed clean via `git status --porcelain` before writing this
  handoff. See the ADR's Measurement section for the full methodology and the reasoning
  (`reqwest`/TLS is not removable this way; `tack-api`'s webhook/github-sync/import code
  depends on it unconditionally) behind why the delta is small.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - Frontend SPA bundle-size delta from removing docket UI is explicitly `not_measured` —
    the binary-size experiment above omitted `--features embed-spa` (no `frontend/dist/`
    was built) specifically to isolate the Rust-only adapter delta from frontend size, per
    the card's own fallback instruction ("if you run out of time, record what you could
    measure... do not invent a number").
  - The ADR documents that the "two fleet concepts" UI confusion this card was raised to
    resolve is real and is **not** solved by the keep decision — it is a UI-visibility gap
    (Sidebar always renders Fleet/Approvals/Economics/Provision; only Fleet gets an "Off"
    badge). The ADR's Consequences section recommends a properly-scoped future card to fix
    it via the existing `orchAvailable()`/`isOrchDisabled()` runtime pattern rather than a
    new build-time flag — not executed here, by design (deletion/gating execution is out of
    this card's scope; keep needed none).
  - The ADR also surfaces, as a finding rather than something this card fixes, that
    `Makefile`, `docs/DEPLOYMENT-GUIDE.md`, and `.github/workflows/release.yml` all hardcode
    `--features embed-spa` with no other flag — relevant only if a future card implements
    the default-off cargo feature this ADR declined to implement now.
- Secrets/logging review: n/a — no code touched. The reverted experiment never ran the
  binary or touched credential/token paths, only `cargo build`.
- Safe merge order and likely conflicts: additive-only (two new files under `docs/`); no
  conflict expected with V-B1 (auth rows in `docs/CONFIG.md` — this card added no rows
  there) or with any other Part IV/V card. ADR number `0060` was reserved for this card in
  the card brief (`0059` is claimed by the sibling V-B1 agent); confirmed free at start
  (`docs/adr/` held only `0008`, `0050`, `0058`) and still free of collision at the time of
  this commit.
- Checklist: no unowned files (only the ADR and this handoff were added); no live secret;
  no panic stub; no blind retry.
