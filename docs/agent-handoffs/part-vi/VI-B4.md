# VI-B4 handoff

- Base SHA / branch / final SHA: base `f2bb5bd` (`develop` tip at dispatch) / `agent/vi-b4-provider-trait` / not yet committed at time of writing (see below).
- Files changed (must equal ownership list): does **not** equal the ownership list exactly —
  see "Files changed, and why three fall outside Owns" below.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/**` and
  `crates/tack-orch/src/execution.rs` were not touched; `runner_contract` is byte-identical
  (18/18 — see Measured numbers).
- Behavior implemented: a `Provider` trait + registry replacing `provider.rs`'s hardcoded
  `match` arms; a second real provider module for Anthropic's own API; `attach_catalog`/
  `resolve_endpoint` walk the registry instead of naming Vercel; `tack runner doctor` prints
  one block per registered provider with aggregate price/context-window counts.
- Tests added and exact commands/results: see Measured numbers.
- Failure/adversarial case proved:
  `provider::tests::one_providers_unresolvable_secret_never_suppresses_the_others_catalog`
  proves a provider with an unresolvable secret gets its own typed `SecretUnresolved` status
  while a second, working provider's catalog and `model_combinations` entry still arrive —
  fakes, no network, one assertion per outcome (status map, not just a status code).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: Anthropic's own `/v1/models` publishes no price
  field at all, so every `CatalogEntry.price` from that provider is `None` — not a gap in
  this parser, the vendor's own catalog has nothing there (see anthropic.rs's module doc).
  Anthropic's catalog fetch is a single page (`limit=1000`, no pagination loop) — recorded,
  not built, since nothing in scope needs it and the vendor's current catalog is far smaller.
- Secrets/logging review: `resolve_endpoint`/`attach_one_catalog` log the secret *entry name*
  only (`tracing::debug!(secret = %config.secret, ...)`), unchanged pattern from before this
  card. The live doctor run below shows no key in its output. Not independently re-proven via
  a fresh `sqlite3 .dump`/log-grep pair this card (VI-B1/B2 already established the mechanism
  and this card changed no code on that path).
- Safe merge order and likely conflicts: independent of VI-B3 (disjoint files: this card never
  touches `local_runner.rs`, the embedded-runner UI wiring, or anything VI-B3 owns). Needs
  VI-B2 merged first per the board (already true — VI-B2 landed 2026-09-04). VI-B5 depends on
  this card's `CatalogEntry` shape existing.
- Checklist: no unowned files edited without explanation (see below), no live secret
  committed (checked: `git diff` contains no key material), no panic stub, no blind retry.

## Files changed, and why three fall outside Owns

Owns lists: `provider.rs`/its module tree, the catalog-printing part of `doctor.rs`, the
`[provider.*]` config shape, and this handoff. Actual diff:

```
crates/tack-cli/src/doctor.rs                    | 107 ++--   (owned: catalog rendering)
crates/tack-runner/src/bootstrap.rs              |  13 +-    (NOT in Owns — see below)
crates/tack-runner/src/config.rs                 | 100 ++-   (owned: [provider.*] shape)
crates/tack-runner/src/harness/claude_code.rs    |   1 +     (NOT in Owns — see below)
crates/tack-runner/src/provider.rs               | 399 ---   (owned: replaced)
crates/tack-runner/src/provider/anthropic.rs     | 180 +++
crates/tack-runner/src/provider/mod.rs           | 702 +++
crates/tack-runner/src/provider/vercel_ai_gateway.rs | 240 +++
docs/CONFIG.md                                   |   2 +     (NOT in Owns — see below)
```

- **`bootstrap.rs`**: `attach_catalog`'s return type changed from one `CatalogStatus` to
  `BTreeMap<String, CatalogStatus>` (the whole point of "per-provider status" in the card's
  own Tasks list) — `DiscoveryReport.provider_catalog`'s field type had to follow. This is the
  one direct caller of `attach_catalog` outside `provider/`; not editing it would not compile.
- **`claude_code.rs`, one line**: a real bug this card's own testing found, not scope creep —
  see "The naming collision" below. Escalating it instead of fixing it would have shipped a
  provider nobody could ever configure through claude-code.
- **`docs/CONFIG.md`, two rows**: documents the two new `TACK_RUNNER_PROVIDER_ANTHROPIC_*`
  env vars this card's config-shape change added, mirroring the existing Vercel rows exactly.
  CLAUDE.md names this file "the single authority" for `TACK_*` tables; leaving a new pair
  undocumented seemed worse than the two-line addition.

## The naming collision (found by running the full suite, not anticipated by the card)

`cargo nextest run --workspace` failed two pre-existing tests after the first working version
of this card: `claude_code::tests::validate_accepts_every_known_provider_family_case_insensitively`
and `harness::tests::the_same_fixture_completes_through_both_real_adapters`, both because they
pass `requested_model_provider: Some("anthropic")` expecting claude-code's own native-vendor
passthrough (its `KNOWN_PROVIDERS` whitelist: `anthropic`/`bedrock`/`vertex`/`foundry`, from
`claude_code.rs:107-122` — "anthropic" is literally claude-code's **default** vendor family,
used when a request names no provider at all). My first attempt named Tack's own new provider
`ANTHROPIC_PROVIDER = "anthropic"` too, so requesting that family now matched a *registered but
disabled* Tack provider and got rejected as `NotConfigured` instead of passing through as `None`
(the harness's own subscription mode).

Fix: `ANTHROPIC_PROVIDER` (the wire-level identifier — what a request's
`requested_model_provider` must equal to opt into *Tack's own configured* Anthropic endpoint)
is now `"anthropic-direct"`, not the bare vendor family name. `ANTHROPIC_CONFIG_KEY` (the
`[provider.anthropic]` TOML section name) is unaffected — it stays `"anthropic"`, since TOML
section names and wire-level provider strings are different namespaces and only the latter
collided. `claude_code.rs`'s `KNOWN_PROVIDERS` whitelist gained one line
(`crate::config::ANTHROPIC_PROVIDER`) so a request naming `"anthropic-direct"` is accepted by
`validate()` at all — without it, the new provider would be unreachable from claude-code
regardless of what `resolve_endpoint` says, since that whitelist check runs first. Both
previously-failing tests pass again, unmodified, once the rename landed.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A provider is a module behind one trait; no vendor name in the dispatch machinery | `grep` over `provider/mod.rs` (production code, before its `#[cfg(test)]` split), `doctor.rs` (same), and `bootstrap.rs` for `VERCEL_AI_GATEWAY_PROVIDER\|VERCEL_AI_GATEWAY_CONFIG_KEY\|ANTHROPIC_PROVIDER\|ANTHROPIC_CONFIG_KEY\|"vercel-ai-gateway"\|"anthropic-direct"` → zero hits (exit 1) in all three; reintroducing `crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY` into `attach_one_catalog` made the same grep return the line, then reverted — proven load-bearing |
| The Vercel path is unchanged | 5 pre-existing `provider.rs` tests (relocated verbatim into `provider/mod.rs`) pass unchanged; `runner_contract` 18/18 byte-identical; live fetch below |
| A live gateway attempt still completes | `TACK_RUN_LIVE_VERCEL_CATALOG_TEST=1 TACK_LIVE_VERCEL_AI_GATEWAY_KEY=<real key> cargo nextest run --workspace --run-ignored ignored-only -E 'test(vercel_ai_gateway::tests::live_)' --success-output=immediate` → `1 passed`; stderr: `live Vercel AI Gateway catalog: 373 models (352 priced, 355 publish a context window)` |
| A second real provider (Anthropic's own API) proves the trait | `provider/anthropic.rs` (180 new lines, its own module); a real `tack runner doctor` run on this machine (below) shows `Provider endpoint (anthropic): reaches: claude-code` (not codex — correctly asymmetric, since Anthropic's own API has no OpenAI-Responses endpoint) |
| A misconfigured provider never suppresses another's catalog | `provider::tests::one_providers_unresolvable_secret_never_suppresses_the_others_catalog` — fakes, no network; asserts both the status map and the `model_combinations` entry |
| `doctor` prints price/limit fields, `Not measured` where unpublished | doctor.rs test fixtures + live run below (`price: 352 of 373 models published a price (21 Not measured)` when live-configured; `Not measured` phrasing not exercised in the disabled-by-default transcript below since neither is enabled there — proven instead by `render_provider_does_not_panic_for_any_catalog_status`'s `Configured` case and by the `Configured`-status manual construction in that same test) |
| `doctor --json` is unchanged | Code path untouched (`serde_json::to_string_pretty(&report.capabilities)`); confirmed live below — no `provider_catalog` sibling field, same `RunnerCapabilities` shape as before |

## Measured numbers

- `cargo nextest run --workspace`: **1402 passed, 0 failed, 7 skipped** (6 pre-existing
  opt-in live tests + this card's 1 new one).
- `cargo nextest run --workspace -E 'binary(runner_contract)'`: **18/18**, byte-identical.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean (after `cargo fmt`).
- `./scripts/check-comments.sh`: clean.
- `./scripts/check-test-hygiene.sh`: clean (once staged — an unstaged deletion of the old
  `provider.rs` made its internal `git ls-files` grep warn about a file "not found"; this is a
  pre-commit-staging artifact, not a real hygiene finding).
- `git diff --cached --stat`: **8 files, 1297 insertions(+), 445 deletions(-)** (includes the
  deleted 399-line `provider.rs`); `provider/anthropic.rs` alone: **180 insertions, 1 new
  file**; `config.rs`: **96 insertions(+), 4 deletions(-)**; `harness/claude_code.rs`: **1
  insertion**. See "Files changed, and why three fall outside Owns" for the full stat and why
  the card's "no file outside its own module and one registry line" framing undersold this by
  two files — both explained there, neither is scope creep.
- **Live catalog fetch, re-measured against the real gateway today** (`curl` with a bearer
  token, and separately through the actual `VercelAiGateway::fetch_catalog` code path via the
  live test above — both agree): **373 models total, 352 publish a price (21 do not), 355
  publish `context_window` (18 do not)**. The 21-no-price figure matches decision 7's own
  number. **The 18-no-context-window figure does not match decision 7's stated "101 of 373" —
  measured twice, both times 18.** Recording this as a correction per the project's own
  "re-measure before you quote" rule rather than repeating the ADR's number; the catalog may
  simply have changed composition since that number was written, or it counted a different
  thing (`max_tokens` also shows 18 nulls in today's body, not 101 either).
- **`provider.rs`'s existing test count**: the card's Acceptance says "the four existing
  `provider.rs` tests pass untouched" — measured (`grep -c '#\[test\]' provider.rs` before this
  card): **5**, not 4. All five pass unchanged, relocated verbatim into `provider/mod.rs`.
- **A real `tack runner doctor` run on this dev machine** (codex 0.149.1 and claude-code
  2.1.261 both installed, neither provider configured):

  ```
  Provider endpoint (vercel_ai_gateway):
    reaches: claude-code, codex
    status:  not configured

  Provider endpoint (anthropic):
    reaches: claude-code
    status:  not configured
  ```

  and `tack runner doctor --json` still emits exactly `RunnerCapabilities` (protocol_version,
  runner_version, harnesses[], features, limits — no sibling `provider_catalog` field), with
  both harnesses' `model_combinations: []` since neither provider is enabled.

## What a stranger still cannot do

Get Anthropic's per-model price or context-window data onto the actual Tack board: those
numbers are aggregated into `tack runner doctor`'s human-readable output (`price: N of M...`,
`limit: N of M...`) but never reach `RunnerCapabilities`/`ModelCombination` (the wire type
stays ids-only, exactly as before — VI-B5 owns extending it). A stranger also still cannot
configure either provider through any UI screen — both are TOML/environment-variable only
today, the same limitation the Vercel provider already had before VI-B3/VI-C1 land. And a
stranger cannot point codex at Anthropic's own API: `Anthropic::endpoint(Wire::OpenAiResponses)`
is `None` by design, since Anthropic's API does not speak that wire at all.

## Surface-map delta

Not independently re-checked this card — §VI.0 was not in this card's read list. By the same
reasoning VI-B2 recorded for its own card (runner-side machinery and a CLI command, not a UI
surface), this card should move no row either; if that reasoning was wrong for VI-B2 it is
wrong here in the same way, which is itself worth flagging to whoever verifies board rows.

## Context spent

- Tokens read before the first edit (cold start): not measured with a tool; by content
  volume, roughly in line with the block's ~16k estimate (board prelude + card, ADR excerpt,
  `provider.rs` whole, the named greps).
- Context size at handoff: not measured with a tool; well under the ~150k stop threshold —
  the large token counts in this transcript are mostly `cargo build`/`nextest`/`clippy`
  dependency-compilation noise, not durable context.
- Files opened beyond the read list, and why:
  - `crates/tack-runner/src/config.rs` **whole** (428 lines), not just the named grep range —
    needed to edit `defaults()`/`environment_overrides()`/tests correctly without guessing at
    surrounding structure (`FileConfig`, `apply()`). Read, not blindly patched.
  - `crates/tack-runner/src/harness/claude_code.rs`, ~25 lines across two new targeted greps
    (`KNOWN_PROVIDERS` at 107-122, `parsed_from_result_line`'s default at 605-635, the two live
    gateway tests at 2725-2920) — not the read list's ranges, but a direct consequence of
    chasing the two test failures the naming collision caused. Still targeted grep-then-range,
    never the whole file.
- Read-list lines that were wrong: `grep -n -A 12 -i "measured vendor" docs/agent-handoffs/part-vi/VI-B2.md`
  (the exact command the block names) returns **nothing** — VI-B2.md's actual heading is
  "Vendor table, measured", not "measured vendor". Used a broader `-i` grep over the same file
  only to find the equivalent section; never opened another handoff.

## Amendments

*(none yet)*
