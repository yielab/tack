# ADR 0061: Where a model provider's API key lives, and who's allowed to touch it

**Decide:** approve six rules about the coding-agent "runner" component — the small
worker that lives on your own machine and actually launches Claude Code / Codex /
OpenCode. In short: a provider's API key (like a Vercel AI Gateway key) is allowed to
live in the operating system's keychain on the runner's machine — or, where there is
none, in the runner's own private folder — never inside Tack's shared database or
logs; and a new UI switch is allowed to turn that runner on/off, but only when it's
running on your own machine with no network exposure.

**Why now:** two earlier decisions (ADR 0050, ADR 0058) correctly said "Tack's server
will never hold or forward a model API key." People — including this project's own
`tack runner doctor` output — have been reading that as "Tack can't help you configure
a model provider at all," which was never true and is the exact confusion users have
reported. Nobody had actually written down what the *runner* (a separate piece, not the
server) is allowed to hold, or how a key reaches it. This ADR writes that down.

**If you do nothing:** the next three pieces of work — a place to store a key, wiring
up Vercel AI Gateway as a model source, and a UI switch to turn the runner on — stay
blocked. Each would otherwise have to guess at an answer this ADR gives them.

## The six decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | A provider key lives only on the runner's own machine — in the operating system's keychain when there is one, otherwise in the runner's private, owner-only folder, and the runner says which — never in Tack's database, never in a log line. | Keeps the promise that Tack's server never holds or could leak a vendor credential, and matches what `gh` and `docker` already do on a developer's machine. |
| 2 | One narrow route lets the web UI hand a key to the runner sharing its machine — and it only exists when both are on that same machine, with no outside network access. | Lets a UI-only user paste a key without opening a terminal, without weakening the server's normal security for everyone else. |
| 3 | The runner itself asks the model gateway "what models do you offer," using its own key. Tack's server never talks to any model vendor. | Keeps the server's hands clean; the runner already talks to the network for its own job (checking out code, reporting progress). |
| 4 | The gateway gets its own name (`vercel-ai-gateway`) and keeps its own model-name spelling, kept separate from the same model reached directly (not through the gateway). | So a report never confuses "reached the vendor directly" with "reached them through the gateway" — cost and behavior can differ. |
| 5 | The board's existing table of "what has to stay a terminal command vs. what can move to the UI" becomes the standing rule for this project — not something each task re-decides for itself. | Keeps the console/UI split consistent and lets anyone check it later instead of re-deriving ten different agents' private judgment calls. |
| 6 | Add a one-click switch in the web UI to start/stop the built-in runner. It only works when Tack is running on your own machine (not exposed to a network), and it stays off until someone flips it. | Removes the one remaining step — typing a startup flag — that a UI-only user can't avoid today, without loosening any existing safety rule. |

If you accept this table, you have accepted the ADR — record the date below. Everything
past this point is supporting detail for whoever implements or later audits one of these
six calls; nothing above depends on anything below it.

---

- **Status:** accepted 2026-09-03 — recorded as a dated amendment at the bottom of
  `docs/agent-handoffs/part-vi/VI-A2.md`.
- **Date:** 2026-09-03
- **Relationship to earlier ADRs:** doesn't change ADR 0050 (`0050-runner-control-plane.md`)
  or the safety rules in ADR 0058 (`0058-standalone-single-binary-runner.md`) — decision 6
  adds a *second way* to reach the same on/off gate ADR 0058 already defined, without
  changing what that gate does.
- **Wire contract:** unaffected. `docs/contracts/runner-v1/` gains no new field from this
  ADR by itself.

## Full reasoning

*(For implementers and reviewers. If you only needed to approve the six decisions above,
you're done reading.)*

### Background

ADR 0050 decided that Tack's server schedules work and `tack-runner` executes it, and
that "the Tack API never starts a coding harness and never becomes a model proxy." ADR
0058 restated the credential half of that explicitly: "Vendor credentials remain outside
Tack." Both statements are about the **server's** own conduct, and both stay true,
unchanged by anything here.

Both have also been read more broadly than they were written — as "Tack, as a whole
product, offers no help with a model provider." `docs/CONFIG.md` said so outright
("there is no `TACK_*` variable for a model provider or endpoint, by design"), and
`tack runner doctor`'s own printed output repeats the same framing. That reading is
false: the **runner** side of this boundary was simply never decided. This ADR decides
it.

Three facts about the runner already exist and aren't up for debate here:

- The runner already keeps one credential of its own — its enrollment credential
  (`crates/tack-runner/src/config.rs`), stored in `TACK_RUNNER_STATE_DIR`, owner-only on
  disk, redacted from every log and debug print.
- Every adapter (Claude Code, Codex, OpenCode) already accepts a "here's a secret,
  fetch it yourself" field on a work request (`secret_reference`) and already refuses to
  resolve it — each one logs "no secret-store client exists yet" and skips it rather
  than inventing a value.
- The embedded runner's on/off switch is checked exactly once, at startup, before the
  server opens a network socket — there's no existing way to turn it on or off while
  the server is already running.

One existing precedent already proves the pattern decision 2 reuses: Settings → Cloud
Backup already lets an operator paste a secret (the S3 key) into the UI, stores it
write-only, and never echoes it back — only a "is one set?" boolean.

### 1. Where a provider key lives

**Chosen:** a runner-local store with two backends, tried in this order: the operating
system's credential store (macOS Keychain, Windows Credential Manager, Linux Secret
Service) through the `keyring` crate; and, only where no platform store answers — a
headless server, a container — an owner-only file next to the runner's existing
credential file, with the same locked-down permissions already proven for that folder.
The runner reports which backend holds a key (`tack runner doctor`, and the response to
the UI route in decision 2), so a file is never mistaken for a keychain. Entries are
named `<provider>/<label>`; the only label written today is `default`. What's fixed here
is that a key never appears in a log line, an error message, or any message the runner
sends back to the server — it only ever leaves the runner's own machine as something
the runner itself decides to send to a model vendor.

