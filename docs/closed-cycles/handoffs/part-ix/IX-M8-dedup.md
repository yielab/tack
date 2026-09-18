# IX-M8-dedup handoff

- Base SHA / branch / final SHA: `fd8c901` (develop tip at dispatch) / `agent/ix-m8-dedup` / `fe1ff9f`
- Files changed (must equal ownership list): 35 files, exactly the test files named in the
  49 `duplicate-tests` pairs — 24 in `tack-api`, 5 in `tack-orch`, 2 in `tack-db`, 2 in
  `tack-cli`, 2 in `tack-runner`. No production file, no `TODO.md`, no
  `scripts/maintainability.py`/baseline touched.
- Contract fixtures consumed: none. `crates/tack-orch/tests/runner_contract/domain.rs` is
  named in one pair (`requested_and_actual_model_values_are_distinct_types`); only its test
  *name* changed — its assertions, fixtures and the pin table in
  `crates/tack-orch/tests/runner_contract.rs` are untouched.
- Behavior implemented: none — every commit is a `#[test]`/`#[tokio::test]` function rename.
  No production code, no assertion, no fixture changed anywhere.
- Tests added and exact commands/results: none added, none removed. `cargo nextest run
  --workspace` → `1413 tests run: 1413 passed, 8 skipped` (unchanged shape from before this
  card; `measure --totals`'s own test counter reads 1408 before and after, see *Measured
  numbers*).
- Failure/adversarial case proved: N/A — no behavior changed. The adversarial case for this
  card is naming: every renamed pair was re-run through
  `python3 scripts/maintainability.py duplicate-tests --top 50` and confirmed absent from
  the crate's own list, not just believed reduced.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `tack-api` (68.84%) and `tack-db` (68.28%)
  line coverage sit below their CI floor (70%) — see *Budget check*. This is pre-existing,
  not introduced here: a pure rename cannot move an executed-line count, and this card
  deleted zero tests, so the "put the test back" rule (§IX.1 rule 3 / the IX-M8 card's
  coverage guard) does not fire. Flagged for whichever `IX-M8-tack-api`/`IX-M8-tack-db`
  sub-card runs next, since CI's coverage job never runs on `develop` and nothing else was
  going to surface this.
- Secrets/logging review: N/A, no logging or secret-handling code touched.
- Safe merge order and likely conflicts: no ordering constraint against other Wave 32 cards
  — this card touches only test *names*, and per TODO.md's dispatch table it is meant to
  land first (it shrinks what the five `IX-M8-<crate>` cards then read). A crate sub-card
  that started from a pre-dedup checkout will conflict trivially (a `fn <old_name>` line);
  rebase onto this commit and take this side.
- Checklist: no unowned files touched (all 35 are named in a `duplicate-tests` pair or its
  file); no live secret; no panic stub; no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `duplicate-tests` returns 0 pairs workspace-wide | `python3 scripts/maintainability.py duplicate-tests --top 50` → `0 pairs in total` (see *Budget check* for the full per-crate breakdown) |
| No test behavior changed by this card | `cargo nextest run --workspace` → `1413 tests run: 1413 passed, 8 skipped`, same shape before and after; `git diff --stat develop...HEAD` shows only renamed `fn` identifiers (34 files, 1 changed line each in tack-orch/db/cli/runner, 1 line each in tack-api) |
| Every renamed test still states an accurate, distinct claim in ≤ 60 characters | Longest new name is 58 characters (`checksum_mismatch_aborts_store_streaming_before_commit`, `artifact_content_put_stages_nothing_on_checksum_mismatch`); each name was checked against the actual test body it labels (see `docs/agent-handoffs/part-ix/IX-M8-dedup.md`'s own working notes — not committed — for the full pair-by-pair reading) |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `python3 scripts/maintainability.py duplicate-tests --top 50` before: 49 pairs (39
  `tack-api`, 5 `tack-orch`, 3 `tack-db`, 1 `tack-cli`, 1 `tack-runner`).
- Same command after: `0 pairs in total` (every crate line reads `0 near-identical pairs`).
- `cargo nextest run --workspace`: `1413 tests run: 1413 passed, 8 skipped` both before and
  after (no test added or removed by this card).
- `measure --totals` before and after are byte-identical — see *Budget check*.

## What a stranger still cannot do

Reading `duplicate-tests` output alone still does not tell a stranger *why* two
near-identical names were kept apart rather than merged — that reasoning (two-layer pin vs.
genuinely distinct route) lives only in this handoff and the commit messages, not in the
code. A future near-duplicate pair in one of these same clusters (e.g. a sixth
`*_when_orch_disabled` router test) will need the same read-the-bodies judgment call again;
this card did not add a lint or doc comment that would catch it automatically — see
`.claude/scope-discipline.md`'s "no mechanism with no caller" rule, which is why no such
mechanism was added.

## Budget check

`python3 scripts/maintainability.py check --changed` (final tree):
```
✓ maintainability budgets hold (35 files checked)
```

`python3 scripts/maintainability.py measure --totals`:
```
before: totals: prod=55448 (comments 10993) test=71211 ratio=1.284 tests=1408 sleeps_in_tests=38 env_gated=0
after:  totals: prod=55448 (comments 10993) test=71211 ratio=1.284 tests=1408 sleeps_in_tests=38 env_gated=0
```
Unchanged, as expected — every commit is a same-length identifier swap, never a line added
or removed.

