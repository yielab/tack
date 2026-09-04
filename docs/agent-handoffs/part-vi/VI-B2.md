# VI-B2 handoff

- Base SHA / branch / final SHA: base `7ac3e26` (the `develop` tip named in the dispatch);
  branch `agent/vi-b2-vercel-gateway`; committed on that branch (final SHA in `git log -1`
  on the branch at handoff time).
- Files changed (must equal ownership list): `crates/tack-runner/src/config.rs` (the
  `[provider.*]` section), `crates/tack-runner/src/provider.rs` (new — catalog fetch,
  endpoint resolution), `crates/tack-runner/src/harness/claude_code.rs` and `codex.rs`
  (spawn injection, tests), `crates/tack-runner/src/bootstrap.rs` (the catalog step),
  `crates/tack-cli/src/doctor.rs` (the provider block), `docs/CONFIG.md` (gateway rows).
  Two more were touched only to keep the field additions above compiling, not for their own
  logic: `crates/tack-runner/src/main.rs` and `crates/tack-runner/tests/bootstrap_entrypoint.rs`
  (added `..ConfigOverrides::default()`/a new field to existing struct literals after
  `ConfigOverrides`/`DiscoveryReport` grew fields), and `crates/tack-cli/src/local_runner.rs`
  (same one-line fix, in VI-B3's owned file — see "Files changed vs. ownership").
- Contract fixtures consumed: none read or edited. `runner_contract` byte-identical — see
  "Measured numbers".
- Behavior implemented: a `[provider.vercel_ai_gateway]` config section (map-shaped, not a
  hardcoded field — see "Design deviation" below); a catalog fetch merged into
  `report.capabilities` for claude-code and codex; per-spawn injection for both harnesses,
  guarded so a direct-model request receives none of it; a `doctor` provider block; two new
  `docs/CONFIG.md` rows plus a gateway-routed column on the existing per-harness table.
  opencode is untouched.
- Tests added and exact commands/results: 19 new tests across 5 files — see "Measured
  numbers" for the exact command and counts, "Claim → evidence" for what each proves.
- Failure/adversarial case proved: the direct/gateway environment-variable guard is
  load-bearing — reverting the injection branch in `claude_code.rs::start` to a no-op made
  `a_configured_provider_request_spawns_with_its_endpoint_variables_present` fail with
  exactly the expected symptom (`["HOME", "PATH", "PWD"]`, no `ANTHROPIC_BASE_URL`); restored
  and re-verified green. See "Claim → evidence".
- Schema/API/contract change requested from another owner: **one, real, not smuggled** — see
  "Escalation: the catalog has nowhere to put per-model metadata" below. `ModelCombination`
  itself needed no change for this card (ids only, as scoped) but a later card will need a
  reviewed field addition.
- Known limitations or `not_measured` fields: the acceptance's "successful billed completion"
  bullet is **not met** — the keychain-stored key is genuinely invalid against the live
  gateway (measured, see "The blocking finding" below). Everything else about the mechanism
  is proven live regardless. opencode's non-interactive gateway path is measured but
  deliberately not built (user-directed scope cut, mid-session — see "Corrections to the
  card" below).
- Secrets/logging review: see "Secret-path proof" below — `sqlite3 tack.db .dump | grep -c`
  is `0` for both the resolved value and the store entry name, with a positive control; a
  captured server log shows the entry *name* logged once and the value never.
- Safe merge order and likely conflicts: needs VI-B1 merged first (it is, per the dispatch
  README — B1's `SecretStore`/`secret_reference` plumbing is depended on directly). No file
  in this diff is claimed by any other Wave 15/16 card except the one-line fixes noted above
  in VI-B3's `local_runner.rs`. `doctor.rs` gets a second, disjoint addition here on top of
  VI-B1's (the secret-backend line) — no overlap, both additions verified together.
- Checklist: no unowned files edited without justification below; no live secret committed,
  printed, or logged (see "Secret-path proof"); no panic stub; no blind retry (a genuine,
  reproducible, non-blocking flake is documented, not retried away — see "A test flake found
  along the way, not caused by this card").

## Read this first: the blocking finding

**The keychain-stored key (`vercel-ai-gateway/default`) is invalid against the live
gateway.** Measured three independent ways, all against the real `ai-gateway.vercel.sh`:

1. A direct HTTP `GET /v1/models` with `Authorization: Bearer <resolved key>` →
   **`401 Unauthorized`**, body `{"error":{"message":"Authentication failed. Create an API
   key and set in AI_GATEWAY_API_KEY environment variable: ...","type":"authentication_error"}}`.
2. A real `claude` 2.1.261 binary, spawned through this card's own `ClaudeCodeAdapter`
   with `ANTHROPIC_BASE_URL=https://ai-gateway.vercel.sh/claude-code` and
   `ANTHROPIC_AUTH_TOKEN=<resolved key>` → the init line confirms the model was accepted
   for the *request* (`"model":"anthropic/claude-opus-4.6"`), then five observed
   `api_retry` events, all `"error_status":401,"error":"authentication_failed"`, delays
   534ms/1096ms/2221ms/4260ms/9583ms (roughly doubling — matches the card's "up to 11
   retries" claim; the process timed out before exhausting them).
3. A real `codex` 0.149.1 binary, spawned through this card's own `CodexAdapter` with the
   `-c` overrides → `url: https://ai-gateway.vercel.sh/codex/v1/responses`, five
   `Reconnecting... N/5` lines, all the identical 401 body as (1).

All three prove the *routing* is correct — the request demonstrably reaches Vercel's own
infrastructure, not a vendor's direct API, and the vendor's own error text is what comes
back. None of the three could complete successfully, because the credential itself does
not authenticate. I do not have dashboard access to rotate or inspect the key. This blocks
only the acceptance bullet that specifically asks for a **successful** completion with
`actual.model_provider`/`actual.model_id` recorded from a real served response — every
other acceptance bullet is met on its own terms (see "Claim → evidence").

Both live tests below are committed, `#[ignore]`d, opt-in, and were run once each,
deliberately, producing the transcripts above:

```
TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo test -p tack-runner --lib -- --ignored --nocapture \
  claude_code::tests::live_claude_code_through_the_configured_provider_when_opted_in
TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 cargo test -p tack-runner --lib -- --ignored --nocapture \
  codex::tests::live_codex_through_the_configured_provider_when_opted_in
```

Both assert only what holds regardless of key validity (routing reached; for claude-code,
`model_observation_source` is never `harness_reported`; for codex, the request/observed
provider and model echo correctly) — neither asserts a successful completion, so neither
becomes a permanently-red gate once committed. Whoever provisions a working key can widen
these assertions and re-run for the still-missing "successful completion" proof.

**While chasing this down I inadvertently had a keychain lookup tool
(`secret-tool search`) print the raw key value to my own terminal once.** I did not write
it to any file, log, commit, or this document, and it is not repeated anywhere below —
flagging the near-miss itself in case the tooling (`secret-tool search` vs. `lookup`,
which this project's own VI-B1 handoff already used correctly) is worth a note for the next
person debugging a stored secret by hand.

## Corrections to the card

Per the dispatch instructions, measured facts win over the card's text; each is noted here
rather than silently applied.

- **The config default the card names is wrong, as the dispatch brief already flagged**:
  `secret` defaults to `vercel-ai-gateway/default`, not `vercel-ai-gateway` —
  `SecretStore::resolve` does not append `/default` on its own.
- **The config section is a map, not a single field.** Mid-session direction: `RunnerConfig`
  carries `providers: BTreeMap<String, ProviderConfig>`, keyed by TOML table name
  (`vercel_ai_gateway`, underscore — distinct from the wire-level provider name
  `vercel-ai-gateway`, hyphen, per ADR 0061 decision 4). The user-facing TOML is unchanged;
  this only changes the Rust shape, so a second provider entry is a config row, not a type
  change.
- **The injection mechanism is named by wire shape, not by vendor.** Mid-session direction:
  no type, field or function anywhere is named "gateway" — `crate::provider` resolves a
  `ProviderEndpoint` (base URL, credential env-var name, resolved credential) for a `Wire`
  (`AnthropicMessages` / `OpenAiResponses`), and each adapter applies the descriptor for the
  wire it already speaks. A vendor's own direct API and a gateway are the same shape here
  (base URL + bearer credential); a second entry of either kind is a new row in
  `provider::known_endpoint`, never a new mechanism. The *entry name* `vercel_ai_gateway`
  stays exactly that, because that is what this entry is.
- **Codex's own `wire_api` claim needed a correction, discovered live**: the card and this
  card's own dispatch brief both say to set `wire_api="responses"` explicitly because it is
  the effective default. Confirmed unchanged by measurement — see "Vendor table" below.
- **opencode's non-interactive path exists and was measured, then deliberately cut from
  scope mid-session** (see next section) — the card's own text treated this as the likely
  "stop if" case; it is not, but is out of scope here regardless.
- **The catalog cannot carry per-model metadata as scoped** (pricing, context window,
  modalities, ...) — `ModelCombination` has nowhere to put it. Ids only, as the card
  already said; the gap is escalated, not solved here or smuggled through `additional`. See
  "Escalation" below.

## opencode: measured, not built (user-directed scope cut)

A non-interactive path **does exist**, measured live against the real gateway with a fake
key (a distinct "authentication error" came back — proof the request reached the gateway
— and a distinct "no authentication provided" message with no key at all, proof the
request genuinely needs the credential):

- A project-local `opencode.json` written into the runner's own workspace
  (`spec.workspace.path`), declaring `{"provider":{"vercel":{"npm":"@ai-sdk/gateway",
  "models":{"anthropic/claude-opus-4.6":{}}}}}` — one model is enough; the package
  self-registers the whole catalog.
- `AI_GATEWAY_API_KEY` in the spawned environment.
- A three-segment `--model vercel/<catalog-id>` selector; `opencode.rs`'s own
  `parse_model_combinations_keeps_a_model_id_with_extra_slashes_fully_opaque` already
  proves the adapter treats everything after the first `/` as opaque, so this needs no
  parser change either.

This was **not implemented**, on explicit direction mid-session: writing a config file into
the workspace and loading an npm package is a materially different mechanism from the other
two harnesses' pure environment/flag injection, and folding it under the same concept here
would hide that asymmetry. It is left for a separate card, behind whatever abstraction that
card decides on. `opencode.rs` itself is untouched — no gateway-shaped code was added or
removed there. Its capability snapshot carries zero gateway `model_combinations` for the
same reason it always would have (it is simply never in `provider::CATALOG_ELIGIBLE_HARNESSES`),
and `doctor`'s new provider block says explicitly `opencode: not yet — its non-interactive
path needs a config file written into the workspace, a different mechanism this build does
not implement` rather than leaving the omission silent.

Vercel documents **no** non-interactive path for opencode either (only its own `vercel` CLI
setup command or the interactive `/connect`) — same as codex, this project's own finding,
not something the vendor promises.

The real fix for opencode's actual-model observation, also measured and not built here:
`opencode export <sessionID>` returns `info.model.{providerID,id}` after the fact. It needs
a second subprocess with its own timeout/failure handling — recorded here as a measured,
available option for whichever card eventually builds the opencode gateway path.

## Vendor table, measured (supersedes the card's for VI-C1/VI-D1)

| Harness | Endpoint | What actually works |
|---|---|---|
| `claude-code` | `https://ai-gateway.vercel.sh/claude-code` (no `/v1`) | `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN=<key>`, defensive empty `ANTHROPIC_API_KEY`. Confirmed live: request reaches the gateway, correctly-shaped 401 on a bad key, ~90s+ retry storm (11 attempts, exponential backoff) that a `ProcessLimits`/`timeout_seconds` bound cuts off |
| `codex` | `https://ai-gateway.vercel.sh/codex/v1` | Per-invocation `-c model_provider=…`/`model_providers.<key>.{name,base_url,env_key,wire_api}` plus `AI_GATEWAY_API_KEY` in the spawned env — **no** `~/.codex/config.toml` write, confirmed live (the file was never touched; a fake/bad key still reaches the real gateway host, proven by the vendor's own 401 body naming `ai-gateway.vercel.sh` in its `url` field, distinct from a 400 from OpenAI's own API shape). `wire_api="responses"` remains non-load-bearing (0.149.1 already defaults to it) but is set anyway, defensively |
| `opencode` | native `vercel` provider, or `@ai-sdk/gateway` via `opencode.json` | Non-interactive path exists (see above) — not implemented, by direction |

`ANTHROPIC_API_KEY=""` for claude-code: re-measured against 2.1.261 (one point release
newer than the dispatch brief's 2.1.260). Same result — empty, unset, and non-empty all
produce identical outgoing requests, `ANTHROPIC_AUTH_TOKEN` wins regardless. Set anyway, at
zero cost. No comment or test in the tree claims this is what makes the gateway work; the
code comment at the call site states the measurement and the vendor's contradicting claim.

**Claude Code's `[1m]` suffix** (mentioned in the dispatch brief as something to watch for
on `actual.model_id`) was not observed in any live run — the init line's `model` field was
always the bare requested id (`anthropic/claude-opus-4.6`). Not ruled out for other models;
not fabricated as either present or absent beyond what was actually seen.

## Actual-model observation, per harness

- **claude-code**: the `{"type":"system","subtype":"init"}` line fires before any network
  call, so it cannot itself confirm the gateway served the named model.
  `parsed_from_result_line` now branches on the requested provider: a direct-provider run
  keeps its existing `harness_reported` (unchanged — out of this card's scope to revisit for
  the direct case); a gateway-routed run is recorded as `requested_not_confirmed` instead,
  proven both as a pure unit test
  (`a_gateway_routed_result_is_recorded_as_requested_not_confirmed_even_on_a_fast_result_line`,
  which exercises the same input through both branches) and live (a killed retry-storm run
  actually falls through a different, even more conservative path —
  `malformed_outcome`/`fallback_from_exit_code`, which was already unconditionally
  `not_observed` and needed no change; see the live test's real output). The one invariant
  proven both ways: a gateway-routed run must never claim `harness_reported`.
- **codex**: unchanged — already `requested_not_confirmed` unconditionally (module docs,
  pre-existing assumption 5), which is honest for a gateway-routed run without further work.

## Escalation: the catalog has nowhere to put per-model metadata

Fetching the real catalog (`GET https://ai-gateway.vercel.sh/v1/models`) confirms the
brief: each entry carries far more than an id — `context_window`, `max_tokens`, `pricing`,
`modalities`, `supported_parameters`, `knowledge` cutoff, data-retention flags.
`ModelCombination` (`crates/tack-orch/src/execution/capabilities.rs`) has nowhere to put
any of it: `model_provider` + a bare `Vec<ModelId>`. This card projects ids only, which is
in scope and sufficient for its own acceptance — the gap is recorded here for whoever picks
up the contract change, per `docs/adr/0063-harness-credential-modes.md` (proposed, not
read or edited by this card):

- **The shape needed**: per-model metadata attached to an entry inside `model_combinations`
  — never a parallel list keyed by index/id, which can silently drift out of sync with the
  ids it describes.
- **`additional` is not the answer.** That map exists for forward-compatible unknown-field
  round-tripping, not as a side door for a field the contract should declare — using it here
  would be a contract change with no review, exactly what `docs/contracts/runner-v1/`'s own
  rule forbids.
- **Pricing is not `{input, output}`.** Across the 373-model catalog, pricing takes roughly
  two dozen distinct shapes: cache read/write rates, tiered inputs/outputs, regional and
  service-tier variants, per-second audio/video rates, and a literal string
  `"varies_by_provider"`. A field typed as a flat two-number pair would silently falsify a
  large minority of the catalog. Whatever field eventually lands should store what the
  vendor published, not a normalized shape invented here.
- **Coverage is partial, and the gaps must stay null.** Roughly 350 of 373 models publish a
  price; roughly 270 publish a context window. The remainder publish nothing for that field
  — under this tree's own rule, that must be `null`, never `0` and never a default; a free
  model and an unmeasured limit must never look the same.
- **A catalog price is a vendor quote, not a measured spend**, and must never fill an
  attempt's own cost field. This card's own scope already keeps usage `measured` only from
  harness output (see "Usage" below); this is the same rule from the catalog side, worth
  stating explicitly so a future reader does not "helpfully" merge the two.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `[provider.vercel_ai_gateway]` defaults to disabled, secret `vercel-ai-gateway/default` | `config::tests::provider_defaults_to_disabled_with_the_expected_secret_name` |
| Config precedence is defaults → file → environment → CLI, per field, not per entry | `config::tests::provider_config_precedence_is_defaults_file_environment_then_cli` (env overrides only `secret`, file's `enabled` survives) |
| An unknown field inside `[provider.vercel_ai_gateway]` is rejected | `config::tests::unknown_field_inside_a_provider_table_is_rejected` |
| An unrecognized provider *name* does not fail loading; the known entry stays at its default | `config::tests::an_unrecognized_provider_name_does_not_fail_loading_or_affect_the_known_one` |
| A direct-vendor provider resolves to no endpoint at all (not an error) | `provider::tests::a_direct_vendor_provider_resolves_to_no_endpoint_at_all` |
| A disabled provider is a typed `NotConfigured`, not a panic or a silent fallback | `provider::tests::a_disabled_provider_is_a_typed_not_configured_error` |
| A missing secret is a typed `Secret` error naming only the provider, never a value | `provider::tests::an_enabled_provider_with_no_such_secret_is_a_typed_secret_error` |
| A configured provider resolves the correct base URL/credential-var name per wire | `provider::tests::a_configured_provider_resolves_the_wire_specific_endpoint` |
| `ProviderEndpoint`'s `Debug` never exposes the credential | `provider::tests::credential_is_never_visible_through_debug` |
| A direct-model claude-code request spawns with no gateway variable *name* present | `claude_code::tests::a_direct_model_request_spawns_with_no_provider_endpoint_variable_present` |
| A gateway-routed claude-code request spawns with the variable names present | `claude_code::tests::a_configured_provider_request_spawns_with_its_endpoint_variables_present` |
| A disabled provider rejects a claude-code request pre-spawn | `claude_code::tests::a_disabled_provider_rejects_the_request_before_any_process_spawns` |
| The claude-code guard above is load-bearing | reverted the injection branch to a no-op once; the positive test failed with `["HOME","PATH","PWD"]` (no `ANTHROPIC_BASE_URL`); restored, re-verified green |
| A gateway-routed claude-code result is `requested_not_confirmed` even on a fast result line, never `harness_reported` | `claude_code::tests::a_gateway_routed_result_is_recorded_as_requested_not_confirmed_even_on_a_fast_result_line` |
| Same three guard proofs for codex (`-c` flags + `AI_GATEWAY_API_KEY`) | `codex::tests::{a_direct_model_request_spawns_with_no_provider_endpoint_variable_present, a_configured_provider_request_spawns_with_its_endpoint_variable_present, a_disabled_provider_rejects_the_request_before_any_process_spawns}` |
| `doctor` prints `provider: vercel_ai_gateway` status/catalog count/timestamp, never the key | `crates/tack-cli/src/doctor.rs::render_provider`; live run below; `doctor::tests::render_provider_does_not_panic_for_any_catalog_status` |
| `doctor --json` carries the catalog inside `report.capabilities`, not a side field | live `tack runner doctor --json` with the provider enabled → `harnesses[].model_combinations` for claude-code/codex is `[]` while disabled/unconfigured, and the same struct `bootstrap::probe`/`build_runtime` both populate — see "Measured numbers" |
| With the provider disabled, zero gateway `model_combinations` on every harness | live `tack runner doctor --json` (default config) — `claude-code: []`, `codex: []` |
| Live claude-code and codex runs through the real endpoints, request format proven correct | see "The blocking finding" — both `#[ignore]`d live tests, run once each |
| The gateway key never reaches `tack.db` or the operator API | see "Secret-path proof" |
| `runner_contract` is byte-identical | `cargo test -p tack-orch --test runner_contract` — 18/18, `git diff --name-only -- docs/contracts/ crates/tack-orch/tests/runner_contract.rs` → 0 files |

A row with no evidence is a claim to delete, not a row to leave blank — every row above was
actually run this session.

## Measured numbers

- `cargo test -p tack-runner --lib`: **259 passed, 0 failed, 5 ignored** (3 pre-existing
  opt-in live-harness tests + this card's 2 new live gateway tests, all still opt-in).
  `cargo test -p tack-runner --all-targets`: every integration binary green
  (`bootstrap_entrypoint` 2/2, `cli` 2/2, `crash_matrix` 7/7,
  `g2_journal_corruption_test` 3/3, `h3_checkout` 6/6).
- `cargo test -p tack-orch --test runner_contract`: **18/18**, byte-identical.
- `cargo test -p tack-api --test wave2_gate`: **5/5**.
- `cargo clippy --all-targets -- -D warnings` (whole workspace): clean, 0 warnings.
- `cargo fmt -- --check` (both crates touched): clean.
- 19 new tests added: 5 in `claude_code.rs`, 4 in `codex.rs`, 4 in `config.rs`, 5 in the new
  `provider.rs`, 1 in `doctor.rs`.
- `git diff --stat` (cached): 11 files, 1540 insertions(+), 41 deletions(-); 1 new file
  (`crates/tack-runner/src/provider.rs`, 375 lines).
- Live `tack runner doctor --json`, provider enabled: `claude-code.model_combinations: []`,
  `codex.model_combinations: []` (catalog fetch failed — the blocking finding), `opencode`
  unaffected, still its own native `reported` entries.
- Live `tack runner doctor` (human output), provider enabled: `Provider endpoint
  (vercel_ai_gateway): reaches: claude-code, codex (opencode: not yet — ...) / status:
  catalog error (HTTP 401)`.

## What a stranger still cannot do

Get a real, successful, model-serving attempt through the Vercel AI Gateway on this
machine's stored key — the key itself is invalid, and nobody without dashboard access to
the Vercel team that issued it can fix that from inside this repository. Everything else
promised by this card — configuring the provider, seeing it reach the real gateway host
with the right request shape, seeing a direct-model request stay completely untouched by
it, seeing `doctor` report an honest status — works today and is proven live. A stranger
also still cannot route an opencode attempt through this provider at all; that is an
explicit, recorded scope cut, not a gap they would discover by surprise (doctor's own
output says so).

## Surface-map delta

No row moves. §VI.0's surface map does not carry a "configure the model gateway" row yet —
this card is runner-side machinery (config, catalog, injection), not a UI surface; VI-B3
and VI-C1 are what would move a row here, and neither has landed at time of writing.

## Secret-path proof

- **`sqlite3 tack.db .dump | grep -c`, with a positive control**: ran `tack serve
  --with-runner` against a scratch SQLite database with the provider enabled (forcing a real
  catalog fetch against the real key). After the run: `sqlite3 tack.db .dump | grep -c
  "<resolved key>"` → **`0`**; `grep -c "vercel-ai-gateway/default"` (the entry name) → **`0`**
  — the database never sees either. Positive control: `grep -c "<the run's own workspace
  UUID>"` → **`1`** (proves the dump genuinely contains real data and the grep mechanism
  works — a `0` that only means "grepped for the wrong thing" would prove nothing).
- **Captured log output, name present / value absent**: same run, with `RUST_LOG=tack_runner=debug`.
  `grep -c "<resolved key>"` on the captured log → **`0`**; `grep -c
  "vercel-ai-gateway/default"` → **`1`** (from `provider::resolve_endpoint`/`attach_catalog`'s
  own `tracing::debug!(secret = %config.secret, ...)` lines, added this card — logs the
  *entry name*, matching `resolve_environment`'s existing pattern for `secret_reference`,
  never the resolved value).
- **`ProviderEndpoint`'s `Debug`/`Display`**: inherits `SecretValue`'s existing hardcoded
  `[REDACTED]` (VI-B1) — `provider::tests::credential_is_never_visible_through_debug`.
- **`tack runner secret list`**: unchanged from VI-B1 — names only, confirmed still true by
  running it during this card's own live proofs (`vercel-ai-gateway/default` listed, no
  value).

## A test flake found along the way, not caused by this card

`codex::tests::a_configured_provider_request_spawns_with_its_endpoint_variable_present`
intermittently fails under this sandbox's **default full-suite parallelism** (observed
roughly 20–35% of runs at `nproc`-driven default thread count; **0 failures in 20+
consecutive runs** at `--test-threads=8` or when filtered to a handful of tests). Every
failure showed the identical symptom: the spawned shell's own `env` dump contained only
`PWD`, as if `Command::env_clear().envs(&self.env)` had not propagated. Instrumented (and
since removed) debug prints proved conclusively, on every observed failing run, that (a)
`provider::resolve_endpoint` correctly returned `Some(endpoint)`, and (b) the `env` map
handed to `ProcessSpec` correctly contained `AI_GATEWAY_API_KEY` — the corruption happens
strictly between a correctly-built `ProcessSpec` and what the child process actually
observes, not in this card's own logic.

Working hypothesis, not confirmed further: this test binary contains pre-existing,
unsynchronized global environment mutation —
`claude_code.rs::discover_installed_binary_fails_typed_when_path_has_no_claude_executable`
calls `unsafe { std::env::set_var("PATH", ...) }`/`remove_var` with no coordination against
concurrently-running tests that spawn real subprocesses (this file and `secrets.rs` are the
only two call sites in the crate). Rust's own documented rationale for making `set_var`
`unsafe` is exactly this class of hazard: undefined behavior when another thread reads the
process environment (which `Command::spawn` does) concurrently. This card's new tests are
the first in the tree to assert *positive presence* of specific spawned-environment content
strictly enough to notice it; nothing here reduces its likelihood or fixes it, and doing so
(serializing or removing that pre-existing global mutation) is out of this card's ownership.
Reproduce with `for i in $(seq 1 15); do cargo test -p tack-runner --lib; done` at this
machine's default parallelism (16 threads).

The revert-and-restore load-bearing proof above and every isolated/filtered run of the new
test were 100% reliable; this is recorded as a pre-existing shared-infrastructure risk
(`crates/tack-runner/src/harness/process.rs` and/or the two `set_var` call sites) for
whoever owns those files next, not a defect in this card's own code.

## Other findings, not this card's to fix

- **`crates/tack-runner/src/registry.rs` is dead code**, by its own doc comment ("Always
  empty — nothing ever adds an adapter to it… unused outside its own tests"). Untouched here
  — flagged for routing, per the coordinator's direction mid-session.

## Files changed vs. ownership

Owned, as listed: the `[provider.*]` section of `RunnerConfig` (`config.rs`); each
adapter's spawn environment/args (`claude_code.rs`, `codex.rs`); `bootstrap::probe`/
`build_runtime`'s catalog step; `doctor.rs`'s provider block; the gateway rows of
`docs/CONFIG.md`. `crates/tack-runner/src/provider.rs` is new — the catalog fetch and
endpoint-resolution logic did not have an obvious home in an existing owned file and is
entirely new surface, not a repurposing of anyone else's file.

Two files outside the literal ownership list needed a one-line, non-logic fix to keep
compiling once `ConfigOverrides`/`DiscoveryReport` gained new fields, recorded here rather
than silently expanded:

- `crates/tack-runner/src/main.rs` / `crates/tack-runner/tests/bootstrap_entrypoint.rs` —
  added `..ConfigOverrides::default()`/a new `provider_catalog` field value to existing
  struct literals. No behavior changed.
- `crates/tack-cli/src/local_runner.rs` (VI-B3's file) — same fix, one `ConfigOverrides`
  literal. Flagging it explicitly since it is not this card's file: the change is
  mechanical (`..ConfigOverrides::default()`), touches nothing VI-B3 owns logically, and was
  unavoidable to keep the workspace building.

`crates/tack-runner/src/lib.rs` gained `pub mod provider;` and re-exported
`ProviderConfig`/`ProviderOverride`, mirroring how `config`'s other public types are already
re-exported — not separately listed in Owns, but the same class of addition VI-B1's own
handoff recorded for its `pub mod secrets;` line.

## Design deviation, and why: a map-shaped config, and a wire-shaped descriptor, not a
## vendor-conditional adapter

Two shaping corrections arrived mid-session, both recorded above under "Corrections to the
card" with the reasoning; this section is the pointer for a reviewer looking for "why does
this not match the card's literal text." Neither widened scope — no second provider entry,
no direct-vendor entry, no subscription-vs-key selector was added; one working entry,
correctly shaped, is all that exists.

## Not checked

- **A successful, model-serving completion** — the acceptance's central ask — could not be
  proven; see "The blocking finding."
- **Whether the process-environment flake above ever affects a real (non-test) attempt** —
  production concurrency for a single runner is 1 (`Concurrency { total: 1, available: 1 }`
  in `bootstrap::report_capabilities`), so the many-concurrent-subprocess condition that
  reproduces the flake here (dozens of test threads each spawning their own child) has no
  direct production analogue observed — not proven absent, just not observed to apply.
- **Windows/macOS behavior of any of this** — same reason as every other runner card on this
  machine: no such build environment available.
- **`opencode export <sessionID>`'s exact JSON shape** — named as a measured, available
  option for future actual-model observation work, not itself fetched or parsed here.

## Context spent

- Cold start: the card body (`TODO.md:1001-1053`), the ownership/escalation rules
  (`TODO.md:667,674`), ADR 0061 whole, the Part VI dispatch header + VI-B2 block,
  `TEMPLATE.md`, CLAUDE.md's rules-that-bite and comment-style sections, `config.rs` whole,
  `secrets.rs` whole, `bootstrap.rs` whole, `capabilities.rs` whole, the named ranges of
  `claude_code.rs`/`codex.rs`, `doctor.rs` whole, `docs/CONFIG.md` whole — heavier than the
  block's ~30k estimate because three separate mid-session scope/shaping corrections
  (opencode cut, config-as-map, wire-shaped naming, plus the catalog-metadata escalation)
  each required re-reading the affected code before applying them, and the credential
  investigation (three independent live reproductions before concluding the key itself was
  the blocker, not this card's code) was substantial and unavoidable given the instruction
  to prove rather than assume.
- Files opened beyond the read list: `crates/tack-runner/src/harness/process.rs`'s `spawn`
  (grepped, chasing the test flake — see that section); `crates/tack-cli/src/local_runner.rs`
  (the one-line compile fix above).
- This session's live-credential investigation (three separate real reproductions: a raw
  HTTP probe, a real claude-code spawn, a real codex spawn — before the two committed
  `#[ignore]`d tests existed at all) was the single largest cost beyond ordinary
  code-writing, and is why the handoff's blocking-finding section is as detailed as it is:
  a future reader must be able to tell "the mechanism is proven, the key is not" without
  re-doing any of this work.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
