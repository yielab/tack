# ADR 0061: Provider credentials and model catalogs at the runner boundary

- Status: proposed — pending the user's explicit acceptance (recorded with its date as an
  amendment to `docs/agent-handoffs/part-vi/VI-A2.md`). No Wave 15 card
  (VI-B1/VI-B2/VI-B3) may branch before that acceptance is recorded.
- Date: 2026-09-03
- Supersedes: nothing. Sits alongside ADR 0050 (`0050-runner-control-plane.md`) without
  changing it — decision 3 below reaffirms its l.29-30 statement and decision 4 reaffirms
  its "model ids are opaque and never tiered or parsed" consequence. **Amends** ADR 0058
  (`0058-standalone-single-binary-runner.md`) in one place: decision 6 below adds a second,
  UI-driven path to the on/off gate its Safety posture section describes at l.69-75,
  without changing either the off-by-default or the loopback-only rule stated there.
- Contract: `docs/contracts/runner-v1/` — unaffected by this decision. Nothing here adds,
  removes or changes a wire field. `ModelCombination` (`crates/tack-orch/src/execution/capabilities.rs:55-60`)
  already carries a flattened `additional` map for forward-compatible fields; whether a
  gateway catalog needs a new named field is a question for whichever card implements
  decision 3, and per `TODO.md` §VI.2's ownership table that escalation has no default
  owner — it is raised with evidence against the fixtures, never smuggled through
  `additional` to dodge review.

## Context

ADR 0050 decided who owns what across the Tack API and `tack-runner`: Tack schedules,
`tack-runner` executes, and "the Tack API never starts a coding harness and never becomes
a model proxy" (l.29-30). ADR 0058 extended that split into a single binary and restated
the vendor-credential half of it explicitly: "Vendor credentials remain outside Tack. …
Tack does not read, store, forward or proxy them" (l.80-83). Both statements are about the
API server's own conduct, and both remain true today, unmodified by anything below.

Both have also been read as a broader claim they never made: that Tack, as a product,
offers no help choosing or configuring a model provider. `docs/CONFIG.md:102-104` states
outright "there is no `TACK_*` variable for a model provider or endpoint, by design" —
true of the API server's environment, and the only sentence on the subject most readers
will find. `tack runner doctor`'s own rendered output closes with a line paraphrasing the
same two ADRs (`docs/agent-handoffs/part-iv/IV-A5.md`, "Tack does not proxy model
providers"). IV-A5's own card diagnosed this precisely as a visibility problem and fixed
only the visibility — it made doctor's per-harness credential notes accurate and did not,
because it was not its job to, decide what the runner itself may hold. That decision has
never been written down. This ADR writes it.

The runner side of the boundary already has real, load-bearing facts that predate this
ADR and are not up for revision here:

- `tack-runner` already owns a credential of its own — the enrollment credential in
  `crates/tack-runner/src/config.rs` (`EnrollmentCredential`, redacted in `Debug`/`Display`
  at l.11-34), stored under `TACK_RUNNER_STATE_DIR` (`state_dir`, l.43/60/111) alongside
  `session.json`, owner-only (`docs/CONFIG.md:85-87`, confirmed with `stat -c '%a'` against
  a real run, not assumed).
- Every adapter already accepts a `secret_reference` environment entry on the execution
  request and already refuses to resolve it, on record, in its own source: `claude_code.rs`
  warns "no secret-store client exists in this crate yet" (l.925-945) and skips the entry
  rather than fabricating a value; `codex.rs` documents the same gap inline at l.765-775
  ("`secret_reference` entries are deliberately never resolved here: no secret-store client
  exists in tack-runner yet").
- The embedded runner's on/off gate is already a runtime, environment-driven decision —
  `local_runner::with_runner_enabled` (l.67-70) — checked once, before the server binds a
  socket, by `ensure_loopback` (l.121-131), which refuses to start a non-loopback embedded
  runner outright rather than downgrading it silently.
- There is one existing precedent for a UI-written secret anywhere in this codebase:
  Settings → Cloud Backup's `secret_key` (`crates/tack-api/src/handlers/settings.rs`). It
  is stored server-side in `app_meta` (l.65), never round-tripped back to the client — the
  read view replaces it with a boolean, `"secret_key_set": cfg.backup_secret_key.is_some()`
  (l.107-116) — and a blank value on write means "keep what is already stored" (l.138-139).
  Decision 1 below explains why a provider key does not reuse this pattern even though the
  pattern itself (write-only, boolean-masked, blank-keeps-existing) is sound and is reused
  in decision 2.
- No storage exists yet for a project's default model choice (`crates/tack-orch/src/model_policy/wiring.rs:17-23`,
  `resolve_request_model_policy` always returns `None` for that tier) — a fact this ADR
  does not change; VI-C3 owns that column.

Six questions follow from these facts, none of them answered anywhere in the tree today.
This ADR answers all six so that Wave 15 (VI-B1 secret store, VI-B2 Vercel AI Gateway,
VI-B3 the UI on/off switch and key-entry route) implements a decision rather than guessing
one, and so that Wave 16/17's UI work has a named boundary to build against.

## Measurement

Every number below is quoted from the board's own evidence table (`TODO.md` §VI.0,
measured 2026-09-03) or read directly from the cited source; none is estimated for this
ADR.