`python3 scripts/maintainability.py duplicate-tests --top 50` (final tree):
```
tack-api: 0 near-identical pairs across files (ratio > 0.75)

tack-cli: 0 near-identical pairs across files (ratio > 0.75)

tack-core: 0 near-identical pairs across files (ratio > 0.75)

tack-db: 0 near-identical pairs across files (ratio > 0.75)

tack-desktop: 0 near-identical pairs across files (ratio > 0.75)

tack-orch: 0 near-identical pairs across files (ratio > 0.75)

tack-runner: 0 near-identical pairs across files (ratio > 0.75)

0 pairs in total
```

`cargo fmt --all`: no diff beyond the renames themselves (checked via `git status
--porcelain` immediately after running it — no additional files touched).

`cargo nextest run --workspace`:
```
Starting 1413 tests across 39 binaries (8 tests skipped)
Summary [  17.230s] 1413 tests run: 1413 passed, 8 skipped
```

`./scripts/check-comments.sh && ./scripts/check-test-hygiene.sh`:
```
✓ no board archaeology in crates/ frontend/src frontend/e2e
✓ tests take their temporary paths from a guard
```

`cargo clippy --workspace --all-targets -- -D warnings`: clean, `Finished` with no warnings
printed.

Coverage guard (§IX.1 rule 3) — run for every crate touched, even though this card deleted
no test in any of them (so no floor failure here triggers "put the test back"; recorded for
transparency and for the next sub-card that owns each crate):

| Crate | Floor | `cargo llvm-cov -p <crate> --fail-under-lines <floor>` | Exit |
|---|---|---|---|
| tack-api | 70 | 68.84% (13647/4252 lines uncovered) | **1 (fails floor)** — pre-existing, see *Known limitations* |
| tack-orch | 70 | 90.75% | 0 |
| tack-db | 70 | 68.28% (5420/1719 lines uncovered) | **1 (fails floor)** — pre-existing, see *Known limitations* |
| tack-runner | 85 | 89.21% | 0 |
| tack-cli | none | 39.94% (measured for completeness; no floor defined) | n/a |

tack-api and tack-db were already below their CI floor before this card touched them — a
pure test-function rename cannot change which lines execute, and `measure --totals`
confirms zero lines added or removed. Nothing was put back because nothing was deleted.

## What was removed

Nothing was removed or merged; every pair was resolved by renaming (plan §2.2 rule 2 — the
name states its own claim) so both sides of every pair stay. On inspection each of the 49
pairs was one of:

- **A legitimately distinct claim about a different route, provider, or migration set**
  that happened to share wording (e.g. `tack-api`'s ten `*_when_orch_disabled` router tests,
  one per endpoint family; `tack-runner`'s two providers' identical
  `a_malformed_body_is_a_parse_error_not_a_panic`; `tack-db`'s `orphan_fk_insert_is_rejected`
  in both migration-range test files).
- **The same invariant pinned at exactly two layers** (a unit/domain-level test plus one
  router- or contract-level test) — rule 5 permits up to two layers, so both stay:
  `rework_denominator_excludes_stale_attempts` (unit) /
  `get_economics_summary_excludes_stale_rework_attempts` (router);
  `effective_body_limit_clamps_to_protocol_ceiling` (unit) /
  `runner_v1_router_enforces_configured_body_limit` (router);
  `checksum_mismatch_aborts_store_streaming_before_commit` (unit) /
  `artifact_content_put_stages_nothing_on_checksum_mismatch` (router);
  `model_id_types_are_distinct_so_swaps_fail_to_compile` (unit) /
  `requested_vs_actual_model_and_provider_types_differ` (runner-v1 contract fixture, name
  only touched);
  `create_execution_args_require_profile_snapshot_and_policy` (unit) /
  `mcp_create_execution_rejects_missing_profile_snapshot` (MCP end-to-end);
  `production_router_rejects_substituted_credentials` (full production router) /
  `scoped_routers_reject_substituted_operator_and_runner_auth` (scoped sub-routers).

No pair reached a third instance of the same invariant (rule 5's actual trigger for
deletion), and none was the only test exercising its behavior, so no deletion was
warranted anywhere. The full old-name -> new-name -> file table for all 49 pairs (rule 2
throughout) is in the commit bodies of the five per-crate commits on this branch
(`2d98759` tack-api, `9326acb` tack-orch, `cd94628` tack-db, `0cff779` tack-cli, `fe1ff9f`
tack-runner); `git log --stat agent/ix-m8-dedup` lists every touched file per commit.

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` has no diff:
```
$ git diff scripts/maintainability-baseline.json
(no output)
```

## Context spent

- Tokens read before the first edit (cold start): the read list in §IX.0/§IX.1
  (~2.5k), the IX-M8 card block (~600), plan §2.2 (~600), this handoff's template and the
  part-6 body template (~500), and the `duplicate-tests --top 50` output itself (~1.8k) —
  roughly in line with the dispatch README's ~300-token-plus-output estimate for this card.
- Context size at handoff: moderate — the bulk of the spend was reading the 49 pairs' test
  bodies (only, per the read-list restriction: no other test file, no production code
  beyond what a named test calls) to judge distinct-claim vs. two-layer-pin vs.
  third-layer-redundant, and iterating candidate names against `difflib.SequenceMatcher`
  offline before editing, to avoid repeated edit/re-run cycles against the real script.
- Files opened and not used: none beyond the 35 edited plus their pair partners already
  covered above — no exploratory reads outside the named pairs.
- Read-list lines that were wrong: none noticed. The dispatch README's per-card gate line
  for `M8-dedup` (`cargo nextest run --workspace && duplicate-tests`) undersells the actual
  gate needed — `check --changed`, `check-comments.sh`, `check-test-hygiene.sh` and
  `clippy` are also required per this card's own instructions and were run.

## Amendments

*(none yet)*
