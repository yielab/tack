# docket wire fixtures

These fixtures back `docket_wire_contract_test.rs`, `docket_adapter_test.rs`,
`docket_tick_contract_test.rs`, and the live captures in `docket_live_test.rs`
(`#[ignore]`d — run deliberately against a real `docket serve` instance).
They are what `crates/tack-orch/src/adapters/docket.rs` is verified against;
this file records what was captured, when, and against which docket build,
so a future adapter change can tell "still matches the vendor" from
"nobody re-checked."

## Verified live against a real docket server

Every route `DocketAdapter` uses was exercised against a real, isolated
`docket serve` instance, not just read from `serve.py`/`core/dispatch.py`
source. First captured against docket `0.2.0b1`; recaptured against
`v0.2.0-beta.2` (both `serve.py` and `core/dispatch.py` grew substantially
between the two releases, so nothing below was assumed to still hold).

- **`POST /tasks/{project}` success — unchanged at `v0.2.0-beta.2`.**
  `{"ok": true, "task": "<id>", "project": "...", "status":
  "pending"|"waiting_approval", "approvalToken"?: "..."}`. The task id is
  under `"task"`, not `"taskId"`; a `require_approval` verdict adds
  `"approvalToken"` alongside `"status": "waiting_approval"`.
- **The `pre_input` guardrail's three outcomes — unchanged at
  `v0.2.0-beta.2`.** `block` returns HTTP 400 with `{"ok": false, "error":
  "task rejected by guardrail policy '<id>' at enqueue: <message>"}`;
  `require_approval` returns HTTP 200 with `status: "waiting_approval"` and
  an `approvalToken`; explicit `trusted: false` genuinely flips a
  `prompt-injection`-id policy from skipped to evaluated, while omitting
  `trusted` reproduces the operator-trust default. The gate evaluates once,
  at enqueue, and is never re-evaluated on a later dispatch of the same
  task.
- **`POST /approvals/{token}` — grant confirmed, deny/409/404 newly
  captured.** Grant resumes a gated task (`waiting_approval` -> `pending`)
  and returns `{"ok": true, "token": "...", "state": "granted"}`. `deny`
  returns `{"ok": true, "token": "...", "state": "denied"}` and
  terminalizes the task immediately with no agent turn spent. Replaying a
  decision against an already-decided token returns `409 {"ok": false,
  "error": "Already granted: <token>"}`; an unknown token 404s with `{"ok":
  false, "error": "Approval not found: <token>"}`.
- **`POST /pods` — confirmed unchanged at `v0.2.0-beta.2`**, against an
  isolated server (`DOCKET_HOME` pointed at a scratch dir). A fresh call
  returns `201 {"ok": true, "project", "blueprint", "members": [{"id",
  "role", "model"}]}`; a second call for the same `project` returns `409
  {"ok": false, "error": "'<project>' already exists"}`; an unknown
  blueprint, a missing `project`, or an invalid `pod` value each return
  `400`; a request with no `Authorization` header returns `401`.
- **`POST /dispatch/{project}` — verified live for the first time.** Runs
  the compiled `DocketAdapter::dispatch` (and `get_run` to observe the
  outcome) against the isolated server: the response returns the id under
  `"run"`, before the pipeline has done anything; a missing body is treated
  as `{}`. Provisioning a pod, enqueuing a task, then dispatching it with
  no provider credential configured anywhere the server could reach
  reproduces the whole path for zero cost — the run starts, and only a
  follow-up `GET /runs/{id}` shows the failure (`costUsd` stays `0.0`
  throughout). An unknown/unprovisioned project does **not** 404 on this
  route: the failure (`no pod found for '<project>'`) only surfaces on the
  async `GET /runs/{id}` path, not synchronously. No verdict of any kind —
  guardrail or otherwise — reaches the caller synchronously on this route;
  ADR 0065 depends on that.
- **`list_tasks`/`traces`** (`GET /tasks/{project}`, `GET
  /traces/{project}?since=`) — verified directly against `serve.py`'s
  `do_GET`, not against docket's own `ROADMAP.md` (which still lists them
  `TODO`, a staleness bug in that project's docs, not this one's). `traces`
  wraps `events` as an array of **raw JSON strings**, not parsed objects —
  each element needs a second JSON decode to reach the real event record.
  `next` is docket's own minted resume cursor in a compound `"<ts>Z:<n>"`
  format; this adapter never parses it, only forwards it.

If a route in this list ever 404s against a real docket instance, that
means the plane is running an older build, not that the endpoint is
hypothetical — `DocketAdapter` still classifies it as `OrchError::NotFound`
so a caller can tell "this plane lacks the capability" from a real outage.
