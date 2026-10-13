# Tack runner protocol v1 fixtures

These JSON files are the language-neutral authority for `/api/runner/v1`. Rust,
OpenAPI and frontend types consume them; feature-local DTOs are not a second authority.

`protocol.json` fixes compatibility behavior, route authentication and stable error codes. It also
names any additive v1 operation; recovery observation is separately runner-authenticated and has
paired request/response fixtures like the other canonical exchanges.
`limits.json` fixes all payload and timing bounds. `lifecycle-transitions.json` covers every
ordered state pair: each target appears exactly once in either `allow` or `deny` for each
source state. An idempotent replay is not a lifecycle transition and is therefore denied by
the state validator while the mutation endpoint returns the original success.

Canonical success exchanges are represented by paired `*.request.json` and
`*.response.json` files: enrollment, refresh, claim, heartbeat, accept, start, event-batch,
decision create/poll, artifact, completion, cancellation and recovery-observation. Accept and
start are the `lease_owner`-only `leased -> preparing` and `preparing -> running` transitions
`lifecycle-transitions.json` names; heartbeat's `active_attempts[].state` is a runner
self-report for reconciliation only and never itself authorizes a transition. Authentication
credentials are headers and never appear in a JSON payload, except the one-time enrollment
exchange and the one-time credential response. Every credential-like fixture value begins
with `example_` and is intentionally invalid.

`artifact.response.json` records the upload grant (`upload.path`/`upload.method`) as data
rather than a value a client derives. A client follows that grant verbatim instead of
reconstructing an upload path itself — it is the one place a response body carries
information no other fixture fixes.

Stable failures use one envelope:

```json
{
  "error": {
    "code": "stale_lease",
    "message": "The lease is no longer valid",
    "request_id": "req_http_01",
    "retryable": false,
    "details": {}
  }
}
```

The files under `errors/` enumerate every stable v1 error code. Servers may add a
human-readable message or details member, but clients branch only on `code` and
`retryable`. Error bodies never echo credentials, prompt bodies, query strings or complete
environment values.

Fixture timestamps are RFC 3339 UTC. IDs are opaque strings with type-specific prefixes in
examples only; consumers must not parse those prefixes. SHA-256 values are lowercase hex.
A model is one opaque string: a claim carries `requested_model` (or null), a completion
carries `actual_execution.model` (or null) with `model_source` of `reported` (the agent's own
output named it), `requested` (only the request did) or `agent_default` (neither did). The
string is passed to the agent verbatim and round-trips unchanged, as do additive object fields.

Each entry of a capability report's `harnesses` says what that agent offers and whether it
can run now. `requires_model` is true when a request must name a model, with
`requires_model_reason` saying why; absent means false. `models` is `{source, ids, reason}`: `listed` when the
agent itself listed them, `suggested` when it cannot and Tack names a few, `none` with a
`reason` when nothing could be read. `default_model` is present only when the agent names one; absent means it names none.
`sign_in` is `{state, detail, command}` with `state` one of `signed_in`, `signed_out` or
`unknown`, and `command` the one that signs in. `docs` holds the `models` and `sign_in` pages
of the agent's own documentation. A failed listing or sign-in check never fails the probe: it
is reported in these members.
