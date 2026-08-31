# ADR 0058: Standalone single-binary operation via an embedded runner

- Status: accepted
- Date: 2026-08-26
- Supersedes: nothing. **Refines** ADR 0050 (`0050-runner-control-plane.md`) without
  changing any of its rules.
- Contract: `docs/contracts/runner-v1/` — unchanged by this decision.

## Context

ADR 0050 made Tack the plan of record and `tack-runner` the process owner, and shipped
them as two binaries. That separation is correct and is not in question here.

What the two-binary packaging costs, however, is the product's own stated principle — a
single `tack` binary — and it costs it in the most common case: one developer, one
machine, wanting an agent to run against their own board. Today that person must build or
install a second binary, create a pending runner, copy a one-time token out of terminal
output, export three environment variables, and keep a second process alive. Four manual
steps and two artifacts to get to the thing the product is for.

The friction was reported directly: the split "es totalmente antiintuitivo" and "muy
fraccionado para la mayoría de usos" against "su principio final de un solo binario".

The observation that resolves it is that **ADR 0050 separates roles, not binaries.** Its
rules are about who schedules, who owns processes, who holds credentials, and which
direction connections are opened. None of them are statements about how many executables
are shipped. The `tack` binary already hosts two distinct roles — HTTP server (`tack
serve`) and API client (every other subcommand) — chosen by subcommand at startup. A
third role is consistent with that design, not a departure from it.

## Decision

**One binary may host the runner role.** `tack` gains `tack runner start`, which runs
exactly what the `tack-runner` binary runs, through the same code path. The standalone
`tack-runner` binary is retained for deploying a runner on a machine that has no server.

**`tack serve --with-runner` runs both roles in one process.** The embedded runner is a
task in the server's own process and lives and dies with it.

**The embedded runner speaks runner-v1 over loopback HTTP, exactly like a remote runner.**
It does not call handlers in-process, does not share the server's `AppState`, and does not
receive a privileged path of any kind. This is the load-bearing constraint of this ADR and
the reason it does not weaken ADR 0050: there remains exactly **one** implementation of
the runner protocol client and exactly **one** server-side path that serves it. An
in-process shortcut would create a second path that could silently diverge from
`docs/contracts/runner-v1/`, which the contract fixtures — authoritative over any Rust
type — exist specifically to prevent. It would also be a path `scripts/smoke.sh` does not
exercise, so the mode most users would run would be the mode least proven. On a local
machine the cost of loopback HTTP is not measurable against a harness subprocess that runs
for seconds to minutes.

**The composition root moves into the library.** `tack-runner`'s binary `main` becomes
argument parsing over a public entry point that both it and the embedder call. There is
one wiring of adapters, capabilities, protocol, engine, journal and workspace — not two
that can drift.

**`tack-api` does not learn that runners can be embedded.** The composition happens in
`tack-cli`, which already depends on `tack-api`; it gains a dependency on `tack-runner`.
`tack-api` never gains one. ADR 0050's "the Tack API never starts a coding harness"
therefore remains literally true of the API crate: the *binary* composes two independent
roles that reach each other over HTTP.

## Safety posture

An embedded runner executes arbitrary coding-agent processes on the host that serves the
UI. That is strictly more dangerous than any capability Tack currently gates, so it
carries the strictest posture in the codebase:

- **Off by default**, behind an explicit opt-in (`--with-runner` /
  `TACK_LOCAL_RUNNER_ENABLE`), consistent with the repository rule that anything which
  deletes data or reaches the network is off-by-default behind a `TACK_*_ENABLE` gate.
- **Auto-enrollment is refused when the server is not bound to loopback.** A Tack instance
  served to a team on `0.0.0.0` plus a self-enrolling agent executor is a remote-code-
  execution surface, not a convenience. `AppConfig::binds_loopback()` already exists and is
  the check. Refusal is a startup error, never a downgrade to a silent non-runner mode.
- **Credential handling is unchanged.** The embedded runner redeems a one-time enrollment
  token through the real protocol and stores a durable credential owner-only in its own
  state directory. Only hashes are stored server-side. Nothing is passed in memory that a
  remote runner would have had to earn.
- **Vendor credentials remain outside Tack.** Provider keys (Anthropic, OpenAI, OpenRouter,
  a local endpoint) stay in the harness's own environment on the machine running the
  runner role. Tack does not read, store, forward or proxy them — ADR 0050's exclusion of a
  Tack model gateway is untouched and remains a deliberate non-goal.

## Consequences

- The common case becomes one command: `tack serve --with-runner`, from nothing to a
  completed attempt, with no second binary and no copied token.
- The `tack` binary grows. `tack` is 18.4 MB and `tack-runner` 4.5 MB at
  `0.1.0-beta.6`; most of the runner's weight (tokio, reqwest, serde, `tack-orch`) is
  already linked into `tack`, so the delta is expected to be far below 4.5 MB. The real
  number is measured and recorded by the card that lands it, never estimated in the docs.
- Distributed operation is unaffected. A fleet of remote runners works exactly as before,
  through the same protocol and the same client.
- One new failure mode exists — a supervised child role inside the server process — and it
  must fail loudly. An embedded runner that cannot enroll or dies must surface as an
  operator-visible error; it must never leave `tack serve` running while silently having no
  runner, because that is indistinguishable to the user from a scheduler bug.
- `tack.toml` support for the gate is deliberately deferred. The file is `tack-api`'s
  configuration surface and the gate belongs to `tack-cli`; adding a second reader of one
  file is a change this decision does not need.
- Nothing in `docs/contracts/runner-v1/`, the scheduler, fleets, the operator API or the
  frontend changes.
