# docket — the fake the harness is tested against

`crates/tack-runner/tests/fake_docket/mod.rs` is an in-process docket that answers on a
loopback port. It is written key by key from docket's published specs, not captured from a
running docket, and it mirrors:

- `specs/api/operator-api.spec.md` **5.0.1** (2026-10-14), status *planned*: docket beta.6
  (API 5) does not exist yet.
- `specs/data/serve-read-api.spec.md` for `/status.json`, `/traces/<pod>`, `/inbox`,
  `/approvals/<token>` and `/tasks/<id>/answer`.

Nothing in this directory is a capture. When docket 0.2.0-beta.6 exists, replace what the
fake says with captures from it and say so here; until then the fake is the proof, and no
real docket binary is spawned and no model is called by any test.

## What the fake implements

`GET /status.json`, `GET /health`, `GET /pods`, `GET /pods/<p>`, `POST /pods` (`ensure`),
`POST /tasks/<pod>` (idempotency key, `400` and `422` on the body rules below),
`GET /tasks/<pod>/<id>`, `POST /tasks/<pod>/<id>/cancel`, `GET /tasks/<pod>/<id>/diff`,
`GET /traces/<pod>?since=` (the compound `<ts>:<n>` cursor), `GET /inbox`,
`POST /approvals/<token>` and `POST /tasks/<id>/answer`. Bearer authentication answers `401`
first on everything but `/status.json` and `/health`.

## What it leaves out

- `GET /tasks/<pod>` and its `externalRef`/`status` filters, `POST .../retry`,
  `POST /pods/<p>/apply`, `GET /pods/<p>/corrections`, `/metrics`, `/runs`, `GET /approvals`,
  `POST /tasks/<id>/pregrants`, `POST /dispatch/<pod>`, `GET .../evidence`.
- The one-second hold-back of `GET /traces` (the fake's timestamps are all in the past), the
  `413` for bodies over 1 MiB, and `409` for a task that ran in place.
- `restrictions.policies` validation (a non-`block` rule is not refused), `baseRef` failures
  other than an unresolvable ref, and `pipeline` resolution: the fake stores the name and
  never fails a task with `failureKind: "pipeline"` on its own; a test scripts that.
- `byModel` cache counters (the spec's example carries `input` and `output` only).