- `secret_reference` environment entries: accepted by the runner-v1 contract, resolved by
  **zero** of the three adapters — confirmed independently in `claude_code.rs:925-945`,
  `codex.rs:765-775`, and (per the board's table) `opencode.rs:1156-1158`.
- The model choice already reaches the harness once resolved: `--model` is passed by all
  three adapters (`codex.rs:753`, `claude_code.rs:968`, `opencode.rs:1135`, per the board's
  table) — the gap this ADR closes is *which provider supplies the key*, not whether a
  chosen model reaches the process.
- Exactly one UI-written secret exists in the whole tree today (Cloud Backup's
  `secret_key`, above); zero UI-written provider credentials exist.
- The embedded-runner gate is checked exactly once, at `local_runner::serve_with_embedded_runner`
  call time (before `ensure_loopback`), not re-checked or re-readable at any later point in
  the process's life — there is no existing code path that starts the runner role inside an
  already-running plain `tack serve`. Decision 6 depends on this fact directly.
- Vercel AI Gateway publishes a dedicated coding-agent endpoint for all three harnesses in
  this tree (`vercel.com/docs/ai-gateway/coding-agents`, fetched 2026-09-03 by the board —
  **re-fetch before relying on it**, per the board's own caveat). This ADR treats that as
  the reason a gateway is the second provider worth building (`TODO.md` §VI.0, "Why one
  provider, and why this one"), not as a fact this ADR re-verifies.

## Decision

### 1. Where a provider key lives

**A runner-local, owner-only store, alongside `session.json`, in `TACK_RUNNER_STATE_DIR`,
with the same `0600` file / `0700` directory posture already proven for that directory.**
The store's shape (one file, a small encrypted-or-plain keyed map, its own module) is
VI-B1's to design against `crates/tack-runner/src/config.rs`'s existing patterns
(`RunnerConfig`, `EnrollmentCredential`'s redacted `Debug`/`Display`); this ADR fixes only
where it lives and what it must never do: never appear in a log line, an error message, or
any `runner-v1` protocol frame, and never leave the runner's own filesystem except as an
opaque `secret_reference` the runner itself resolves locally before spawning a harness.

**Rejected: `app_meta`, the Cloud Backup pattern.** The pattern (write-only, masked on
read, blank-keeps-existing) is sound and decision 2 reuses it for the *route's* auth
model — but the storage location is wrong for this secret specifically, and the difference
is not cosmetic. The backup `secret_key` is **the server's own** secret: `tack-api` is the
sole consumer, uses it itself to reach the operator's configured backup target, and
already lives inside the trust boundary ADR 0050/0058 draw around the API server. A
provider key is not the server's secret — it exists so a *different* process
(`tack-runner`, possibly on a different machine) can reach a model vendor. Storing it in
`app_meta` would make the Tack API the holder of a vendor credential, which is the
"becomes a model proxy" line ADR 0050 forbids at l.29-30, regardless of whether the server
ever uses the key to make a call itself — holding it is the crossing, not using it. It also
costs a specific, checkable obligation this repo already enforces: `CLAUDE.md`'s posture
rule requires "every new secret column is added to `remote_backup.rs::scrub_snapshot_secrets`
in the same commit" precisely because `tack.db` (and therefore `app_meta`) is included in
remote backup snapshots. A provider key stored there becomes a permanent line item in that
scrub list, and any future gap in it leaks a vendor key into a cloud backup artifact — a
strictly worse blast radius than a plain-file leak on the one machine that already holds
the runner's own enrollment credential in the same posture.

**Rejected: a Tack-side vault** (a dedicated encrypted secret table, fetched by reference
over the operator API). This is a proxy by another name: any component that mediates a
provider key between where an operator types it and where a runner uses it, over the
**operator** API, puts model-provider secrets in `/api`'s own request/response schema
(visible in `docs/openapi.json`) — the exact "model proxy" shape ADR 0050 excludes, and in
the wrong direction for a card whose purpose is fixing a *misreading* of that exclusion.
It also costs real, unbounded scope this Part explicitly does not need: encryption-at-rest
key management, a rotation UX, and an audit of every export/backup path that currently
assumes `tack.db` holds no vendor secret — building for a fifth case
(`.claude/scope-discipline.md` rule 3) when the second case (one runner-local key) is what
Wave 15 needs to prove.

### 2. The one loopback-only, write-once exception

**One operator route exists to hand a key to the co-located runner's own store, and it
does not exist under any other condition.** Its preconditions: the embedded runner is
enabled (decision 6's switch, or `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE`) **and** the
server is bound to loopback — the same `AppConfig::binds_loopback()` check
`local_runner::ensure_loopback` (l.121-131) already performs before opening a socket, run
again here per-request rather than assumed from startup. When either precondition is
false, the route **does not exist** — a `404`, the same shape ADR 0060's own measurement
already establishes for `TACK_ORCH_ENABLE`-gated routes ("unset, the reconciler never
spawns and every `orch_*`/`/api/fleet` route 404s"), never a `403` that confirms the route
is real. It returns an acknowledgement only — never the key, never a fingerprint of it,
matching (not exceeding) the Cloud Backup precedent's own boolean-only read view. A blank
write is rejected outright rather than reused as "keep existing," because unlike the
backup secret this route is write-once by design: decision 1's store has no update-in-place
UX to defend, and a silent no-op on blank input is a worse trap for a route whose whole
job is "the key went in."

**Auth: the ordinary operator token (`require_token`/`TACK_API_TOKEN`), matching Cloud
Backup's own secret-key write** — not a new, dedicated token in the shape of
`TACK_ORCH_APPROVAL_TOKEN` or `TACK_EXECUTION_DECISION_TOKEN`. Those tokens exist for
routes that approve remote code execution or mutate fleet-wide state on a
possibly-shared, non-loopback deployment; this route only exists at all on a loopback
bind with the embedded runner already on — exactly ADR 0059's "operator and only user of
the machine are the same person" case, where `require_token` may already be unset by
design in pure-local mode, and where an attacker with local operator-token access already
has a strictly easier path to the same secret (reading `TACK_RUNNER_STATE_DIR` directly).
**Rejected: a distinct, higher-privilege token.** It would buy no additional containment
on the one deployment shape this route exists for, and it costs exactly what this ADR
exists to remove — a second secret a UI-only user must generate, store and document before
they can do the one thing this card is meant to make possible without a console.
**Rejected: no auth check because loopback already implies trust.** Loopback is not
single-user; CLAUDE.md's posture rule treats every route that moves privileged material as
requiring a token regardless of bind, and this route moves a vendor credential into a
process that will use it to reach the network on the operator's behalf. The route follows
the same `require_token` rule as everything else — no special case, in either direction.

### 3. Catalog fetching

**The runner's probe calls the gateway's model-list endpoint with the runner's own key.**
This is runner-side network activity — already the runner's domain since it already opens
outbound connections to enroll, heartbeat and report — not a new capability. It keeps ADR
0050 l.29-30 literally true at the transport layer: the API server issues zero requests to
any model-provider or gateway endpoint, catalog or otherwise, consistent with l.99's "v1
deliberately excludes … a Tack model gateway." A catalog entry records exactly three
things: `provider`, `model_id` (decision 4's vocabulary), and `probed_at` — when the
runner's own probe last saw the vendor list it. It does **not** record a price as a cost:
whatever number a gateway's catalog reports for a model is a vendor quote, not an
observation of any run this system executed, so it is tagged `catalog_reported` and must
never be written into a `cost`/`usage` field whose other legal values are `measured`,
`estimated` or `not_measured` (ADR 0050's own Consequences) — conflating a vendor's list
price with something this system measured would be exactly the kind of unearned precision
`CLAUDE.md`'s "unmeasured is nullable" rule exists to prevent.

**Rejected: a hardcoded, static model list anywhere in the tree.** §VI.1 rule 5 ("catalogs
are measured, never typed in") already forecloses this, and the evidence table already
shows the cost of the alternative: four places today where docs assert something the code
does not do (`agent-runners.md`'s harness id, its "no runtime effect" claim, its "no write
route" claim, per `TODO.md` §VI.0's evidence table) — every one of them a hand-typed claim
that rotted the moment the code moved. A static model list is the same failure mode aimed
at a target that changes faster (vendor catalogs) than this repo's own code does.
**Rejected: run the probe from the API server using a server-held key.** This is decision
1's rejected vault, restated for one endpoint instead of every call: it requires the API
server to hold a provider key (forbidden by decision 1) and requires that server —
frequently the shared, small-VPS board in §VI.0's own "one board, many runners" story — to
reach an outbound vendor endpoint on behalf of every runner, when the entire point of the
runner boundary is that outbound vendor traffic happens where the credential and the
workload already are.

### 4. Vocabulary

**The gateway's provider id is `vercel-ai-gateway`**, used verbatim in
`model_combinations`, `requested_model_provider`/`requested_model_id`, and any `actual`
column a harness report resolves to. **A gateway model id is the gateway's own
`creator/model` string, unmodified** — opaque, per ADR 0050's Consequences ("Model ids are
opaque and never tiered or parsed"); Tack does not split, canonicalize or re-key it.
**Reaching the same underlying model directly versus through the gateway are two different
`(provider, model_id)` pairs**, per §VI.1 rule 6 ("requested and actual stay distinct" /
"two different pairs") — `(anthropic, claude-…)` and `(vercel-ai-gateway,
anthropic/claude-…)` are never merged, deduplicated, or treated as aliases of one another
anywhere a report, a cost rollup, or a picker groups by provider/model.

**Rejected: reuse the underlying vendor's provider id with a `via_gateway` boolean.**
This directly breaks the invariant §VI.1 rule 6 already decided before this ADR existed,
and it reintroduces the requested/actual conflation that rule was written to prevent: a
boolean bolted onto an existing provider id is exactly the kind of "parse it out later"
shortcut that breaks the moment a second gateway is added or a runner's report disagrees
with what was requested. **Rejected: no distinct provider id at all**, treating the
gateway as an invisible transport under whichever vendor id the request already carries —
costs the same thing more completely: `model_combinations` keyed only by vendor id could
never express "this runner reaches Anthropic directly" and "this runner reaches Anthropic
through the gateway" as two different capabilities, which is precisely the distinction a
capacity-aware scheduler and an honest cost report both need.

### 5. The surface map is the product rule, not a per-card judgment call

**`TODO.md` §VI.0's surface-map table is adopted as written and its "why not fully UI"
column is closed.** A future step that cannot be moved to the UI needs an amendment to
this ADR (or a new one) naming the structural reason, exactly as the table's existing rows
do (OAuth device flows need a TTY; external binaries install outside Tack) — it is not a
judgment an implementing card makes for itself and records only in its own handoff.

**Rejected: leave it to each card**, the status quo before this ADR. This is the
mechanism that produced the very problem this Part exists to fix: starting the runner,
running `tack runner doctor`, and checking a harness's own login state are all
console-only today, and none of the three was ever written down as an intentional
boundary — each reads, to a stranger, as an oversight rather than a decision, because
no two of them share a recorded rationale. Per-card discretion also cannot be audited
later: a reviewer checking whether the current console/UI split is still correct would
need to re-derive each card's private reasoning rather than checking one table. Routing
every future exception through an ADR amendment costs one extra step per exception and
buys exactly the audit trail this Part's own evidence table shows was missing.

### 6. Turning the embedded runner on from the UI — an amendment to ADR 0058

**On a loopback bind, one switch in the UI may start or stop the in-process runner; the
choice persists in `app_meta`, read by the server after the database opens** — reusing the
same storage mechanism Cloud Backup's config already proves out (`settings.rs`), applied
here to a boolean rather than a secret. **Both of ADR 0058's Safety-posture rules (l.69-75)
are unchanged by this amendment**: off by default — a fresh install runs no harness process
until a person on that machine flips the switch — and loopback-only — the switch's own
route does not exist on any other bind, in the same 404-not-403 shape as decision 2.
`--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` remain, unchanged, as the flag-only equivalent
`scripts/smoke.sh` and any script keep using.

This amendment decides *that* a UI toggle exists; it does not itself close the gap between
that decision and the code as measured above. Today `with_runner_enabled`/`ensure_loopback`
are evaluated exactly once, before `tack serve` binds a socket — there is no existing path
that starts the runner role inside an already-running plain server. A UI switch flipped
after startup therefore needs the runner started as a supervised task against the bind the
running server already holds (`AppConfig::binds_loopback()` is knowable at that point,
not only at startup), reusing `supervise`/`ensure_runner_credential`'s existing shutdown-
coupling rather than a second implementation of it. That wiring is VI-B3's to build; if it
finds the boundary cannot be crossed the way this decision assumes, that is an amendment to
this ADR, not a silent redesign inside a card.

**Rejected: keep the flag as the only path.** Named directly by the evidence this Part is
built on: it is the one console step a UI-only user cannot avoid, and leaving it in place
costs ADR 0058's own stated goal — "one binary, one command, no second binary and no
copied token" — for exactly the audience that goal was written for. **Rejected: make bare
`tack` mean `serve --with-runner` on loopback.** This changes a security default — today
running bare `tack` executes no agent code; this would make the default invocation launch
an arbitrary code-execution subprocess service — to save the one click a UI switch can
already offer. `TODO.md` §VI.5 already records this as an open question; this ADR declines
to fold it in as a side effect of deciding the switch exists.

## Consequences

- `docs/CONFIG.md`'s provider bullet (l.85-108) changes from an absolute negation to the
  precise rule decisions 1-4 state: no `TACK_*` variable on the **API server** names a
  model provider or endpoint, and the API server still never holds, forwards or proxies a
  provider credential — but the runner may hold one, in its own state directory, reached
  only through decision 2's loopback exception or the harness's own configuration.
- VI-B1, VI-B2 and VI-B3 each implement one slice of decisions 1, 2/6, and 3/4
  respectively and are unblocked by this ADR's acceptance, not before.
- The catalog concept (decision 3) adds no schema and no contract field by itself. If
  VI-B2 finds it genuinely needs one, `TODO.md` §VI.2 already names that escalation path
  (no default owner for `docs/contracts/runner-v1/**`) and this ADR does not shortcut it.
- Decision 6 does not change `AppConfig::binds_loopback()`, `ensure_loopback`, or the
  existing `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` flag — VI-B3 adds a second caller of
  the same rule, it does not relax the rule.
- `docs/`, `README.md` and `tack runner doctor`'s own rendered text still contain the
  negation framing this ADR narrows. Every sentence found by
  `grep -rn "never becomes a model proxy\|no TACK_\* variable for a model provider\|never
  reads, stores, or forwards" docs/ README.md` other than this ADR and ADR 0050/0058
  themselves is listed in this card's handoff for VI-A1/VI-D1 to correct; this ADR corrects
  only the one bullet it owns in `docs/CONFIG.md`.
- Nothing in `docs/contracts/runner-v1/`, `crates/tack-orch/tests/runner_contract.rs`, or
  any adapter's spawn path changes as a result of this ADR. `git diff` against `develop`
  for this card is exactly one new file (this ADR) plus the one `docs/CONFIG.md` sentence.
