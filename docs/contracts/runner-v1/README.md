# Tack runner protocol v1 fixtures

These JSON files are the language-neutral authority for `/api/runner/v1`. Rust,
OpenAPI and frontend types consume them; feature-local DTOs are not a second authority.

`protocol.json` fixes compatibility behavior, route authentication and stable error codes.
`limits.json` fixes all payload and timing bounds. `lifecycle-transitions.json` covers every
ordered state pair: each target appears exactly once in either `allow` or `deny` for each
source state. An idempotent replay is not a lifecycle transition and is therefore denied by
the state validator while the mutation endpoint returns the original success.

Canonical success exchanges are represented by paired `*.request.json` and
`*.response.json` files. Authentication credentials are headers and never appear in a JSON
payload, except the one-time enrollment exchange and the one-time credential response.
Every credential-like fixture value begins with `example_` and is intentionally invalid.

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
Unknown opaque model ids and additive object fields round-trip unchanged.

