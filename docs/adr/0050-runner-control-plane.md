# ADR 0050: Tack-owned scheduling and runner-owned execution

- Status: accepted
- Date: 2026-08-06
- Baseline: `1d71785` (`chore: preserve unreleased Part II baseline`)
- Contract: `docs/contracts/runner-v1/`

## Context

Tack's first orchestration integration treated Docket as the owner of remote tasks,
runtimes, approvals and provisioning. Codex, Claude Code and OpenCode are coding harnesses,
not remote project-management schedulers. Extending the Docket-shaped `ControlPlane` trait
would force each harness to invent fleet-wide APIs and would leave two systems able to
schedule the same work.

The baseline named above intentionally preserves the unreleased Part II work before the
Phase 50 safety repairs. It includes migrations 032-038, the Docket contract oracles,
capability groundwork and their known Wave 0 defects. Part III branches from that exact
commit; it does not silently absorb, reset or discard the baseline.

## Decision

Tack is the plan of record, durable queue and only scheduler for new execution requests.
It owns items, execution requests, fleet assignment, fenced leases, normalized attempt
history, decisions, artifact metadata and optional workflow status mapping.

`tack-runner` is the process owner. A runner pulls work, prepares one isolated workspace
per attempt, keeps the local pre-spawn journal, launches/cancels/probes the coding harness,
and reports actual execution facts. Vendor credentials remain runner-local wherever
possible. The Tack API never starts a coding harness and never becomes a model proxy.

Docket is optional and may exist only as `legacy-docket`: a compatibility bridge with one
documented owner for scheduling. It must not dual-schedule a Part III execution request.
GitHub Actions remains CI/integration work and is not a coding-harness adapter.

Runners initiate every connection. Tack does not call runner-owned URLs. Runner routes are
under a separately authenticated `/api/runner/v1` router; operator, runner and decision
credentials are not substitutable.

## Delivery and recovery semantics

The system does not claim exactly-once harness execution. It guarantees at most one valid
active lease, monotonically increasing fencing tokens and idempotent persistence. A crash
after process launch may leave the side effect ambiguous. Lease expiry alone therefore
never launches another process: journal/probe evidence must prove the prior process absent,
otherwise the attempt becomes `needs_operator`.

Every runner mutation supplies runner id, attempt id and fencing token. A stale or expired
token returns `stale_lease` and writes nothing. Event rows and their checkpoint commit in
one transaction. Replayed event batches and terminal reports are no-ops. Cancellation is
requested and observed separately; a sent signal is not proof of cancellation.

## Enrollment, revocation and rotation

- An operator issues a short-lived, single-use enrollment token. Only its hash and expiry
  are stored. Redemption is transactional and cannot be replayed.
- Successful enrollment returns a runner credential exactly once. Tack stores only its
  hash. The credential is scoped to one runner and runner-v1 routes.
- Refresh authenticates with the current runner credential and can update non-secret
  identity/capability facts; it never returns the stored credential.
- Rotation returns one replacement credential once and atomically revokes the predecessor.
  Revocation immediately rejects claims, heartbeats and reports, including an otherwise
  valid lease. Revocation does not fabricate a terminal attempt state.
- Logs and audit events contain runner/request/attempt ids and credential fingerprints only.
  They never contain credentials, enrollment tokens, prompts, query strings, full event
  payloads or complete environment values.

## Protocol compatibility

The JSON fixtures in `docs/contracts/runner-v1/` are the language-neutral authority.
Protocol v1 is the integer `1`. During this cycle an API accepts only v1 and answers
`unsupported_protocol` with its supported range otherwise. Additive fields must be
optional and ignored when unknown; meanings, required fields, enum values and limits may
not change in-place. A semantic change requires a new protocol version and fixtures. Phase
57 must prove compatibility with one previous released runner before widening the accepted
range.

## Consequences

- PM item state and execution state remain separate; one item can have many requests and
  attempts.
- Requested and actual harness/provider/model values use distinct fields and types. Model
  ids are opaque and never tiered or parsed.
- Capabilities, not harness-name checks, control scheduling and UI eligibility.
- Usage remains nullable with `measured`, `estimated` or `not_measured` provenance.
- Docket can be absent without disabling the runner path.
- v1 deliberately excludes automatic task decomposition, multi-agent fan-out, a generic
  plugin ABI, a Tack model gateway and GitHub Actions execution.