**Rejected — the owner-only file as the only backend.** It is what the harnesses
themselves do on Linux and would have been enough for one machine, but it is below the
level the same harnesses use on macOS, and below what `gh` and `docker` do everywhere:
try the platform keychain first, fall back to a file, say which one you used. The
keychain costs one dependency and no configuration in the normal case — one developer
on their own machine — and keeps the key out of every backup, sync folder and
`cat`-able path on it.

**Rejected — storing it in Tack's own database** (the same table Cloud Backup's secret
uses). That pattern is sound, but the *location* is wrong for this secret specifically:
Cloud Backup's key is the server's own secret, used by the server itself, already inside
its trust boundary. A model-provider key is not the server's secret — it exists so a
*different* machine can reach a model vendor. Storing it in Tack's database would make
the server a holder of vendor credentials, which is precisely the "becomes a model
proxy" line ADR 0050 forbids — holding the key is the violation, whether or not the
server ever uses it. It would also become a permanent addition to the backup-scrubbing
list this project already maintains (`remote_backup.rs::scrub_snapshot_secrets`), and
any future gap there would leak a vendor key into a cloud backup file — a worse outcome
than a plain-file leak on the one machine that already holds the runner's own
credential the same way.

**Rejected — a dedicated "vault" service inside Tack**, fetched by reference over the
API. This is the same problem by another name: anything that moves a provider key
through the server's own request/response traffic is visible in the API's own
documented shape, which is exactly the "model proxy" pattern ADR 0050 rules out. It also
buys real, unneeded complexity (encryption-at-rest key management, key rotation, an
audit of every backup/export path) to solve a problem — one runner, one key — that
doesn't need it yet.

### 2. The one loopback-only, write-once exception

**Chosen:** exactly one server route exists to hand a key to the runner sharing its
machine, and it exists **only** when both of these are true: the built-in runner is
turned on, and the server is reachable only from the same machine (no outside network
access). If either isn't true, the route doesn't exist at all — visitors get a plain
"not found," never an error that reveals the route is there but refused. The route
accepts a new key and confirms it was saved; it never reads one back, matching Cloud
Backup's own read view. Unlike Cloud Backup, a blank value is rejected outright rather
than treated as "keep what's there" — this route has no update-in-place behavior to
protect, and silently doing nothing on a blank input is a worse trap for a route whose
whole job is "the key went in."

**Auth:** the same everyday login token every other Tack API call already needs — not a
new, separate one. The higher-privilege tokens this project uses elsewhere exist for
actions that can affect a shared, possibly remote deployment; this route only exists at
all on a single machine that already has the runner turned on, which is squarely the
"one person, one machine" case this project already treats differently. A new,
separate token here would buy no real extra safety and would cost exactly what this ADR
is trying to remove: one more secret a UI-only user has to generate and keep track of.

**Rejected — no login check at all, since it's local-only anyway.** "Only reachable
from this machine" isn't the same as "only one person can use it" — this project's own
security rules already say any route moving a secret needs the login check regardless
of where it's reachable from, and this route hands a real vendor credential to a
process that will use it to reach the internet. No exception here.

### 3. Catalog fetching

**Chosen:** the runner itself asks the gateway which models it offers, using the
runner's own key — this is no different from the network calls the runner already makes
to check in with the server. The server issues zero requests to any model vendor,
catalog included. A catalog entry records only the provider name, the model's name, and
when the runner last checked — never a price. Whatever a gateway's catalog *says* a model
costs is a vendor's quoted number, not something this system actually measured running a
task — it gets a distinct label (`catalog_reported`) and must never be written into a
cost field that otherwise only ever means "measured," "estimated," or "not measured."
Blurring a vendor's list price with an actual measurement is exactly the kind of false
precision this project's "unmeasured stays explicitly unmeasured" rule exists to stop.

