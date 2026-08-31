# ADR 0059: Single-operator identity posture

- Status: accepted
- Date: 2026-08-31
- Supersedes: nothing. Sits alongside ADR 0050 (`0050-runner-control-plane.md`) and ADR
  0058 (`0058-standalone-single-binary-runner.md`) without changing either.
- Contract: `docs/contracts/runner-v1/` — unaffected. This decision is scoped to the
  operator-facing `/api` surface authenticated by `require_token`, not the separately
  authenticated runner protocol.

## Context

Tack has no identity model. There is no users table, no sessions, and no per-user
permissions anywhere in the schema. `assignee` (migration 015) is a free-text column on
`items` — a label an operator types, not a foreign key to any account. `roles` (see
`Role` in `tack-core::models`) is a per-project name/color/icon attached to items, for
visual grouping; it is not an identity either and was never meant to be one. Every
`/api/*` request that clears `require_token` (`crates/tack-api/src/middleware.rs`) is,
structurally, the same principal — `operator_principal_value` derives a single stable
label (`operator:token:<hash>` or `operator:local`) from the one configured secret,
precisely because there is nothing more specific to derive it from.

Authorization is one shared bearer token, `TACK_API_TOKEN`, compared in constant time.
When it is unset, `require_token` allows every caller through by design — pure-local
mode, where the operator and the only user of the machine are the same person.

That is a defensible design for a single operator. It stops being defensible the moment
a reader assumes it means something else. "Self-hosted" is the word the product uses
publicly, and to most readers of that word, self-hosted implies accounts: something
they could sign into, something that would tell two people apart. Tack cannot do either.
Shipping that ambiguity — implying a capability that does not exist — is worse than
either honest answer would be on its own, because a team that deploys Tack believing it
has per-user access control has deployed something that does not do what they think it
does.

The posture also aged out from under itself. The non-loopback-without-token check
existed as a hard startup error already (`AppConfig::validate_security`, added in
`fix(api): harden trust boundaries`) — this ADR does not introduce that error, it
ratifies a decision that had never been written down and closes the one real gap in it:
no deployment could opt back in, which meant the check, exactly as it stood, made every
container deployment un-startable (see Decision). Separately, ADR 0058 chose the same
error posture for the embedded runner it defines, for a related but distinct reason:
an embedded runner executes arbitrary coding-agent processes, which is a strictly larger
blast radius than an unauthenticated board API. Both decisions land on "startup error,"
and this ADR is what makes that agreement a stated one instead of two independent
authors arriving at the same answer for reasons neither document cross-references.

## Decision

**Tack v1 is single-operator.** One person (or one small team willing to share one
secret) runs one instance and holds the one credential that unlocks it. This is not
staged as step one of a multi-user roadmap with the rest implied — it is the considered
scope for this major version, stated as a limit, not a gap.

**Rejected alternatives, and why now is not the time for any of them:**

- **Full multi-user accounts with per-user permissions.** This is the largest of the
  three rejected options and the one a reader most likely assumes already exists. It
  would touch nearly every table in the schema — `assignee` would need to become a real
  foreign key, every board/item/comment/attachment read would need a permission check,
  and the audit trail (`x-tack-principal`, currently a hash of the one shared token)
  would need to carry a real actor identity throughout the execution domain, not a
  stand-in for it. That is a schema-wide change justified by team-collaboration demand
  Tack does not have evidence of yet. Building it speculatively, ahead of a real user
  asking for it, is exactly the failure mode `.claude/scope-discipline.md` names:
  a well-built mechanism with no caller.
- **OIDC/SSO.** Solves a problem — federating identity into an existing organizational
  directory — that presupposes the first problem (accounts) is already solved. It is a
  second, larger dependency (an identity provider integration, token refresh, group/role
  mapping) on top of a foundation that does not exist yet. Out of order and out of
  scope for the same reason as full accounts.
- **Per-user API tokens.** The lightest of the three, and the most likely to be built by
  mistake as a "small" version of accounts — issue N tokens instead of 1, keep
  everything else the same. It was rejected on its own merits, not just as a smaller
  version of the option above: without an accounts table there is nothing for a
  per-user token to identify. It would produce N distinguishable secrets that all still
  resolve to the same undifferentiated principal everywhere the principal is actually
  used (idempotency scope, audit `actor` fields), which is a real cost — key rotation,
  storage, a revocation UI — for authentication theater rather than real per-user
  authorization. A shared token that is honest about being shared is more useful than
  distinct tokens that imply a distinction the rest of the system cannot honor.

**The operational rule that follows from single-operator scope:** a bind reachable
beyond the local machine with no credential configured is not "pure-local mode reached
by a wider network" — it is every reader of that network holding, unknowingly, the same
authority as the operator. There is exactly one principal in this design, and leaving
the bind open hands that principal to anyone who can open a TCP connection. Loopback
with no token remains unchanged: on a single machine, whoever can reach `127.0.0.1` on
that machine already has an equivalent or greater level of access to it.

## Safety posture

- **Loopback with no token: unchanged.** This is pure-local mode and remains the
  zero-configuration default `AppConfig::default()` already is.
- **Non-loopback with no token: a startup error**, not a warning and not a log line —
  `AppConfig::validate_security` refuses to start
  (`crates/tack-api/src/config.rs`). A capability that grants full read/write access to
  the board and the runner-scheduling surface is not something an operator should have to
  notice in a log stream to catch; the process must simply not come up.
- **One explicit, documented opt-out: `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`.**
  The Dockerfile and `docker-compose.yml` in this repository necessarily bind `0.0.0.0`
  inside the container — that bind is often reached only from a network the operator
  already trusts (the container's private bridge network, a host firewall, a reverse
  proxy that is the only inbound path). Turning the check above into a hard error with
  no escape hatch would silently brick every one of those deployments on upgrade, which
  is not an acceptable cost of closing a documentation gap. The opt-out is off by
  default, named for exactly what it accepts, and the refusal's own error message names
  it, so setting it is a legible, deliberate act by whoever configures the deployment —
  never a default the code falls back to on its own.

## Consequences

- The public docs (`docs/book/src/user-guide/administration.md`, `docs/CONFIG.md`,
  `README.md`) now say what Tack's authentication actually is before they describe how
  to configure it, closing the "self-hosted implies accounts" gap this ADR exists to
  close.
- `assignee` remains, explicitly, a label rather than an account — documented as such
  rather than left to be discovered by whoever first goes looking for a users table.
- The single-shared-token design is unaffected in code; this ADR records why it stays
  that way rather than changing anything about `require_token` or
  `operator_principal_value`.
- Deploying Tack for a team that needs to tell its members apart — different
  permissions, a real audit trail of who did what, individually revocable credentials —
  is out of scope for v1. Nothing here forecloses building it later; it is deferred
  until real usage says which of the three rejected shapes above (or some fourth one)
  the demand actually looks like, so it can be designed against real requirements
  instead of guessed at.
- The runner-v1 protocol, its credentials, and everything in
  `docs/contracts/runner-v1/` are unaffected — that boundary was already, correctly,
  never routed through `require_token` (ADR 0050).
