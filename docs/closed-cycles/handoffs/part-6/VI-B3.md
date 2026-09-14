# VI-B3 handoff

- Base SHA / branch / final SHA: base `f2bb5bd` (the `develop` tip named directly by the
  dispatching agent, overriding the dispatch README's Wave-15 row, which still names a
  stale `2958e9e` — VI-B1/VI-B2 are already merged into `f2bb5bd`); branch
  `agent/vi-b3-embedded-control`; committed on that branch (final SHA in `git log -1` on
  the branch at handoff time).
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/local_runner.rs`
  (new — the `LocalRunnerControl` trait, its types, the five route handlers, the
  `app_meta`-backed enable preference); its mounts in `crates/tack-api/src/router.rs`
  (`AppState::local_runner`, `local_runner_routes`, the loopback+wired-in gate in
  `build_router`); the server→embedded-runner seam, rewritten, in
  `crates/tack-cli/src/local_runner.rs` (`EmbeddedRunnerControl`, replacing
  `serve_with_embedded_runner`); `frontend/src/features/agents/{api.ts,ExecutionToggle.tsx,
  ProviderKeyPanel.tsx}` with their tests. Beyond the literal wording, unavoidable to wire
  the seam through — see "Files changed vs. ownership" below: `crates/tack-api/src/config.rs`
  (`local_runner_enable` field), `crates/tack-api/src/server.rs` (the two new `serve_*`
  entry points and the boot-time auto-start check), `crates/tack-api/src/lib.rs` /
  `handlers.rs` / `openapi.rs` (module registration + spec), `crates/tack-api/src/
  orch_store.rs` (one new field on a reconstructed `AppState`), 23 `tests/**` files plus
  `tests/common/mod.rs` (the new `AppState` field, mechanical), `docs/openapi.json` /
  `frontend/src/shared/api/schema.gen.ts` (regenerated), `docs/CONFIG.md` (two rows),
  `crates/tack-cli/{Cargo.toml,src/main.rs}` (the new dependency, `run_server` unified to
  one code path), `frontend/src/app/routes.tsx` (`/agents` route),
  `frontend/src/features/agents/AgentsPage.tsx` (new — a minimal page to actually mount the
  two panels on; VI-C1 owns the real one), `.gitignore` (the e2e webServer's new
  `.tack-runner/` state dir).
- Contract fixtures consumed: none read or edited. `runner_contract` byte-identical
  (18/18, `git status --porcelain docs/contracts/ crates/tack-orch/tests/runner_contract.rs`
  → empty).
- Behavior implemented: `PUT`/`GET /api/local-runner` (on/off + runtime state + catalog
  snapshot) and `GET`/`PUT`/`DELETE /api/local-runner/secrets(/{name})`, all absent (404) on
  a non-loopback bind or when the embedding process never wired a runner in; the persisted
  on/off preference (`app_meta`, same precedence as `TACK_ORCH_ENABLE`); a UI-only Vercel
  key write that also flips that one provider on; a re-probe that is really "every catalog
  read is uncached", so there is nothing to invalidate; `ExecutionToggle`/`ProviderKeyPanel`
  panels plus a minimal `/agents` page to mount them on.
- Tests added and exact commands/results: see "Measured numbers".
- Failure/adversarial case proved: a boot-time-only regression I introduced and then caught
  myself — see "A bug I introduced and fixed: auto-start on a non-loopback bind" below,
  with the revert-and-watch-it-fail proof.
- Schema/API/contract change requested from another owner: none. (One narrow limitation
  handed forward to VI-C1/future work: see "What a stranger still cannot do.")
- Known limitations or `not_measured` fields: see "Not checked" below.
- Secrets/logging review: see "Secret-path proof".
- Safe merge order and likely conflicts: this is the last Wave-15 card besides VI-B4 (parallel,
  disjoint files — provider.rs/registry/doctor.rs's catalog half, none of which this card
  touched) and VI-B5 (after VI-B4). No file in this diff is claimed by VI-B4's ownership list.
  `crates/tack-cli/src/doctor.rs` was **not** touched by this card (VI-B4 owns its catalog
  half; this card never needed it — see "Design decisions" below for why `reprobe` didn't
  need `doctor.rs` at all). The 24 mechanical `AppState` literal edits are a single
  find-and-insert-after-one-line pattern; if VI-B4 or another concurrent card also grew
  `AppState`, the two diffs touch adjacent lines in the same literals and merge cleanly by
  hand.
- Checklist: no unowned files edited without justification below; no live secret committed
  (a stray real Vercel AI Gateway key was found in this dev machine's keychain during
  testing — see "A pre-existing stray secret, not from this card" — cleared, not committed
  anywhere, never printed by anything this card wrote); no panic stub; no blind retry.

## A bug I introduced and fixed: auto-start on a non-loopback bind

Rewriting `serve_with_embedded_runner` into `EmbeddedRunnerControl` plus a boot-time
auto-start check (`server.rs::serve_inner`), I first wrote the auto-start condition as
`if let Some(control) = &local_runner && effective_local_runner_enabled(...)` — no loopback
check. That is wrong: a preference saved as "on" from an earlier loopback session, on a
server later bound to `0.0.0.0`, would have silently started the embedded runner on a
non-loopback bind — the exact thing ADR 0061 decision 6 exists to prevent, and worse than
the route just being unreachable, because the runner itself would still be live and
executing arbitrary agent processes.

Caught before this went into the diff I gate: added
`state_for_local_runner.config.binds_loopback()` to the condition, then proved it
load-bearing by reverting the one line and re-running the new test
(`cargo nextest run --workspace -E 'test(a_persisted_enable_preference_never_auto_starts)'`)
— it failed with exactly the expected symptom (`RecordingControl::start` was called on a
`0.0.0.0` bind), then passed again once restored. `local_runner.rs`'s own top-level
`ensure_loopback` call (unchanged, still refuses to *boot* when `--with-runner`/
`TACK_LOCAL_RUNNER_ENABLE` was set for this exact process on a non-loopback bind) is now
conditional on `server_config.local_runner_enable` rather than unconditional — a plain
`tack serve` on `0.0.0.0` with no such flag must still boot normally, since that was true
before this card and nothing in the card asked to change it.

## Design decisions worth a second look

- **"Re-probe" needed no new entry point into the runtime.** `catalog()` computes fresh on
  every call (`bootstrap::probe`, the same function `tack runner doctor` already calls, no
  caching anywhere in this card's own code) — mirroring `orch_settings_view`'s documented
  "never cached" rule. `put_local_runner_secret` still calls `catalog()` once, explicitly,
  so the re-probe is something this route visibly *does*, not an accident of whichever GET
  happens next. This is why `doctor.rs` (VI-B4's file) was never touched.
- **Setting the default Vercel secret also flips that provider's `enabled` bit**, on the
  in-memory `RunnerConfig` this control holds — never touching `tack-runner/src/config.rs`
  or `provider.rs`. Narrow by design: only the literal name
  `tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET` triggers it; a deployment that
  configured a different secret-store entry name via
  `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` keeps using its existing console-only
  toggle unchanged.
- **Secret `set_at` timestamps live in a sidecar file this card owns**
  (`<state_dir>/secret_meta.json`), never inside `tack-runner::SecretStore`, which tracks no
  timestamp at all and wasn't touched. A name present in the real store but absent from the
  sidecar (set via `tack runner secret set` before this UI existed) reports `set_at: null`.
- **`AgentsPage.tsx` at `/agents` is a placeholder**, not VI-C1's page. It exists so the two
  panels have somewhere real to render (needed for the Playwright specs, and for anyone
  clicking around before VI-C1 lands) but is not linked from the sidebar — seen in "What a
  stranger still cannot do."

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `/api/local-runner*` routes are a genuine 404 on a non-loopback bind, even with a control wired in | `cargo nextest run --workspace -E 'test(routes_are_absent_on_a_non_loopback_bind)'` — passes; asserts `StatusCode::NOT_FOUND`, not a 409/403 envelope |
| Same routes are 404 on a loopback bind when nothing was ever wired in (a bare `tack_api::serve()` caller) | `test(routes_are_absent_on_a_loopback_bind_with_no_control)` — passes |
| Loopback + a wired-in control: `GET` reports `enabled:false,state:stopped`; `PUT {"enabled":true}` calls the same `start()` a boot-time auto-start would, and the next `GET` reflects it | `test(a_loopback_bind_with_a_control_wired_in_mounts_the_routes_and_starts_it)` — passes |
| A secret `PUT` never writes `app_meta`, and the value never appears in any response | `test(the_enable_preference_is_the_only_app_meta_row)` — passes; also see the live proof below |
| A persisted "enabled" preference never auto-starts the runner on a non-loopback bind | `test(a_persisted_enable_preference_never_auto_starts_on_a_non_loopback_bind)` — passes; reverted the fix once, watched it fail with the exact symptom, restored (see above) |
| A disabled/unconfigured provider's catalog reads `NotConfigured` with no network call | `test(a_disabled_provider_reports_not_configured_with_no_network_call)` (tack-cli) — passes |
| Setting the default Vercel secret flips that provider's `enabled` bit | `test(setting_the_default_vercel_secret_enables_that_provider)` — passes |
| Secret set/list/remove round-trips with a recorded `set_at`; a name the sidecar never recorded reports `null` | `test(set_then_list_then_remove_a_secret_round_trips_with_a_recorded_set_at)` — passes |
| A live, real server: `PUT enabled:true` self-provisions and reports `state:running`; `PUT` a secret returns 204; `GET` reflects both | live `curl` transcript, "Secret-path proof" below |
| The secret value never reaches a log line, at `debug` level, with the name still present | live transcript, "Secret-path proof" below |
| `sqlite3 <db>.dump \| grep -c <value>` = 0 for a value that really was saved | live transcript, "Secret-path proof" below |
| Frontend: `ExecutionToggle` shows Stopped/Turn-on by default, flips to Running/Turn-off after a real toggle, and renders the console command instead of an error on a 404 | `ExecutionToggle.test.tsx` (4/4) plus `e2e/execution-toggle.spec.ts` against the real server |
| Frontend: `ProviderKeyPanel` never renders the pasted value, shows Set/Replace/Remove once stored, and shows the console fallback on 404 | `ProviderKeyPanel.test.tsx` (4/4) plus `e2e/provider-key-panel.spec.ts` — asserts `page.content()` never contains the pasted value, and the catalog line changes from "not configured" after save (a real network call to the real gateway; no valid credential in this sandbox, so it lands on a typed "unreachable (401)", not a fabricated model count) |
| Regenerating `docs/openapi.json`/`schema.gen.ts` is idempotent (no further drift) | `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'` run twice; `diff -q` of the two outputs → identical |

A row with no evidence is a claim to delete, not a row to leave blank — every row above was
run this session, not assumed.

## Measured numbers

- `cargo nextest run --workspace` (post-card): **1401 tests run, 1401 passed, 6 skipped, 0
  failed** — `.config/nextest.toml` summary line, ~23s. Net new vs. the pre-card tree: +11
  Rust tests (6 in `crates/tack-cli/src/local_runner.rs`, 4 in the new
  `crates/tack-api/tests/handlers/local_runner.rs`, 1 in `crates/tack-api/src/server.rs`).
- `cargo nextest run --workspace -E 'package(tack-api)'`: 469/469.
  `-E 'package(tack-cli)'`: 104/104.
- `cargo nextest run --workspace -E 'binary(runner_contract)'`: 18/18, byte-identical
  (`git status --porcelain docs/contracts/ crates/tack-orch/tests/runner_contract.rs` → empty).
- `cargo fmt --all --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`:
  clean. `./scripts/check-comments.sh`: clean (fixed 5 board-scaffolding hits — card-id
  references in doc comments — before this run). `./scripts/check-test-hygiene.sh`: clean.
- Frontend: `npm run type-check` clean; `npx vitest run` — **87 files, 764 tests, all
  passed** (up from 85 files/756 tests pre-card: +2 files, +8 tests —
  `ExecutionToggle.test.tsx` and `ProviderKeyPanel.test.tsx`, 4 each);
  `npm run build` succeeds, `AgentsPage` bundled at 7.38 kB gzipped 2.56 kB.
- Playwright: `npx playwright test e2e/execution-toggle.spec.ts e2e/provider-key-panel.spec.ts
  --project=chromium` — **2/2 passed**, run twice back-to-back with no state cleanup between
  runs (idempotent). Firefox/webkit could not be exercised in this sandbox — see
  "Not checked."
- `git diff --stat` (tracked files, excluding the two generated files and `Cargo.lock`):
  ~40 files touched; two new Rust files (`handlers/local_runner.rs`,
  `tests/handlers/local_runner.rs`), one Rust file fully rewritten
  (`crates/tack-cli/src/local_runner.rs`), four new frontend files
  (`features/agents/{api.ts,ExecutionToggle.tsx,ProviderKeyPanel.tsx,AgentsPage.tsx}` plus
  their two test files), two new Playwright specs. `Cargo.lock`: 1 line (`async-trait` added
  to `tack-cli`'s already-resolved dependency graph — no new crate version).

## What a stranger still cannot do

Find the "Agents" page from the sidebar — `/agents` exists and both panels work, but nothing
links to it yet (VI-C1's job; see its own block in the dispatch README, which explicitly
composes this card's routes and panels). A stranger who already knows the URL can turn the
embedded runner on, paste a Vercel AI Gateway key, and watch the catalog line change — but
discovering that path from the running UI is still Wave 16's job, not this card's.

## Surface-map delta

Two rows of §VI.0's table move from console-only to UI, exactly as the card described:
"Turn on agent execution" (was: `tack serve --with-runner`, now: one switch, with the
console command rendered only on a non-loopback bind or a bare-library embedder) and
"Authenticate through Vercel AI Gateway" (was: impossible in the embedded case; now: a
write-only field with a live catalog readout). Both already had rows naming exactly this
target in §VI.0 — no new row needed, no reason found that the table's last column doesn't
already cover.

## Secret-path proof

Live, against a real `cargo run -p tack-cli -- serve` bound to `127.0.0.1:39877`
(`TACK_LOG_LEVEL=debug`, a scratch `sqlite` file, `TACK_RUNNER_STATE_DIR` under a scratch
dir — all removed afterward):

- `curl -X PUT :39877/api/local-runner -d '{"enabled": true}'` → `204`; `GET
  /api/local-runner` → `{"enabled":true,"state":"running",...}` — a real self-provisioned
  runner, not a stub.
- `curl -X PUT :39877/api/local-runner/secrets/logcheck-secret-marker -d
  '{"value":"UNIQUE_SECRET_MARKER_98765"}'` → `204`.
- **Value never logged, name present**: `grep -c UNIQUE_SECRET_MARKER_98765 server.log` →
  **0**; `grep -c logcheck-secret-marker server.log` → **2** (both `tower_http` request/
  response trace lines, naming the path — never the body).
- **`sqlite3 .dump` grep, positive control**: after the same PUT against a real e2e run
  (`frontend/e2e.db`, via `e2e/provider-key-panel.spec.ts`), `sqlite3 e2e.db .dump | grep -c
  e2e-placeholder-key-never-should-render` → **0**; `SELECT key FROM app_meta;` → **empty**
  (that spec never toggles the on/off preference, so zero app_meta rows is the correct,
  positive-control result — not an oversight).
- **Store holds the value, at mode 600 for the file backend**: not re-measured here — this
  card reuses `tack-runner::SecretStore` unmodified (VI-B1's own file), and VI-B1's handoff
  already proved `stat -c '%a'` → `600` for that exact code path
  (`secrets::tests::file_backend_writes_the_secrets_file_owner_only`, plus a live run). This
  dev machine's real D-Bus/gnome-keyring session made the keychain backend the one actually
  exercised live above and in the Playwright specs; `crates/tack-cli/src/local_runner.rs`'s
  own unit tests force the file backend instead (`DBUS_SESSION_BUS_ADDRESS=/dev/null`) to
  prove the round trip works on that path too, without asserting the mode bit again.

### A pre-existing stray secret, not from this card

While preparing the above, `secret-tool search service tack-runner` turned up a real,
already-stored `vercel-ai-gateway/default` entry, created 2026-09-04. VI-B2's own handoff
(§"Read this first: the blocking finding") names this exact entry and reports it invalid
against the live gateway (`401 Unauthorized`) — it is VI-B2's own test residue on this
shared dev machine, not something this card wrote or relied on. Cleared with `secret-tool
clear service tack-runner username 'vercel-ai-gateway/default'` before every clean test run
above; not committed anywhere, its value never appeared in any command this card ran.

## Not checked

- **Firefox and WebKit**, for the two new Playwright specs — this sandbox's installed
  browser cache (`~/.cache/ms-playwright`) has different revision numbers than this
  project's pinned Playwright version, so both fail to launch
  (`browserType.launch: Executable doesn't exist`) independent of anything this card
  changed. Chromium-only, run twice back-to-back: 2/2 both times.
- **A live "Configured{model_count}" catalog result** — no valid Vercel AI Gateway
  credential is available in this sandbox (the one stray key found is confirmed invalid, see
  above), so `e2e/provider-key-panel.spec.ts` proves the re-probe fires and the catalog line
  changes (not-configured → a typed `unreachable (401)` reason), not a genuine positive
  model count. The `Configured` branch of `CatalogSnapshot` is exercised by the JSON
  (de)serialization but never by a real successful fetch in this session.
- **The full acceptance flow "an active runner appears and a shim attempt completes"** —
  proved that a real self-provisioned runner reaches `state: running` (live curl transcript,
  and the Playwright toggle spec), but did not drive a full claim → shim-harness → completion
  cycle through it; that needs a fake/shim harness binary on `PATH`, which this session did
  not set up. `crates/tack-runner`'s own existing test suite (untouched, still green) already
  covers that cycle against the engine directly.
- **macOS/Windows keychain code paths** — same reason VI-B1 recorded: this CI/this machine
  is Linux-only; `secrets.rs` (VI-B1's file) was not touched by this card.
- **Release-build binary size impact** — not rebuilt with `--release`; this card added no
  new runtime dependency to `tack-runner` (the one new dependency, `async-trait`, lands in
  `tack-cli`, already a workspace-pinned crate used elsewhere).

## Context spent

- Cold start: the dispatch README header + VI-B3 block (~1k), `VI-B1.md` in full (329
  lines, ~3k), ADR 0061's decision table (~1k, read incidentally while resolving the ADR-
  number discrepancy below) — in the same range as the block's ~20k estimate for this
  section, though the actual session ran considerably longer than a cold-start budget once
  implementation began (see below).
- **A correction to the dispatching agent's own instruction, recorded here because later
  readers should not repeat it**: the dispatching prompt stated "ADR 0063 ... decisions 2
  and 6 are the ones your block names." Measured against both ADR files: ADR 0063's
  decisions 2 and 6 are "a gateway and a vendor's own API are one mode" and "a quoted price
  is never a measured spend" — neither is about this card's toggle or secret-write surface.
  ADR 0061's decisions 2 and 6 ("one narrow route lets the UI hand a key to the runner
  sharing its machine" and "add a one-click switch to start/stop the built-in runner") are
  the ones that actually describe VI-B3, and are exactly what TODO.md's own VI-B3 card body
  cites ("Needs VI-B1 (the store) and VI-A2 (decisions 2 and 6)" — VI-A2 is the card that
  wrote ADR 0061). Implemented against ADR 0061 decisions 2 and 6; flagging the mismatch
  rather than silently substituting one ADR for another.
- Files opened beyond the read list, and why: `crates/tack-runner/src/bootstrap.rs`
  (targeted grep + small ranges, sanctioned by the block itself: "a grep for the re-probe
  entry point" — found `bootstrap::probe`, an existing entry point, so no new one was
  needed); `crates/tack-runner/src/provider.rs` (grepped for `CatalogStatus`'s variant
  shape only, read-only, needed to define `CatalogSnapshot`'s mapping — never edited, VI-B4's
  file); `crates/tack-runner/src/config.rs` (grepped for `RunnerConfig`/`ProviderConfig`
  field shapes, needed for `EmbeddedRunnerControl`); `crates/tack-api/src/server.rs` whole
  (not in the read list, but unavoidable — the auto-start seam and the `serve`/
  `serve_with_ready` signatures this card had to extend live there); `crates/tack-api/src/
  handlers/settings.rs` lines 1-390 (the `effective_orch_enabled`/`put_orch_settings`
  precedent the block's own read list points at via `orch_runtime.rs`, read in full rather
  than the block's narrower grep because the exact `app_meta` load/save shape needed
  copying, not just citing); `frontend/src/features/settings/orchestrationSettings/{api.ts,
  OrchestrationSettingsSection.tsx,OrchestrationSettingsSection.test.tsx}` (not named by the
  block — the closest structural precedent for a persisted-preference-plus-runtime-toggle
  panel, more relevant than `ModelProfilesPanel.tsx` for this specific shape; used as the
  template for `api.ts`'s doc comment and the test file's fetch-mocking pattern).
- Read-list items not used as-is: the block's estimate assumed the routes/panels would be
  a comparatively bounded change; the actual seam (making a boot-time-only composition into
  something a live HTTP route can also drive, twice — once for the toggle, once for
  secrets — while keeping `tack-api` free of any `tack-runner` dependency) took substantially
  more design work than the ~20k token estimate implied, including the auto-start
  loopback-safety bug found and fixed mid-session (see above). Not a fault in the block's
  read list — the files it named were the right ones — the composition itself was the hard
  part, not information-gathering.
- This session ran well past a typical cold-start budget once implementation, live
  verification (a scratch `tack serve` process, `secret-tool`, a real Playwright browser
  run repeated several times to chase down a stale-keychain false alarm), and the
  self-caught auto-start bug are counted. Exact token accounting wasn't captured mid-session.

## Proposed status-board row (Wave 15, not applied — integrator's call)

**VI-B3 complete, not yet integrated** (handoff `docs/agent-handoffs/part-vi/VI-B3.md`).
Embedded-runner on/off and Vercel-key routes exist, loopback-gated and absent otherwise; a
boot-time auto-start loopback-safety bug was introduced and caught in the same session (see
the handoff's own section on it). Awaits VI-B4 (parallel, disjoint files) before Wave 15
closes.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