**Rejected — a hardcoded list of models somewhere in the codebase.** This project has
already been burned by exactly this pattern: several places in the docs asserted
something about the code that stopped being true the moment the code changed. A
hardcoded model list rots faster than any of those did, because vendor catalogs change
on their own schedule.

**Rejected — have the server fetch the catalog itself, using a server-held key.** Same
objection as decision 1's rejected vault: it requires the server to hold a provider key,
and it requires the server — often a small shared machine serving many runners — to make
outbound calls that belong, structurally, wherever the credential and the work already
are.

### 4. Vocabulary

**Chosen:** the gateway is named `vercel-ai-gateway` everywhere this system records a
provider, and a model's name reached through the gateway is kept exactly as the gateway
spells it — Tack never rewrites or shortens it. Reaching the same underlying model
directly versus through the gateway are treated as two different, never-merged
combinations (e.g., "Anthropic directly" and "Anthropic through the gateway" are not the
same row anywhere a report or a picker groups by provider).

**Rejected — reuse the underlying vendor's name with a "via gateway" flag bolted on.**
This breaks a rule the project already has (never conflate what was requested with what
actually happened) and is the kind of shortcut that falls apart the moment a second
gateway shows up or a runner's real report disagrees with the request.

**Rejected — no separate name at all, treat the gateway as invisible.** This would make
it impossible to ever say "this runner can reach Anthropic directly" and "this runner can
only reach Anthropic through the gateway" as two different capabilities — a distinction
both a scheduler and an honest cost report need.

### 5. The surface map is the standing rule, not a per-task judgment call

**Chosen:** the board's own table of what has to stay a console command versus what can
become a UI feature (`TODO.md` §VI.0) is adopted as-is, and its list of reasons is
considered closed. If a future task finds something else that "just can't be done in the
UI," that's a new entry requiring an amendment to this ADR (with the same kind of
concrete, structural reason the existing rows give — e.g. "needs an interactive
terminal login") — not a private call one task makes and writes down only in its own
notes.

**Rejected — leave it to whoever's doing the task at the time.** This is the exact
pattern that created the problem this whole effort exists to fix: starting the runner,
running the diagnostic tool, and checking whether a login worked were all left as
unplanned console-only steps, and none of them was ever written down as an intentional
choice — each just looks, to a newcomer, like something nobody got around to. Letting
each task decide for itself also means nobody can audit the current console/UI split
later without re-reading every task's private reasoning.

### 6. Turning the runner on from the UI — extending ADR 0058

**Chosen:** when Tack is reachable only from its own machine, one switch in the web UI
can start or stop the built-in runner; the choice is remembered so it survives a
restart. Both of ADR 0058's existing safety rules are unchanged: it's still off by
default (a fresh install runs nothing until someone flips the switch), and it's still
restricted to a machine-only connection (the switch's own route simply doesn't exist
otherwise). The existing startup flag keeps working exactly as it does today, for
scripts and automated tests.

This decision says a switch is allowed to exist — it doesn't by itself finish the
plumbing. Today, the runner's on/off check only happens once, at the moment the server
starts up; there's no existing way to start the runner role while the server is already
running. Building that — starting the runner as a supervised piece of the
already-running server, reusing the shutdown behavior that already exists rather than
writing a second version of it — is the next piece of work's job. If that turns out to
be harder than expected, that's a note back to this ADR, not a silent workaround.

**Rejected — leave the startup flag as the only way.** This is the one remaining step a
UI-only user can't avoid today, and leaving it in place undercuts the entire point of
having a single, self-contained binary in the first place.

**Rejected — make starting Tack with no arguments automatically turn the runner on.**
This would change a real safety default — today, just starting Tack runs no
agent-executing code at all — purely to save one click that a UI switch can already
offer instead. Left as an open question for later, not decided here.

## Consequences

- The one `docs/CONFIG.md` sentence that read as an absolute "Tack can't help with a
  provider" becomes the precise version: the **server** still never touches a provider
  key, but the **runner** is allowed to hold one, reached only through decision 2's
  route or the harness's own normal login.
- The next three pieces of work (the secret store, the Vercel AI Gateway integration,
  and the UI on/off switch) each implement one part of this ADR and can start once it's
  accepted, not before.
- The model catalog idea (decision 3) needs no database or wire-contract change by
  itself. If it turns out to need one later, that goes through this project's normal
  review process for changing a frozen contract — never smuggled in as an
  unreviewed side effect.
- Decision 6 doesn't change any of the runner's existing safety checks — it adds a
  second way to trigger the same rule, not a looser version of it.
- Other places in the docs and README still use the older, broader "Tack can't help with
  a provider" phrasing this ADR narrows. Those are tracked separately for the
  documentation cards to fix — this ADR only corrects the one sentence it owns.
- Nothing in the wire contract, its test fixtures, or any existing adapter code changes
  as a result of this ADR by itself.
