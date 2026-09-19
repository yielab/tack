# Plan: Agnostic Control Plane

> **Deliverable location.** This session's harness confines writes to this plan file.
> On approval, implementation item **0.0** is: copy this document verbatim to
> `docs/plans/agnostic-control-plane.md` (the path the brief specifies) and commit it
> as `docs: plan for an agnostic control plane`. No other content changes.

## Context

Tack's fleet control center (`tack-orch`, Phases 33-38, all shipped 2026-08-05) works,
but it is docket-shaped end to end: the `ControlPlane` trait carries `provision_pod`,
`list_tasks` and a `FleetStatus` built from docket's agent roster; the schema stores
`blueprint` and `pipeline_file` as first-class columns; the Fleet page renders a
`<th>Pod health</th>`; and the only value the Kind selector offers is `docket`. The
abstraction was written with a second backend in mind and has never met one.

This plan makes the seam real by writing the second adapter against a provider that
shares none of docket's shape (GitHub Actions), and by giving Tack the one thing a
multi-provider control plane cannot work without: **capability negotiation**, so the UI
disables what a provider cannot do and names why, instead of degrading to the
intersection of all providers or lying.

It also closes the half-built GitHub bridge (import + push-only close) into a real
two-way pipeline, and gives the operator per-item model choice - the thing only a
control plane that owns the work item can offer.

---

## 1. Codebase reading

### 1.1 The trait, method by method

`crates/tack-orch/src/lib.rs:621-679`. Classification is against one question: *can an
adapter for a provider with no pods, no roles, no hops, no approval store and no policy
engine implement this without lying?*

| # | Method | Verdict | Why |
|---|---|---|---|
| 1 | `kind() -> &'static str` | **universal** | An adapter discriminator. Keep. |
| 2 | `health() -> Health` | **contaminated DTO** | The concept is universal; `Health { status: String, gateway: u8 }` (`lib.rs:271-280`) is not - `gateway` is docket's telegram gateway liveness bit. |
| 3 | `status() -> FleetStatus` | **docket-specific** | `FleetStatus` (`lib.rs:321-338`) is `{apiVersion, timestamp, gateway, channels, agents[], totalCostUsd}` and `FleetAgent` (`lib.rs:284-308`) is `{kind, scope, model, registered, bindings[], lastActivity, costUsd, budgetUsd}`. This is docket's `serve.py::build_status()` verbatim. GitHub Actions has no roster, no channels, no gateway. |
| 4 | `metrics() -> Vec<MetricSample>` | **transport-specific** | Presumes the provider exposes Prometheus text exposition. The parser (`adapters/prometheus.rs`, 319 lines) is dependency-free and reusable, but the *method* encodes "this provider has a `/metrics` endpoint". |
| 5 | `list_runs(project) -> Vec<RemoteRun>` | **universal, wrong key** | Runs are universal. Scoping by `project: Option<&str>` is docket's `?project=` filter; `RemoteRun.pids` (`lib.rs:383-384`) and `RemoteRun.variables` are docket internals. |
| 6 | `get_run(run_id) -> RemoteRun` | **universal** | Wants to become `run_status(handle)`. |
| 7 | `list_approvals() -> Vec<RemoteApproval>` | **contaminated** | The concept ("blocked on a human") is universal; `RemoteApproval { token, role, action, context }` (`lib.rs:393-417`) is docket's approval store, and `role` is the leak that reaches the API as `PendingApprovalResponse.agent` (`handlers/orch.rs:2106-2107`) and the UI as `agentLabel()`. |
| 8 | `list_tasks(project) -> Vec<RemoteTask>` | **docket-specific** | docket's per-pod task queue. `RemoteTask` (`lib.rs:426-449`) carries `claim_id`, `approval_token`, `pending_approval_index`. GitHub Actions has no queue entity - a dispatch *is* a run. |
| 9 | `traces(project, since) -> TracesPage` | **conflated** | Keyed on **project**, not run. It is docket's per-project JSONL stream and it carries agent telemetry (`tool_call`, `cost_charged`) *and* lifecycle in one channel. This is exactly the conflation H3 predicts. |
| 10 | `enqueue_task(project, task) -> String` | **the universal verb, docket-shaped** | This is `dispatch`. But `NewRemoteTask { description, priority, trusted }` (`lib.rs:461-473`) is docket's enqueue body, and the returned `String` is so thin that `dispatcher.rs:350-373` has to make a **second call** to `list_tasks` to recover status and approval token. |
| 11 | `dispatch(project, vars) -> String` | **dead** | Returns `OrchError::Disabled` unconditionally (`adapters/docket.rs:32-42`). Never called. It is docket's pipeline trigger, not the universal dispatch verb. |
| 12 | `decide_approval(token, grant) -> ApprovalState` | **universal concept, docket id** | `token` is docket's `apr-<uuid>`, which is *also* Tack's `orch_approvals` primary key *and* the URL path segment of `POST /api/approvals/{token}` (`router.rs:111`). |
| 13 | `provision_pod(params) -> ProvisionedPod` | **100% docket** | `ProvisionPodParams` (`lib.rs:507-523`) is `POST /pods`'s body verbatim: `blueprint`, `pod: Option<"full">`, `verify_cmd`, `path`. Has no analogue anywhere else. |

**Score: 2 of 13 methods survive unchanged. 3 are pure docket. 8 need reshaping.** The
trait is contaminated. `docs/book/src/developer/orchestration.md:115-121` already says it
is "no longer frozen" and that card R1 changed it once - so reshaping is in-policy.

Missing entirely: **`capabilities()`**. What exists instead is prose
(`orchestration.md:662-668`: "Pause/resume has zero HTTP surface, in either direction"),
plus two half-mechanisms - a coarse `EXPECTED_API_VERSION = "2"` major-version compare
(`reconciler.rs:259`, `:599-603`) and a convention that a 404 means "capability absent",
asserted only in *test names* (`docket_adapter_test.rs:464`
`list_tasks_404_maps_to_not_found_capability_absent`, `:484`) and nowhere in code - `grep
NotFound crates/tack-orch/src/reconciler.rs` returns zero hits.

### 1.2 Adapter construction is duplicated four times

Identical `match row.kind.as_str() { "docket" => ..., other => ... }`:

| Site | Behaviour on unknown kind |
|---|---|
| `crates/tack-api/src/orch_store.rs:192-213` (`list_registered`) | `warn!` + `continue` - plane silently skipped |
| `crates/tack-api/src/dispatcher.rs:606-618` (`build_control_plane`) | 500 |
| `crates/tack-api/src/handlers/orch.rs:2247-2259` (`build_control_plane_for_decision`) | 500 |
| `crates/tack-api/src/handlers/provisioning.rs:332-344` (`resolve_control_plane`) | 500 |

Sites 2-4 carry a comment saying the duplication is deliberate and that "if a third
caller ever needs this, that's the point to actually share it"
(`handlers/orch.rs:2225-2231`). There are now four. Default kind is hardcoded twice more:
`crates/tack-db/src/repo/orch.rs:132` and the column default at `migrations.rs:404`.

### 1.3 `tack-core` already has a docket noun (existing layer violation)

`crates/tack-core/src/models.rs:691-700` - `pub enum OrchBlueprint { Software, Research,
Content, Ops, AgenticProduct }`, doc-commented "docket pod blueprint names
(`core/blueprints.py`, verified 2026-08-05)". Used by `TemplateOrchestration`
(`:639-689`, fields `blueprint`, `pipeline_yaml`, `pipeline_file`, `verify_cmd`,
`pod_shape`) which is persisted in `project_templates.orchestration` (migration 030) and
**published in the OpenAPI spec** (`openapi.rs:279`, `:296-298`). This is the ACL leak
the brief predicts: a docket noun in the pure-domain crate *and* in the public contract.
It cannot simply be deleted - it is in users' databases and in a committed contract.

### 1.4 The reconciler

`crates/tack-orch/src/reconciler.rs` (3740 lines) is the biggest consumer.

- `ControlPlaneStore` trait `:678-780` - 10 narrow persistence methods. Provider-neutral
  already, except `list_linked_projects` and `list_trace_cursors`/`set_trace_cursor` are
  keyed on `remote_project: String`.
- `RegisteredPlane { id, control_plane: Arc<dyn ControlPlane> }` `:645-648`.
- `FetchOutcome { health, status, runs, approvals, metrics, traces }` `:537-544` - one
  field per poll step, assembled by `reconcile_once` `:553`.
- `evaluate` `:594` reads **only** `health` and `status`; ingestion failures deliberately
  never affect the health verdict.
- `HealthTracker` `:325-352`, `DEGRADED_AFTER_FAILURES = 3`, `UNREACHABLE_AFTER_FAILURES
  = 10`, `MAX_BACKOFF_SECS = 300` (`:247-259`).
- Persistence phase in `spawn_one` `:1448`: `persist_runs` `:836`, `persist_approvals`
  `:891`, `persist_metrics` `:964`, `persist_events` `:1108`.
- Correlation: `correlate_remote_task` `:790`, `extract_task_id` `:815` (reads
  `RemoteApproval.context.taskId`), `session_id_task_id` `:1082` (parses docket's
  `agent:<project>:<task>` session id), `derive_event_id` `:1055` (UUID v5 over
  `(control_plane_id, remote_project, event)` - the idempotency mechanism).
- `spawn_reconcilers_supervised` `:1824` + `supervisor_loop` `:1776` re-read
  `list_registered()` every `supervisor_scan_secs`.
- Jitter is deterministic (`jittered_secs` `:430`, hashes `(plane_id, tick)`) because
  `rand` is deliberately not a workspace dependency.

**Everything is polled per project, on one interval, by one loop.** There is no per-run
polling and no inbound path.

### 1.5 The public contract

`docs/openapi.json`: OpenAPI 3.1.0, **61 paths, 91 operations, 108 schemas, 21 operations
tagged `orchestration`**. Generated from `#[utoipa::path]` annotations (`openapi.rs:90-344`).
Drift-gated by `crates/tack-api/tests/openapi_contract.rs::openapi_spec_matches_committed_file`
(regenerate with `UPDATE_OPENAPI=1`), enforced in CI by `git diff --exit-code
docs/openapi.json` (`.github/workflows/ci.yml`, job `rust`). The frontend has a matching
gate: `npm run gen:api` + `git diff --exit-code src/shared/api/schema.gen.ts` (job
`frontend`).

Docket vocabulary that is **already published** in that contract: `blueprint`,
`pipeline_file`, `pod_shape`, `verify_cmd`, `policy_id`, `on_waiting_approval`,
`approval_token`, `trusted`, `roster[].role`, `roster[].model`, `gateway`,
`OrchBlueprint`, `TemplateOrchestration`.

Doc drift found while reading: every `#[utoipa::path]` on an orch handler still says
"404, Orchestration disabled (TACK_ORCH_ENABLE unset)" and `openapi.rs:339-341` says
"Every route is disabled - 404", but the code returns **409** with `error.code:
"orchestration_disabled"` since commit `1fd686c` (`handlers/orch.rs:68`, `:83-99`).
`docs/book/src/developer/orchestration.md:560-574` and the user-guide pages are stale the
same way.

### 1.6 Auth, CORS and the inbound gap

- `middleware.rs:14-53` `require_token`. Exemptions are **suffix matches**
  (`path().ends_with("/health" | "/openapi.json" | "/alexa")`) - a real footgun for any
  new route. **No `api_token` configured => every request is allowed** (`:36-39`).
- `TACK_ORCH_APPROVAL_TOKEN` is checked inside the handler (`handlers/orch.rs:2203-2222`),
  header `x-tack-approval-token`, and its default is the **opposite** of `require_token`'s:
  unset => always 403.
- **`x-tack-approval-token` is not in the CORS `allow_headers` list** (`router.rs:145-149`
  lists only `CONTENT_TYPE, AUTHORIZATION, ACCEPT`). A cross-origin browser preflight for
  `POST /api/approvals/{token}` fails today. Pre-existing bug, in scope to fix because
  this plan adds more custom headers.
- `webhook.rs` is **outbound only** - it has `sign` (`:30-38`, HMAC-SHA256 via `hmac
  0.13` + `sha2 0.11` + `hex 0.4`) and **no `verify`**. Nothing in the codebase reads
  `X-Tack-Signature`.
- The **only** inbound-push endpoint in the whole API is `POST /api/alexa`
  (`router.rs:313`), which self-authenticates on skill id + an optional `?token=`. It is
  the sole precedent for an unauthenticated inbound route, and
  `middleware::constant_time_eq` (`:57-68`) is the comparator to reuse.

### 1.7 Schema (31 migrations, `crates/tack-db/src/migrations.rs`)

Applied by `run_all` `:45-51`; `apply_migrations` `:82-109` runs **each statement
individually with no wrapping transaction**, then records the name. A statement that
fails leaves the migration unrecorded and retried on every boot.

Load-bearing shapes for this plan:

- `control_planes` (019, `:395-414`): `base_url TEXT NOT NULL`, one `token`, **no config
  or credentials JSON column**, no `UNIQUE(name)`, no `CHECK(kind IN ...)`. A GitHub
  Actions plane needs `{owner, repo, workflow_file, ref, api_base}` plus *two* secrets
  (API PAT and webhook secret) and has nowhere to put either.
- `orch_links` (020, `:416-436`): **PK is `project_id`** - exactly one link per project,
  forever. Columns `blueprint`, `pipeline_file`, `budget_usd` are docket's.
- `orch_tasks` (021, `:438-462`): **no `control_plane_id`** - plane attribution is only
  transitive via `items -> projects -> orch_links`.
- `orch_runs` (022, `:464-482`): `run_id` is a **global** PK across all providers.
- `orch_events` (023, `:484-502`): `run_id` column exists but is **always NULL in the
  entire codebase** (`orchestration.md:521-526`); `item_id` is the only correlation key.
  No `remote_project` column - which is precisely why budget-pause attribution was
  declared impossible (`handlers/orch.rs:816-829`).
- `orch_approvals` (024, `:504-526`): **`token` is the PK and the URL segment.**
- `orch_trace_cursors` (028, `:625-631`): opaque cursor keyed `(control_plane_id,
  remote_project)`. **The one orch table that already generalizes cleanly.**
- `github_links` (018, `:371-380`): `item_id TEXT PRIMARY KEY, repo TEXT, issue_number
  INTEGER, created_at`. **No reverse index on `(repo, issue_number)`, no host column, no
  node id, no `synced_at`, no origin marker.** Nothing prevents two items pointing at the
  same issue. `repo/github_links.rs` (37 lines) has only `set_link` and `get_link` -
  there is no `get_link_by_issue`.

### 1.8 The GitHub pipeline today

- Import: `handlers/import_github.rs`, `POST /api/projects/{id}/import-github`. Raw
  `reqwest` (no `octocrab`). **Page-number paging, not cursor** (`:136-274`, terminates on
  `len() < 100`) - CLAUDE.md's "cursor pagination" is wrong. Rate limits are *observed*
  (`x-ratelimit-remaining`, `:155-157`) and never acted on: no `Retry-After`, no
  `x-ratelimit-reset`, no backoff. Writes items with `ItemSource::Github` via
  `create_item_with_source` - the trust boundary - then a best-effort `set_github_link`
  whose failure only `warn!`s (`:250-255`).
- Push: `github_sync.rs` (130 lines). `state_change(old_done, new_done)` +
  `push_issue_state`. Triggered from `handlers/items.rs:268` `maybe_sync_github` (`:281`),
  `tokio::spawn`'d and **never awaited**, **zero retry**, **no persisted failure record**
  - unlike the auto-dispatch hook beside it, which does write `auto_dispatch_failed`
  events. Only PATCHes `{"state": ...}`. Fires only on `update_item`.
- Test seam: `AppConfig { github_api_base: mock.uri(), .. }` + `test_app_with_config`.
  `crates/tack-api/tests/api_test.rs:1616` `github_import_links_items_then_completion_pushes_close`
  is the end-to-end one; it polls `gh.received_requests()` 40x50ms because the push is
  fire-and-forget.
- `docs/GITHUB-SYNC.md` explicitly lists inbound sync, comment/label mirroring,
  per-project tokens and manual item linking as out of scope for v1.
  `docs/book/src/roadmap.md:386` already carries the spec as **Phase 21 Task 4**.

### 1.9 Frontend coupling

Eight hand-written wire-boundary files (`features/fleet/api.ts`, `features/approvals/api.ts`,
`features/economics/api.ts`, `features/provisioning/api.ts`,
`features/settings/orchestration/api.ts`,
`features/settings/orchestrationSettings/api.ts`, `shared/agentActivity/api.ts`,
`shared/dispatch/api.ts`), each carrying the header contract *"When the real endpoint
lands (or changes), reconciling means editing THIS FILE ONLY."* **No component reads a raw
wire field.** That is the single biggest lever available - the refactor seam is already
built.

Docket vocabulary that is user-visible today (not comments):

| Where | String |
|---|---|
| `features/fleet/FleetPage.tsx:9` | `<th>` column literal `'Pod health'`; also `'Roster'`, `'Gateway'`, `'Burn vs budget'` |
| `features/fleet/FleetRow.tsx:73` | renders `control_plane_kind` raw - the word `docket` per row |
| `features/fleet/FleetRow.tsx:111` | `{agent.role} and {agent.model} joined by a middot` |
| `features/settings/orchestrationSettings/ControlPlanesManager.tsx:254` | `<option value="docket">docket</option>` - **the only Kind option**; defaults at `:44`, `:122`; `<Badge>{cp.kind}</Badge>` at `:174` |
| `features/provisioning/ProvisioningWizard.tsx:305` | `label="Docket project name"` |
| `ProvisioningWizard.tsx:330` | `"Full roster (lead, implementer, reviewer, tester)"` |
| `ProvisioningWizard.tsx:346` | `"Run after each Implementer hop; a non-zero exit blocks completion."` - the only user-visible "hop" |
| `ProvisioningWizard.tsx:386` | `"This will create a real docket pod named ..."` |
| `features/approvals/ApprovalsPage.tsx:172` | toast: `` `... - docket reports "${res.state}".` `` |
| `features/settings/orchestration/format.ts:80-83` | `BUDGET_PAUSE_NOTE`, names `docket profile <pod-id> --resume` |
| `features/settings/orchestration/PolicyPanel.tsx:185-186` | `"run \`docket audit verify\` from the docket CLI"` |

19 user-visible `docket` strings in total. `worktree` and `lane` appear **zero** times -
the ACL has held for those.

`frontend/src/architecture.test.ts` enforces exactly one rule: **no `features/*` file
imports from another `features/*`** (`:36-58`). It does **not** lint vocabulary. Its
practical consequence: `ControlPlaneHealth` is duplicated 4x, `relativeTime`/`elapsedSince`
5x, `HEALTH_LABEL`/`HEALTH_TONE` 3x - any shared abstraction must land in `shared/`.

Only one polling loop exists in the whole app: `ApprovalsPage.tsx:137`, 10 s.

### 1.10 Test coverage of `tack-orch`, stated honestly

**Adapter coverage is good per method and absent as a whole.**

- `crates/tack-orch/tests/docket_adapter_test.rs` - 37 `#[tokio::test]`s against
  `wiremock`, backed by 16 fixtures in `tests/fixtures/` including deliberately malformed
  ones (`status_malformed.json`, `metrics_malformed.txt`, `run_unknown_state.json`,
  `unauthorized.json`).
- **Only four tests assert what goes *out* on the wire**:
  `enqueue_task_sends_the_trusted_flag_on_the_wire` (`:346`),
  `decide_approval_grant_sends_channel_tack_and_returns_the_resulting_state` (`:694`),
  `provision_pod_sends_the_full_request_shape_on_the_wire` (`:982`),
  `unauthenticated_routes_never_send_authorization_header` (`:643`). The other 33 assert
  decoding, not encoding.
- `tests/ingestion_test.rs` (576) and `tests/traces_ingestion_test.rs` (623) drive a real
  `DocketAdapter` against wiremock into an in-memory SQLite, asserting idempotency by
  direct `SELECT COUNT(*)`.
- `reconciler.rs` has ~40 in-file unit tests over a hand-written fake `ControlPlane`
  (`:2144-2290`) - state machine, backoff, jitter, `evaluate`.
- Downstream, `crates/tack-api/tests/` has 14 orch files (~4500 lines).
- `crates/tack-orch` has **no coverage floor of its own** in CI: `ci.yml`'s `coverage` job
  floors `tack-core >= 85`, `tack-db >= 70`, `tack-api >= 70` and does not name `tack-orch`.

**Verdict: a refactor of the trait is *blind* to nine of the thirteen methods' request
shapes.** A rewrite could change what docket receives on nine methods and every test would
still pass. This is the single most important finding in section 1, and it is why Phase 0
of this plan writes the oracle before touching anything.

### 1.11 Verified external facts (GitHub, Claude Code)

Checked against the vendor docs during planning, not assumed:

| Fact | Value | Consequence |
|---|---|---|
| `POST /repos/{o}/{r}/actions/workflows/{id}/dispatches` on github.com | **200** with `{workflow_run_id, run_url, html_url}` | usable as a fast path |
| Same endpoint on **GitHub Enterprise Server 3.17** | **204 No Content, empty body** | **correlation must not depend on it** |
| Cancel a run | `POST .../runs/{id}/cancel` -> 202 (or 409) | `cancel` is supported |
| Pause / suspend / hold a run | **no endpoint exists** | `capabilities().pause = Unsupported` |
| Force cancel | `POST .../runs/{id}/force-cancel` -> 202 | provider extra, not on the trait |
| Re-run | `POST .../runs/{id}/rerun` -> 201 | maps to Tack's `attempt` |
| Run logs | `GET .../runs/{id}/logs` -> **302, link expires in 1 minute** | not an event stream |
| Log/artifact retention | default 90 days; public 1-90, private 1-400 | T7 |
| Job limits | GitHub-hosted **6 h**; self-hosted **5 days**; workflow run 35 days incl. waiting | T9 ceiling |
| Pending deployments | `GET`/`POST .../runs/{id}/pending_deployments`, body `{environment_ids, state: approved\|rejected, comment}` | GHA's decision store |
| Run `status` values | `queued, in_progress, completed, waiting, pending, requested, action_required` + conclusions | maps to `RunState` |
| Webhook headers | `X-GitHub-Event`, `X-GitHub-Delivery` (GUID), `X-Hub-Signature-256` | dedupe key + HMAC |
| Claude Code hooks | `PreToolUse`/`PostToolUse` are **synchronous and block the tool call**; default `command` hook timeout **600 s**, per-hook `timeout` field; exit 2 blocks; JSON `hookSpecificOutput.permissionDecision = allow\|deny\|ask` | T9 ceiling and the HITL mechanism |

---

## 2. Interpretation

**What I understand the goal to be.** Turn a working docket dashboard into a control
plane that is honest about what each provider can do, prove it with a provider that is
maximally unlike docket, close the GitHub loop in both directions, and let the operator
choose the model per work item from the UI - without Tack ever running an agent, proxying
model traffic, or turning on by default.

Four things follow that I want to state plainly because they shape every phase:

1. **Capability negotiation is the product, not plumbing.** Today "docket has no pause"
   is a sentence in a README and a note in a UI panel
   (`features/settings/orchestration/format.ts:80-83`). It must become
   `capabilities().pause == Unsupported`, and every disabled control must name its reason
   from that value, never from a hard-coded `kind === 'docket'` check.
2. **The refactor is currently unsafe.** Nine of thirteen trait methods have no test that
   asserts what leaves the process. Phase 0 fixes that before any other phase runs.
3. **Two channels, only one on the trait.** Lifecycle is *pulled* from a provider API.
   Agent telemetry can only be *pushed* by instrumentation the target repo owns. No
   provider API reports tool-level detail. Modelling both on the trait would force one of
   them to lie.
4. **Model identifiers are opaque strings, forever.** Tack stores the identifier plus the
   id of the gateway that understands it, and never parses, maps, normalises or
   classifies it. No tier abstraction under any name.

**Explicitly out of scope** (from the brief, and I am not smuggling any of it in): per-user
identity and permissions; multi-tenancy; live multi-writer sync; native mobile; binary
signing; a harness abstraction of any kind (the harness is one config string on the
project, beside the workflow name); Tack becoming a gateway; a third adapter; changing the
cost model to observed dollars.

Two things I am also deliberately **not** doing, though they are tempting while in the
files: deleting `provision_pod`'s provisioning wizard (it works and users have it), and
consolidating the 4x/5x duplicated frontend helpers beyond what a phase actually needs.

---

## 3. Hypothesis verdicts

### H1 - the second adapter should be GitHub Actions. **ACCEPTED.**

Evidence from the tree: the three trait methods with **no** GitHub Actions analogue
whatsoever (`status() -> FleetStatus`, `list_tasks()`, `provision_pod()`) are exactly the
three the classification in 1.1 marks as pure docket. A second *agent framework* would
almost certainly have a roster, a queue and a provisioning call, and would let all three
survive by coincidence - proving nothing. GitHub Actions has none of them, no approval
store (only environment protection rules, which are shaped completely differently), no
policy engine, no live event stream, and ephemeral runners. It is the only cheap thing
that can falsify the trait.

Two secondary reasons: Tack's own repo already runs four Actions workflows
(`.github/workflows/{ci,release,scheduled-audit,dependabot-auto-merge}.yml`), so
dogfooding costs nothing; and the GitHub HTTP surface is already a solved problem in this
codebase (`import_github.rs`, `github_sync.rs`, `TACK_GITHUB_API_BASE` for mocking) so the
adapter adds no new dependency.

### H2 - capability negotiation ships before any new adapter. **ACCEPTED.**

The failure mode is already written down as the recommended practice:
`docs/book/src/developer/orchestration.md:650` tells the next adapter author to return
"`OrchError::Disabled` for any write method you don't support yet". That is
`unimplemented!()` through the trait, surfaced to the user as a runtime error on a button
that should never have been enabled.

The UI already needs this and solves it ad hoc, twice:
- `PendingApprovalListResponse.grant_available: bool` (`handlers/orch.rs:2136-2152`) - a
  hand-rolled, single-purpose capability bit.
- `DispatchCardMenu`'s `available: boolean` prop (`shared/dispatch/DispatchCardMenu.tsx:9-14`)
  is fed from `useAgentActivityMap.ts:80` `orchAvailable()`, which is really "a bulk
  agent-activity fetch did not error" - that is *"orchestration is on"*, not *"this
  provider can dispatch"*. With two providers it is wrong.

### H3 - run lifecycle and agent telemetry are two independent channels. **ACCEPTED, with a sharpening.**

The current trait has conflated them, and the schema proves it:
- `traces(project, since)` (`lib.rs:635`) is keyed on **project**, not run.
- `orch_events.run_id` exists in the DDL (`migrations.rs:490`) and is **NULL everywhere
  in the codebase** - confirmed by `docs/book/src/developer/orchestration.md:521-526`:
  *"`orch_events.item_id` is the only reliable correlation key today; anything that needs
  per-attempt (not per-item) correlation can't get it from this table yet."*
- `orch_events` has no `remote_project` column either, which is the documented reason
  budget-pause attribution was declared impossible (`handlers/orch.rs:816-829`).

So today Tack cannot attribute an event to a run at all. docket supplies both channels
from one API, which is exactly why the conflation went unnoticed.

**The sharpening:** the trait should model only **one** of them. `events(handle, cursor)`
is "whatever ordered record stream the *provider* serves". The pushed telemetry channel
does not belong on `ControlPlane` because **no provider serves it** - it arrives at Tack
from inside the run. Putting it on the trait would force every adapter to implement a
method whose data never comes from the provider. They meet again only at the storage
layer, in `orch_events`, distinguished by a `source` column.

### H4 - the unified decision inbox should absorb the fleet view. **SPLIT: the four decision kinds are ACCEPTED; the absorption is REJECTED.**

Accepted half: the inbox should cover four kinds, not one - (a) approval of an
irreversible action, (b) a plan awaiting review, (c) an open question from the agent, (d)
an ambiguity in the work order. Evidence that this is already the real shape: docket's own
`RemoteApproval.action` is free text (`lib.rs:400-402`) and the UI renders it verbatim with
a `'(no action description)'` fallback (`approvals/format.ts:46`) - the kind is currently
carried implicitly in prose. GitHub Actions supplies (a) natively via
`pending_deployments`, and a `PreToolUse` hook supplies (a), (c) and (d) with no provider
support at all. The inbox is also already the most active surface in the app: it is the
**only** polling loop in the entire frontend (`ApprovalsPage.tsx:137`, 10 s), while the
Fleet page does not poll at all (`FleetPage.tsx:106`, a bare `createResource`).

Rejected half: absorbing the fleet view. They answer different questions -
"what needs me right now" versus "is the plant running". The health state machine
(3 failures -> degraded, 10 -> unreachable, `reconciler.rs:247-259`) is **per plane**, has
no decision attached, and is what tells an operator that an empty inbox means "nothing to
do" rather than "the poller is dead". Folding it into the inbox would make a silent outage
indistinguishable from a quiet day. What the fleet view *should* do is generalize: it stops
being a pod roster and becomes runtimes + runs + health, with provider specifics in a
per-adapter fragment. The inbox becomes the primary nav destination; the fleet view stays.

### H5 - usage belongs to a gateway plane, not to the adapter. **PARTIALLY ACCEPTED - and the premise is worse than the hypothesis assumes.**

The finding that changes this: **`orch_tasks.tokens_in` / `tokens_out` are written as
literal `0` by `crates/tack-api/src/dispatcher.rs:382` and are never updated by anything.**
`rg -l 'tokens_in' crates/` shows the only writer is the dispatcher; `reconciler.rs` never
touches them. Every token figure in the Fleet view (`FleetRow.tsx:162`), the Budget panel,
and the whole Economics page (`repo/economics.rs:146`, `COALESCE(SUM(t.tokens_in), 0)`)
therefore reads a structural zero today. There is no ingestion path for **any** provider,
docket included. `pricing_snapshot_at` is likewise always `null`
(`user-guide/orchestration.md:502-518` lists both as known placeholders).

So the real choice is not "adapter vs gateway" - it is "build the first measurement path
at all, and where". Verdict:

- **`usage(handle)` stays on `ControlPlane`, capability-gated** by
  `UsageSupport { NotMeasured, FromProvider, FromGateway }`. Removing it would foreclose
  docket, which genuinely emits `cost_usd`/token data on its trace events
  (`RemoteEvent.cost_usd_estimated`, `lib.rs:595-598`) and just has no roll-up yet.
- **The measurement floor is the pushed telemetry channel, not the gateway.** Every
  OpenAI-compatible response carries a `usage` object, so the `PostToolUse` hook can push
  token counts with the correlation id. That works against a bare vendor key with no
  gateway at all, which is the common case for a solo operator.
- **The gateway is an optional, per-project, higher-quality source**, preferred where it
  exists (aggregate spend by key or tag). Tack stores the base URL and an optional
  **read-only spend-query credential used server-side**; it never puts a model-traffic key
  on the request path and never sends one into a run (see T5 - a key must never travel as
  a workflow input; the run gets its key from a GitHub Actions secret the operator sets).
- Where none of the three exists: **"not measured"**, never zero. The frontend already has
  the right discipline for this (`fleet/format.test.ts` asserts `formatEstimatedCost` never
  says "spend"; `FleetRow.test.tsx` asserts a stale row never renders `$0.00` or `0 tokens`)
  - it just needs a `not measured` state distinct from `stale`.

### Objective 2 - the three concrete answers

**Where does it run?** **GitHub-hosted runners**, first implementation.

| Option | Verdict |
|---|---|
| GitHub-hosted | **Chosen.** Zero infrastructure, honours "one binary, no runtime dependencies", and the adapter's job is to falsify the trait, not to reach local resources. |
| Self-hosted | Not first, but **costs Tack zero code**: `runs-on` lives in the target repo's workflow file, which Tack neither writes nor reads. It is the only option that reaches a local GPU, database or `.env`. Switching is a change in the customer's repo. |
| "Under docket" | **Rejected.** It would make adapter 2 a docket variant and prove nothing about the trait. |

Verification that the choice is real and not assumed:
`gh api repos/{owner}/{repo}/actions/runners --jq '.total_count'` returns the self-hosted
runner count for a repo; `gh api repos/{owner}/{repo}/actions/runs/{id}/jobs --jq
'.jobs[].runner_group_name'` shows which pool actually served a run. Neither value appears
anywhere in Tack's design - that is the point.

**How is it controlled?** Verified against the GitHub REST reference during planning:

- **Cancel: yes.** `POST /repos/{o}/{r}/actions/runs/{run_id}/cancel` -> `202` (or `409`).
  There is also `force-cancel` -> `202`, which the adapter exposes as provider metadata,
  not as a trait method.
- **Pause: no. There is no endpoint at all.** Not a gap to work around - it is
  `capabilities().pause = Support::Unsupported`, and the UI must disable the control and
  name the reason. This is the same shape as docket's already-documented gap
  (`orchestration.md:662-668`), which is the strongest evidence that a capability field is
  the right home for both.
- Re-run exists (`POST .../runs/{id}/rerun` -> `201`) and maps onto Tack's existing
  `orch_tasks.attempt` counter rather than onto a new trait method.

**How does progress reach the panel?** Two channels, exactly as the brief frames them:

| Channel | Source | Granularity | Availability | Trait? |
|---|---|---|---|---|
| Run lifecycle | pulled: `GET .../actions/runs/{id}` and `.../jobs`; later pushed by the `workflow_run`/`workflow_job` webhooks | queued / in_progress / completed + per-job, per-step | always | **yes** - `run_status()` and `events()` |
| Agent telemetry | **pushed from inside the run** by a `PostToolUse` hook | tool calls, files touched, tokens, decisions | **only if the repo is instrumented** | **no** - arrives at Tack's ingest endpoint |

No provider API reports tool-level detail, because the provider is not the agent. GitHub
knows "job 2 of 3, in progress" and nothing more. Run logs are not a substitute: `GET
.../runs/{id}/logs` is a `302` to an archive URL that **expires in 1 minute**, the archive
itself is deleted on the retention schedule (default 90 days), and it is a zip of text, not
an ordered event stream.

This is what forces the **inbound ingest endpoint Tack does not have today**. It is
classified type C and proposed, not assumed - see 8.3.

---

## 3b. Decisions taken (asked and answered before planning the phases)

| # | Decision | Chosen |
|---|---|---|
| D1 | Trait reshape | **One breaking reshape.** The trait has no consumers outside this workspace; `orchestration.md:115-121` already declares it unfrozen. |
| D2 | Agent telemetry ingest | **Add an inbound endpoint**, gated by a third shared secret distinct from `TACK_API_TOKEN` and `TACK_ORCH_APPROVAL_TOKEN`. D5 then refined that secret's role: it is `TACK_ORCH_RUN_BOOTSTRAP_TOKEN`, whose only power is to exchange a valid unbound nonce for a per-run credential - never to write events directly. |
| D3 | Published contract | **Break with notice.** The 21 orchestration operations are reshaped in place; `docs/openapi.json` and `schema.gen.ts` regenerate; `CHANGELOG.md` carries a breaking-change section. No permanent aliases. |
| D4 | Concurrency control | **`version INTEGER` column + `ETag`/`If-Match`,** 412 on mismatch, absent header preserves today's behaviour. |
| D5 | Run credential | **One per-run scoped credential**, minted at bind time, expiring with the run. It can append events and raise decisions **for that run only** - it can never resolve a decision and never edit cards. |
| D6 | Non-additive migrations | **Rebuild `orch_runs` and `orch_approvals`** via SQLite's documented 12-step procedure, one table per migration name. |
| D7 | Plane-wide metrics | **Keep a plane-scoped `plane_metrics()` on the trait**, capability-gated. `/api/metrics` and `/api/projects/{id}/orch-policy` keep working for docket and return an explicit "this provider reports no plane metrics" shape for others. |

### The target trait (D1), corrected by the stress test

Sixteen methods, every one capability-gated and honest. Four corrections against my first draft,
each forced by real code:

```rust
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    /// The ONLY input to the reachability verdict. The adapter decides what
    /// "reachable" means and owns its own expected-version check, so docket
    /// keeps requiring both /health and /status.json while a GitHub plane
    /// does not go unreachable for lacking a runner-admin scope.
    async fn health(&self) -> Result<PlaneHealth, OrchError>;

    async fn runtimes(&self) -> Result<Vec<Runtime>, OrchError>;
    async fn plane_metrics(&self) -> Result<Vec<MetricSample>, OrchError>;

    /// Returns a rich ack, so no adapter ever needs a read-back call to
    /// recover the state it just created.
    async fn dispatch(&self, t: &DispatchTarget, r: DispatchRequest)
        -> Result<DispatchAck, OrchError>;

    async fn list_runs(&self, t: &DispatchTarget) -> Result<Vec<RunStatus>, OrchError>;
    async fn run_status(&self, h: &RunHandle) -> Result<RunStatus, OrchError>;

    /// SCOPED. docket serves events per project and cannot serve them per
    /// run; GitHub Actions can only serve them per run.
    async fn events(&self, scope: &EventScope, cursor: Option<&str>)
        -> Result<EventPage, OrchError>;

    /// First-class, so the reconciler never reaches into provider_metadata
    /// to correlate a remote record back to a Tack item.
    fn correlation_keys(&self, record: &CorrelatableRecord) -> Vec<String>;

    async fn artifacts(&self, h: &RunHandle) -> Result<Vec<Artifact>, OrchError>;
    async fn pending_decisions(&self) -> Result<Vec<Decision>, OrchError>;
    async fn resolve_decision(&self, id: &str, a: DecisionAnswer)
        -> Result<DecisionState, OrchError>;
    async fn usage(&self, h: &RunHandle) -> Result<Option<Usage>, OrchError>;
    async fn cancel(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn pause(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn resume(&self, h: &RunHandle) -> Result<(), OrchError>;
}
```

**Correction 1 - `events` is scoped, not per-run.** `RemoteEvent` (`lib.rs:585`) carries no
run id and `persist_events` says so at `reconciler.rs:1180-1185` (*"docket's trace payload
carries no run_id, only session_id... Left unset rather than guessing"*). A per-run
`events()` would be unimplementable for docket - every event currently ingested would be
dropped. `EventScope::{Run, Project, Plane}` plus `capabilities().event_scope` declares
which one an adapter serves; the cursor store keeps its
`(control_plane_id, remote_project)` key for `Project` scope (the key that
`migrations.rs:619-624` argues for explicitly) and gains a run-scoped key for `Run`.

**Correction 2 - `dispatch` returns `DispatchAck`, not a bare id.** Today
`dispatcher.rs:350-373` makes a **second** call to `list_tasks` after `enqueue_task`
purely to recover `remote_status` and `approval_token`. Deleting `list_tasks` without
widening the ack would make `DispatchedTaskResponse.approval_token` permanently `null`
and would send every approval-gated dispatch down the `on_running` branch instead of
`on_waiting_approval` (`dispatcher.rs:399`). **The OpenAPI drift gate cannot catch that** -
the field still exists, only its value dies. This is the plan's canonical
green-CI-broken-product regression, and section 6 names the test that catches it.

**Correction 3 - `RunState` stays a normalized closed enum on the trait, never in
`provider_metadata`.** `orch_store.rs:580-584` is the only place a finishing agent moves a
card, and it matches on three literals. GitHub conclusions have nine values; seven would
fall through `_ => return` with no error, no log and no event, leaving cards permanently in
"In Progress". The normalized set becomes
`Queued | Running | Blocked | Succeeded | Failed | Cancelled | TimedOut | Unknown(String)`,
with an explicit, tested per-adapter mapping table:

| Provider value | Normalized |
|---|---|
| docket `queued/running/succeeded/failed/cancelled` | same five - **docket is byte-identical** |
| GHA status `queued`, `pending`, `requested` | `Queued` |
| GHA status `in_progress` | `Running` |
| GHA status `waiting`, conclusion `action_required` | `Blocked` **and raises a `Decision`** - a deployment gate is a human waiting, not a terminal state |
| GHA conclusion `success` | `Succeeded` |
| GHA conclusion `failure`, `startup_failure` | `Failed` |
| GHA conclusion `cancelled` | `Cancelled` |
| GHA conclusion `timed_out` | `TimedOut` |
| GHA conclusion `skipped`, `neutral`, `stale` | `Cancelled` (documented as "the provider decided not to run it") |

`StatusMap` gains two optional keys, `on_blocked` and `on_timed_out`. **Absent means fall
back to `on_waiting_approval` and `on_failed` respectively**, so every `status_map` already
saved in a user's database behaves exactly as it does today.

**Correction 4 - `plane_metrics()` stays (D7)**, because `/metrics` is plane-wide with no
run or project dimension and `GET /api/projects/{id}/orch-policy` (`handlers/orch.rs:1220`)
is built entirely from it, including a server-computed `denial_rate`. A per-adapter UI
fragment cannot produce a number the server already committed to in the spec.

**`get_run()` is dead too.** Production call sites: zero - only
`docket_adapter_test.rs:178,516,632`. `dispatch()` was not the only dead method.

---

## 4. Phased plan

Eleven phases in dependency order. Every phase ships alone. Every checklist item is one
commit and carries the command that verifies it.

Two conventions throughout:

- **Every migration adds at most one table or one column.** `migrations.rs:82-109` runs
  each statement individually with **no wrapping transaction** and records the migration
  name only after *all* its statements succeed. A five-`ALTER` migration that fails on
  statement three records nothing, re-runs statement one on the next boot, hits SQLite's
  `duplicate column name`, and **the server never boots again** - with no down-migration.
  Every existing `ALTER` migration in that file is deliberately a single statement (029 at
  `:653`, 030 at `:665`). This plan does the same.
- **Verification is a command whose failure mode is named**, not "the tests pass".

### Phase 0 - Build the regression oracle before touching anything

**Goal:** make the docket adapter's *and the reconciler's* observable behaviour a committed
artifact, so the reshape has something to be proved against.

Why first: nine of thirteen trait methods have no test that asserts what leaves the
process (1.10). Today a rewrite could change what docket receives on nine methods and CI
would stay green.

| # | Item | Verification command |
|---|---|---|
| 0.1 | `crates/tack-orch/tests/docket_tick_contract_test.rs` - **the primary oracle**. Drives a full `reconcile_once` + persist phase against a `wiremock` docket *and* an in-memory SQLite (both patterns already exist in `tests/ingestion_test.rs` and `tests/traces_ingestion_test.rs`; `sqlx` is already a dev-dep at `crates/tack-orch/Cargo.toml:42-45`). Snapshots two things to `tests/golden/`: the **ordered** list of HTTP requests the tick issued, and the resulting rows of `orch_runs`, `orch_approvals`, `orch_events`, `orch_metrics`, `orch_trace_cursors`, deterministically sorted. Scenarios: cold start, warm cursor, rewound cursor, a plane with 0 linked projects, a plane with 3. | `cargo test -p tack-orch --test docket_tick_contract_test` then `UPDATE_GOLDEN=1 cargo test -p tack-orch --test docket_tick_contract_test && git diff --exit-code crates/tack-orch/tests/golden/` - the second must exit 0 on an unmodified tree. |
| 0.2 | `crates/tack-orch/tests/docket_wire_contract_test.rs` - the secondary oracle: per-method request transcript (method, path, sorted query, which headers are present, canonicalised body) plus the decoded result, for all 13 current methods. | `cargo test -p tack-orch --test docket_wire_contract_test` |
| 0.3 | Pinned-literal event-id test. `derive_event_id`'s namespace constant (`reconciler.rs:1046`) carries a "must never change once any deployment has ingested a single event" warning, and the existing `derive_event_id_is_deterministic_for_the_same_source_event` (`reconciler.rs:3308`) only proves determinism *within one build*. Add `assert_eq!(derive_event_id(<fixed uuid>, "proj", &<fixed event>).to_string(), "<literal uuid>")`. | `cargo test -p tack-orch derive_event_id_matches_the_pinned_literal` - and confirm it is real by flipping one byte of the namespace constant locally and seeing it fail. |
| 0.4 | CI gates: a `golden drift` step in `ci.yml` job `rust` mirroring the existing OpenAPI gate at `ci.yml:31-35`, plus `cargo llvm-cov -p tack-orch --fail-under-lines 70` in job `coverage` (there is **no** `tack-orch` floor today - the adapter is the least-guarded code in the workspace). | `cargo llvm-cov -p tack-orch --fail-under-lines 70` |

**Why the tick-level oracle is primary and the per-method one is not enough.** Three
refactors would pass a method-level golden and still be wrong:

- *Delegate and re-scope.* Implement the reshaped `events()` as a straight delegation to
  the same `/traces/{project}?since=` request - byte-identical golden entry - then rewrite
  `reconcile_once` to iterate active runs instead of `list_linked_projects`. With three
  linked projects and zero active runs (the steady state, and exactly what a per-method
  fixture set looks like) the tick now issues **zero** trace calls where it issued three.
  Trace ingestion silently stops for every user. **Caught by 0.1's ordered request list.**
- *Drop a guard.* Remove the `occurred_at < retention_cutoff` check
  (`reconciler.rs:1157-1163`) during the rewrite. Not an adapter concern, so the method
  golden is unchanged. A rewound cursor then resurrects rows already rolled into
  `orch_events_daily` and purged, and the next `rollup_and_purge_orch_events`
  (`repo/orch.rs:1493`) counts their cost **a second time**. **Caught by 0.1's row
  snapshot on the rewound-cursor scenario.**
- *Change the id derivation.* Alter `derive_event_id`'s separator, field order
  (`reconciler.rs:1090-1104`) or namespace bytes. Method golden unchanged; every
  previously-ingested event re-inserts under a fresh id on the first poll after upgrade.
  **Caught by 0.3.**

**Ships alone:** yes, trivially - it adds only tests and CI steps.
**If it ships alone and nothing else does:** the repo gains a behavioural spec for
orchestration it does not have today. Pure gain.

### Phase 1 - Capability model, plane configuration, one adapter registry

**Goal:** make "what can this provider do" a typed value that the UI reads, and give a
plane somewhere to keep provider configuration and credentials.

`control_planes.config`/`secrets` are pulled forward into this phase deliberately: the
registry's signature is `build(kind, base_url, config, secrets)`, and without those columns
it would have to be defined twice.

| # | Item | Verification command |
|---|---|---|
| 1.1 | `Capabilities` struct + enums in `tack-orch/src/lib.rs`: `dispatch`, `cancel`, `pause: Support{Unsupported,Advisory,Supported}`, `resume`, `event_scope: EventScope{None,Run,Project,Plane}`, `artifacts`, `decisions: DecisionSupport{None,Poll,Push}`, `usage: UsageSupport{NotMeasured,FromProvider,FromGateway}`, `model_selection: ModelSelection{Unsupported,Advisory,Honoured}`, `runtimes`, `plane_metrics`, `provisioning`. Each carries a `reason: &'static str` for the disabled case. | `cargo test -p tack-orch capabilities` - asserts docket's struct **field by field** against the documented facts, in particular `pause == Unsupported` with a reason naming `docket profile <id> --resume`. |
| 1.2 | Migration 032: `ALTER TABLE control_planes ADD COLUMN config TEXT NOT NULL DEFAULT '{}'`. **One statement.** | `cargo test -p tack-db --test orch_migrations_test` plus a new `run_up_to(&pool, "032_control_plane_config")` + `column_exists` assertion. |
| 1.3 | Migration 033: `ALTER TABLE control_planes ADD COLUMN secrets TEXT`. **One statement.** Same commit adds the `control_planes.secrets` block to `remote_backup.rs::scrub_snapshot_secrets` - that function's own doc at `:257-261` states the rule, and it must run before the trailing `VACUUM` at `:331`. | `cargo test -p tack-api scrub_removes_control_plane_secrets_from_snapshot` - a new test in the shape of the existing `scrub_removes_control_plane_token_from_snapshot` (`remote_backup.rs:1208`), asserting the column is nulled and the row survives. |
| 1.4 | `health = 'unconfigured'` as a new state. A restored backup has `secrets IS NULL`; `orch_store.rs:172-218` currently `continue`s on adapter-construction failure with only a `warn!`, so the plane **silently vanishes from polling** with no user-visible signal. Benign for docket (its token is optional - `docket.rs:293` only adds the header when present), invisible and fatal for any plane whose credentials are required. Widens `ControlPlaneHealth` in `frontend/src/features/fleet/api.ts:41`. | `cargo test -p tack-api unconfigured_plane_reports_unconfigured_not_unknown` + `npm --prefix frontend test -- fleet/format` asserting `HEALTH_LABEL` covers five states. |
| 1.5 | `tack-orch::adapters::registry::build(kind, base_url, config, secrets)`. Replaces all four duplicated `match kind` sites. It must live in `tack-orch` - `crates/tack-orch/Cargo.toml:7-13` forbids depending on `tack-api`. The four call sites keep their **different** failure behaviour (`orch_store.rs` warns and continues; the other three error), so the registry returns a typed error and each caller maps it. | `rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` returns **0**, and `cargo test -p tack-api --test orch_reconciler_wiring_test` passes unchanged (`list_registered_builds_a_live_adapter_for_a_docket_plane`, `list_registered_skips_an_unknown_kind_without_failing`). |
| 1.6 | `adapters::github_actions` **compile-only stub** - `kind()`, `capabilities()` truthfully filled in, every other method `unimplemented!()`. It is never registered. Its only job is to make "both adapters compile against the trait" a Phase 4 gate rather than a Phase 6 discovery. | `cargo build -p tack-orch` and `cargo test -p tack-orch github_actions_capabilities_are_declared` asserting `pause == Unsupported` and `event_scope == Run`. |
| 1.7 | Expose capabilities on `GET /api/control-planes/{id}` and `GET /api/fleet`; regenerate the contract. | `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract && git diff --exit-code docs/openapi.json` fails **before** the commit and passes after; `jq '.capabilities.pause' <<< $(curl -s :3210/api/control-planes/$ID)` returns `"unsupported"`. |
| 1.8 | Frontend `shared/orch/capabilities.ts`; every gated control reads it. Retire `PendingApprovalListResponse.grant_available` and `useAgentActivityMap`'s `orchAvailable()`-as-dispatch-gate in favour of real capability reads. | `rg -n "kind === 'docket'\|grant_available" frontend/src` returns **0**, and `npm --prefix frontend test -- capabilities` asserts a disabled control renders a reason string **sourced from the capability**, not a hard-coded literal. |

**Ships alone:** yes. Capabilities are additive to the wire; the registry is a pure
refactor; the stub is unregistered.
**If it ships alone:** the UI stops guessing. Nothing else changes.

### Phase 2 - Optimistic concurrency, and an audit of who actually writes

**Goal:** make a lost update detectable, and be honest about the writers that never go
through HTTP.

| # | Item | Verification command |
|---|---|---|
| 2.1 | Migration 034: `ALTER TABLE items ADD COLUMN version INTEGER NOT NULL DEFAULT 1`. **One statement.** Repo layer bumps it on every `UPDATE items`. | `cargo test -p tack-db version_increments_on_every_item_update` - asserts the counter moves for `update_item`, `update_item_status_checked`, **and** `check_and_update_parent_status`. |
| 2.2 | Migrations 035 and 036: the same column on `orch_links` and `control_planes`, one statement each. | `cargo test -p tack-db --test orch_migrations_test` |
| 2.3 | `ETag` on `GET`, `If-Match` on `PATCH`/`PUT` for items, orch-links and control-planes. Absent `If-Match` = today's behaviour exactly. Stale `If-Match` = `412`. | **Corrected 2026-08-06 - the criterion originally written here was weak, and Wave B's adversarial pass proved it.** The gate is the *sequential* tests: `cargo test -p tack-api patch_with_a_stale_if_match_is_rejected_with_412_and_the_standard_envelope patch_with_an_if_match_for_a_different_item_is_rejected` - each captures an ETag, lets a write land, then replays the now-stale ETag and requires `412`. No concurrency, no scheduler dependence, fails 100% of the time against an ignored `If-Match`. The two-racer test is a supplementary property check, **not** the gate - see below. |
| 2.4 | CORS: `router.rs:135-150` has **no `expose_headers` call at all**, so a browser can read no non-safelisted response header today. Add `expose_headers([ETAG])`, add `if-match` and **`x-tack-approval-token`** to `AllowHeaders::list`. The approval-token omission is a pre-existing bug: the decide call at `frontend/src/features/approvals/api.ts:65` works only because production is same-origin via `embed-spa`, and is already broken on any `TACK_ALLOWED_ORIGINS` cross-origin path. There is no CORS test in the repo. | `cargo test -p tack-api preflight_allows_if_match_and_approval_token_and_exposes_etag` - the repo's first CORS test, asserting the three header names in an `OPTIONS` response. |
| 2.5 | `crates/tack-cli/src/client.rs`'s `request()` (`:103`) has **no way to set a header**, so every MCP write (`mcp.rs:154` `update_item`, `:179` `move_item`) is unconditionally last-write-wins. Add header support and send `If-Match` from the MCP tools. The agent-versus-human race is exactly the race this phase exists for, and the agent path is the one currently unprotected. Note `mcp.rs:461` asserts `tools.len() == 8` - it must move in step with any tool change. | `cargo test -p tack-cli mcp_update_item_sends_if_match` (wiremock, `.and(header_exists("if-match"))`). |
| 2.6 | **Document the writers that bypass HTTP entirely**, in `docs/book/src/developer/orchestration.md`: `orch_store.rs:610` calls `dispatcher::apply_mapped_status` **from the reconciler** with no request and no `If-Match` - the largest single mutator of `items.status` is outside this control by design; and `items.rs:311-327` `propagate_parent_completion` mutates the **parent** item on a child's PATCH, so a parent's ETag changes with no caller having touched it. Both are correct behaviour; both must be written down so a client does not conclude 412 is a total ordering. | `mdbook build docs/book` and a grep asserting the section exists. |

**Ships alone:** yes.
**If it ships alone:** clients gain a way to detect a lost update. Nothing regresses,
because an absent `If-Match` preserves current behaviour byte for byte.
**Honest limit:** this is optimistic concurrency for HTTP writers only. It does not, and
cannot, order the reconciler's own writes against a human's.

**Correction, 2026-08-06 - this phase's own acceptance criterion failed the acid test.**
The criterion first written for item 2.3 was "two concurrent PATCHes with the same ETag
yield exactly one 200 and one 412". Wave B's adversarial pass short-circuited the `If-Match`
comparison so a stale ETag was accepted and `412` never returned - and that test detected it
only **5 times in 15 runs**. The reason is instructive and worth keeping: the mutation
removes only the client-value comparison and leaves `claim_item_version`'s atomic
`UPDATE ... WHERE version = ?` in place, so with exactly two racers sharing one still-valid
version the lower-level compare-and-swap *coincidentally reproduces* the one-200-one-412
shape the test watches for. The test cannot distinguish "the header is enforced" from
"there is an unconditional atomic claim underneath". The two sequential tests - replay a
stale ETag after a write has landed, and present an ETag belonging to a different item -
failed 5/5 and 5/5.

The general lesson, which applies to every remaining phase: **a test that observes a race
is not a test that the precondition was checked.** When the criterion is "this input is
validated", drive it sequentially and make the staleness deterministic; save concurrency
tests for properties that are genuinely about concurrency.

### Phase 3 - Run identity and decision store rebuilds (D6)

**Goal:** give a run an identity that two providers can share, and let a decision exist
without a control plane.

Why it cannot be an `ALTER`: `orch_runs.run_id` is a **global** primary key
(`migrations.rs:468`) with `control_plane_id` outside the key. Minting a placeholder row on
the Tack correlation id and later "backfilling" the provider's id inserts a **second** row
under a different PK - `ON CONFLICT(run_id)` (`repo/orch.rs:777`) cannot merge two
different primary keys, so you get two rows per run forever. And
`orch_approvals.control_plane_id` is `NOT NULL REFERENCES control_planes(id)`
(`migrations.rs:513`), which a hook-originated decision from a never-dispatched run cannot
satisfy.

| # | Item | Verification command |
|---|---|---|
| 3.1 | Migration 037 - rebuild `orch_runs` **only**, via SQLite's 12-step procedure: `PRAGMA foreign_keys=OFF` -> create `orch_runs_new` with `PRIMARY KEY (control_plane_id, external_run_id, run_attempt)` and a `correlation_id TEXT UNIQUE` -> `INSERT ... SELECT control_plane_id, run_id, 1, NULL, ...` -> `DROP` -> `RENAME` -> recreate `idx_orch_runs_plane_state` -> `PRAGMA foreign_key_check`. | `cargo test -p tack-db orch_runs_rebuild_preserves_every_row_and_passes_foreign_key_check` - seeds N rows at migration 036, runs 037, asserts row count, per-row field equality, `PRAGMA foreign_key_check` empty, and that the old PK's uniqueness is still enforced. |
| 3.2 | Migration 038 - rebuild `orch_approvals` **only**: `control_plane_id` becomes nullable, plus `kind TEXT NOT NULL DEFAULT 'approval'`, `external_id TEXT`, `provider_metadata TEXT NOT NULL DEFAULT '{}'`. `token` stays the primary key and stays the URL segment - renaming a column that is in a user's database is out of scope and buys nothing. | `cargo test -p tack-db orch_approvals_rebuild_preserves_rows_and_allows_a_null_plane` |
| 3.3 | Boot-safety guard: `run_all` refuses to start if a rebuild migration is half-applied (both `orch_runs` and `orch_runs_new` present), with an error naming the backup endpoint, rather than re-running `DROP TABLE`. | `cargo test -p tack-db a_half_applied_rebuild_refuses_to_boot_with_a_named_error` - creates the intermediate state deliberately and asserts the error text. |
| 3.4 | Release note in `CHANGELOG.md`: this upgrade rewrites two tables; take a backup first (`GET /api/backup`). | `rg -n "orch_runs" CHANGELOG.md` |

**Ships alone:** yes - the new columns are unused until Phase 4.
**If it ships alone:** nothing user-visible changes. `external_run_id` equals the old
`run_id` and `run_attempt` is 1 for every existing row, so the reconciler behaves
identically.
**Risk owned here, not elsewhere:** this is the only phase that rewrites existing rows. 3.1
and 3.3 exist because of it.

### Phase 4 - The trait reshape (breaking)

**Goal:** replace the docket-shaped trait with the agnostic one, and prove docket's
behaviour did not move.

| # | Item | Verification command |
|---|---|---|
| 4.1 | New DTOs in `tack-orch/src/lib.rs`: `PlaneHealth`, `Runtime`, `DispatchTarget`, `DispatchRequest`, `DispatchAck`, `RunHandle`, `RunStatus`, `RunState` (the normalized closed enum), `EventScope`, `EventPage`, `RunEvent`, `Artifact`, `Decision`, `DecisionKind`, `DecisionAnswer`, `DecisionState`, `Usage`, `CorrelatableRecord`. Each provider-shaped extra lives in `provider_metadata: serde_json::Value`. `RunState` keeps the `remote_string_enum!` `Unknown(String)` discipline (`lib.rs:133-206`). | `cargo test -p tack-orch run_state_normalization_table` - a table-driven test asserting **every** documented GitHub status and conclusion, and every docket state, maps to the value in section 3b's table. A mapping that silently falls through fails it. |
| 4.2 | The trait itself; `DocketAdapter` rewritten against it; `provision_pod` leaves the trait for a provider-specific route. | `cargo test -p tack-orch --test docket_tick_contract_test && git diff --exit-code crates/tack-orch/tests/golden/` - **the golden must not move.** Plus `cargo build -p tack-orch` proving both adapters (docket real, GitHub stub) compile against the new shape. |
| 4.3 | `reconciler.rs` restructured: `evaluate` consumes **only** `health()`, `FetchOutcome` fields renamed, per-scope event polling, correlation via `correlation_keys()` instead of `RemoteRun.task_ids` / `RemoteApproval.context.taskId`. `EXPECTED_API_VERSION` moves out of `reconciler.rs:259` into the docket adapter; `PlaneHealth` carries `api_version` and `version_ok`. | `cargo test -p tack-orch --test docket_tick_contract_test` (golden unchanged) plus the existing state-machine tests at `reconciler.rs:1862-2107`, updated only where `evaluate`'s inputs are named - **not** where its verdicts are asserted. If a verdict assertion has to change, docket's behaviour moved and the change is wrong. |
| 4.4 | `StatusMap` gains `on_blocked` and `on_timed_out`, both optional, both falling back to the existing keys. | `cargo test -p tack-api a_status_map_saved_before_this_release_behaves_identically` - loads a `status_map` JSON with only the six original keys and asserts every transition matches Phase 3 behaviour. |
| 4.5 | `tack-api`: `orch_store.rs`, `dispatcher.rs`, `handlers/orch.rs`, `handlers/provisioning.rs` onto the new trait. `dispatcher.rs`'s read-back call disappears because `DispatchAck` carries `state` and `pending_decision_id`. | `cargo test -p tack-api --test orch_dispatch_test` - in particular `dispatch_waiting_approval_applies_on_waiting_approval_not_on_running` (`:294`), plus a **new** `dispatch_ack_carries_the_approval_token_without_a_second_call` asserting `approval_token != null` **and** that wiremock saw exactly one docket request. This is the test that catches the canonical green-CI-broken-product regression. |
| 4.6 | `tack-core`: move `OrchBlueprint` and `TemplateOrchestration` (`models.rs:639-700`) out of the pure-domain crate into `tack-api::handlers::provisioning`. `project_templates.orchestration` is already `TEXT`, so `tack-core` keeps only an opaque `serde_json::Value`. No migration. | `rg -n "blueprint\|Blueprint\|\bpod\b\|docket" crates/tack-core/src/` returns **0**. That is the ACL check, as a command. |
| 4.7 | Regenerate the contract and the TS types; write the breaking-change section of `CHANGELOG.md` (D3). | `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`, then `npm --prefix frontend run gen:api`, then `git diff --exit-code docs/openapi.json frontend/src/shared/api/schema.gen.ts` must pass on the committed result. |
| 4.8 | Frontend: neutral fleet/decision/activity shapes, and `shared/orch/providers/docket/` as a **lazy** fragment (`lazy(() => import(...))`, the pattern already used for every route in `app/routes.tsx:7-22`) so the 30 KB gzipped entry-bundle gate at `ci.yml:178-190` is unaffected. Column header `Pod health` becomes `Health`; `Roster` becomes `Runtimes`; the docket-specific cells move into the fragment. | `npm --prefix frontend run build` then the CI bundle-size check; `rg -n "Pod health\|Roster\|Burn vs budget" frontend/src` returns **0**; `npm --prefix frontend test -- fleet` (the existing `FleetPage.test.tsx` asserts all seven headers and must be updated in the same commit). |

**Ships alone:** yes - it is a complete, coherent replacement.
**If it ships alone:** orchestration works exactly as today for docket, with neutral
vocabulary, and clients of the 21 orchestration operations must update (D3).

### Phase 5 - The decision inbox generalized

**Goal:** turn the approvals inbox into a four-kind decision inbox that any provider, and
an uninstrumented hook, can feed.

| # | Item | Verification command |
|---|---|---|
| 5.1 | `DecisionKind { ApprovalOfIrreversibleAction, PlanAwaitingReview, OpenQuestion, WorkOrderAmbiguity }` end to end: trait DTO, `orch_approvals.kind` (already added in 3.2), API, UI. | `cargo test -p tack-api decision_inbox_returns_all_four_kinds_distinctly` |
| 5.2 | Routes move to `/api/decisions` and `/api/decisions/{id}` (D3: broken with notice, no alias). `TACK_ORCH_APPROVAL_TOKEN` and the `x-tack-approval-token` header keep their exact meaning - resolving a decision stays higher-privilege than editing a card. | `cargo test -p tack-api --test orch_approvals_test` retargeted, including `decide_approval_403s_when_no_approval_token_is_configured_even_with_a_header` (`:271`), which must still pass verbatim in behaviour. |
| 5.3 | Frontend inbox renders kind, with a per-kind answer control (grant/deny; approve-plan/request-changes; free-text answer; disambiguation choice). | `npm --prefix frontend test -- approvals` plus an axe scan in `e2e/a11y.spec.ts` for the new controls. |
| 5.4 | Decision provenance: every resolution writes an `orch_events` row naming who resolved it and through which surface. With one shared secret there is no per-user actor (T6) - the row records the surface, and the UI says so rather than implying attribution. | `cargo test -p tack-api resolving_a_decision_records_an_unattributed_audit_row` and `npm --prefix frontend test -- approvals` asserting the UI never renders a user name it does not have. |

**Ships alone:** yes.
**If it ships alone:** the inbox handles more shapes of "blocked on a human" for docket.

### Phase 6 - The GitHub Actions adapter, with its ingest path

**Goal:** the falsification test. A second adapter, end to end, including the only way its
agent telemetry can exist.

The ingest endpoints are **in this phase, not before it.** Their only consumer is this
adapter's handshake; shipping an orch-gated, separately-credentialed write endpoint with
zero callers for two phases is pure attack surface.

| # | Item | Verification command |
|---|---|---|
| 6.1 | `adapters::github_actions` for real: `health` (a cheap authenticated `GET /repos/{o}/{r}`, never the runner list - that needs repo-admin and would pin every plane at `unreachable` through `backoff_secs`), `runtimes`, `list_runs`, `run_status`, `events` (`EventScope::Run`, derived from `GET /runs/{id}/jobs` steps, cursor = highest `(job_id, step_number)`), `artifacts`, `pending_decisions` (from `GET .../pending_deployments`), `resolve_decision` (`POST .../pending_deployments`), `cancel`, `dispatch`. `pause`/`resume` return `OrchError::Unsupported` and `capabilities().pause == Unsupported`. Raw `reqwest` - no `octocrab`, per `crates/tack-orch/Cargo.toml:20-23`. | `cargo test -p tack-orch --test github_actions_adapter_test` - wiremock, with a golden request transcript in the same shape as 0.2, plus fixtures for a 204-on-dispatch (GHES) and a 200-with-`workflow_run_id` (dotcom). |
| 6.2 | Correlation, with a **single-use nonce**. Tack mints `tack_run_id`, passes it as a non-secret `workflow_dispatch` input. Because that input is caller-supplied, anyone with `actions:write` could forge one - so `POST /api/fleet/runs/bind` must verify the nonce was minted **by Tack, for that plane, and is still unbound**, and consume it. It returns the per-run credential (D5). This one call is simultaneously the handshake, the correlation binding, and the credential exchange. | `cargo test -p tack-api bind_rejects_a_forged_nonce_a_reused_nonce_and_a_nonce_from_another_plane` - three cases, three 403s, and an assertion that no `orch_runs` row is created for any of them. |
| 6.3 | `POST /api/fleet/runs/{correlation_id}/events`, authenticated by the run credential only. Idempotent on a caller-supplied `event_id` via a partial unique index. Rejects events for a run in a terminal state. | `cargo test -p tack-api posting_the_same_event_batch_twice_yields_one_row_per_event` - asserts a `SELECT COUNT(*)` unchanged after a replay, in the shape `tests/ingestion_test.rs` already uses. |
| 6.4 | Migration 039 - `ALTER TABLE orch_events ADD COLUMN source TEXT NOT NULL DEFAULT 'poll'`. **One statement.** Plus, in the same commit, the backfill: `orch_events.run_id` is **not** always NULL and `orch_events.id` is **not** always a UUIDv5 - `orch_store.rs:663-681` writes `Uuid::new_v4()` with `run_id: Some(..)` for `status_map_skipped_human_override`, and `dispatcher.rs:~555` and `items.rs`'s auto-dispatch hook write `new_v4()` with `run_id: None`. Three provenances already exist, so the vocabulary is `'poll' \| 'push' \| 'local'` and existing locally-minted rows are backfilled to `'local'` keyed on `event_type`. | `cargo test -p tack-db existing_locally_minted_events_backfill_to_local_not_poll` - seeds one row of each of the three provenances before the migration and asserts the correct `source` after. |
| 6.5 | Migration 040 - `ALTER TABLE orch_events ADD COLUMN external_id TEXT`. **One statement.** Migration 041 - `CREATE UNIQUE INDEX ... ON orch_events(control_plane_id, external_id) WHERE external_id IS NOT NULL`. A partial unique index over a brand-new column cannot fail on existing data. | `cargo test -p tack-db --test orch_migrations_test` |
| 6.6 | Auth wiring. `router.rs:323` layers `require_token` over the whole `/api` router and `orch_routes` adds `require_orch_enabled` (409). The bind and ingest routes must sit **outside** both: a run must not need `TACK_API_TOKEN`, and toggling orchestration off in the UI must not 409 an in-flight handshake and re-label every live run "not instrumented". They get their own sub-router with their own guard - **not** a fourth entry in `middleware.rs:20-34`, whose exemptions are `path().ends_with(...)` suffix matches that would exempt any future path ending in the same string. | `cargo test -p tack-api ingest_routes_need_no_api_token_and_survive_the_orch_toggle` and `cargo test -p tack-api no_other_route_became_exempt` - the second enumerates the router and asserts the exemption set is exactly the four intended paths. |
| 6.7 | Reference workflow + `PostToolUse` hook under `docs/examples/github-actions/`, plus the operator guide. Documents honestly: fork-PR runs receive no secrets, so they cannot bind and will correctly render "not instrumented". | `mdbook build docs/book`; the workflow YAML is linted by `actionlint` if available, otherwise parsed by the repo's existing `serde_yaml`. |
| 6.8 | "Not instrumented" versus "waiting on a human". A bind that never arrives is **not** enough to declare a run uninstrumented: a run parked on a required-reviewer environment sits in `waiting` for up to 35 days and is precisely the case the decision inbox exists to surface. The timer is suppressed whenever the run's own status is `waiting`, or a `Decision` is open against it. Separately, a run that goes `queued -> cancelled` without ever starting never arms an `in_progress`-based timer at all, so the reaper is driven by a **dispatch-time deadline**, not by a state transition. | `cargo test -p tack-api a_waiting_run_is_never_labelled_not_instrumented` and `cargo test -p tack-api a_run_cancelled_before_starting_is_reaped_and_leaves_an_event` - the second asserts the card does not sit in "In Progress" forever and that an `orch_events` row explains why. |
| 6.9 | Frontend: `github-actions` as a Kind option with its own config form (owner/repo, workflow file, ref, API base) and a lazy `shared/orch/providers/github-actions/` fragment. | `npm --prefix frontend test -- orchestrationSettings` asserting two Kind options and that selecting each renders a different config form; `npm --prefix frontend run build` under the bundle gate. |

**Ships alone:** yes.
**If it ships alone:** Tack drives GitHub Actions end to end. This is the phase that either
falsifies the trait or proves it; if a trait change is needed here, it is **evidence**, and
Phase 4's golden re-runs to prove docket still did not move.

### Phase 7 - Model policy owned by Tack

**Goal:** the operator picks the model for a piece of work from the UI, and can see where
the choice came from.

| # | Item | Verification command |
|---|---|---|
| 7.1 | Migration 042 - `CREATE TABLE orch_model_policy (project_id, scope, scope_key, model TEXT, gateway_id TEXT, ...)`, one row per level, `UNIQUE(project_id, scope, scope_key)`. **One table.** `scope` is `item`, `item_type`, or `project`. | `cargo test -p tack-db --test orch_migrations_test` |
| 7.2 | `tack_core::model_policy::resolve(item_override, item_type_default, project_default, plane_default) -> Resolved { model: Option<String>, gateway_id: Option<String>, provenance: Provenance }`. Pure, no I/O, in `tack-core` - and it **never parses, maps, normalises or classifies the identifier**. Tack classifies work items; the gateway classifies models. No tier abstraction under any name (docket removed `economy`/`standard`/`premium` in 0.2.0 and accepts them nowhere). | `cargo test -p tack-core model_policy_resolution_order` covering all sixteen presence combinations, plus `model_policy_never_inspects_the_identifier` asserting a nonsense string like `"zzz/not-a-model:v9"` resolves and round-trips unchanged. |
| 7.3 | API: read and write the policy; every response carries the **resolved** value and its provenance. | `curl -s :3210/api/items/$ID/model-policy \| jq '{model, provenance}'` returns e.g. `{"model":"sonnet","provenance":"project_default"}`. A response that returns a model with no provenance fails the handler test. |
| 7.4 | `capabilities().model_selection` respected in the UI, with all three values live: docket `Unsupported` (it owns its own routing and may ignore an external model), GitHub Actions `Honoured` (forwarded verbatim as a workflow input). The picker is disabled with a named reason where unsupported, and labelled "advisory" where advisory - never a picker that silently does nothing. **This is the capability-negotiation acceptance test for the whole cycle.** | `npm --prefix frontend test -- modelPolicy` - asserts three distinct renderings for the three capability values, and that the `Unsupported` case renders a reason naming the provider's own behaviour. |
| 7.5 | Gateway config per project: base URL, an optional **server-side read-only spend-query credential**, and nothing else. Tack never puts a model-traffic key on the request path and never sends one into a run - the run gets its key from a GitHub Actions secret the operator sets. Any gateway secret stored in `app_meta` must also be added to `SENSITIVE_META_KEYS` (`remote_backup.rs:262`). | `cargo test -p tack-api gateway_secret_is_write_only_and_scrubbed_from_a_backup` |
| 7.6 | Token measurement, at last. `orch_tasks.tokens_in`/`tokens_out` are written as literal `0` by `dispatcher.rs:382` and updated by nothing - every token figure in the Fleet view, the Budget panel and the whole Economics page currently reads a structural zero. Roll up pushed telemetry (`orch_events` with `source='push'`) and docket's own `cost_charged` events into `orch_tasks`. Where no source exists, render **"not measured"**, never `0`. | `cargo test -p tack-api pushed_usage_events_roll_up_into_orch_tasks` and `npm --prefix frontend test -- economics` asserting a project with no measurement source renders "not measured" and **never** `$0.00` or `0 tokens` - the existing `FleetRow.test.tsx` already holds that line for the stale case. |
| 7.7 | `harness` as one plain config field on the project (`orch_links.harness TEXT`, migration 043, one statement), beside the workflow name. No trait, no capability matrix, no plugin layer. | `rg -n "trait Harness\|HarnessRegistry\|harness_capabilities" crates/` returns **0**. |

**Ships alone:** yes - with no policy rows, resolution returns the plane default, which is
today's behaviour.

### Phase 8 - Inbound GitHub webhooks

**Goal:** replace polling latency with push for run lifecycle, and add the inbound half of
the pipeline.

| # | Item | Verification command |
|---|---|---|
| 8.1 | `POST /api/webhooks/github/{control_plane_id}` verifying `X-Hub-Signature-256` with `hmac 0.13` + `sha2 0.11` + `hex 0.4` (already present; `webhook.rs:30-38` is the signing counterpart) and `middleware::constant_time_eq` (`:57-68`). Its own sub-router, same treatment as 6.6. | `cargo test -p tack-api webhook_rejects_a_bad_signature_a_missing_signature_and_a_signature_for_another_plane` - three 401s, zero writes. |
| 8.2 | Delivery dedupe on `X-GitHub-Delivery` (GUID), persisted with the retention sweep. | `cargo test -p tack-api replaying_the_same_delivery_guid_is_a_no_op` - asserts the second delivery changes no row. |
| 8.3 | `workflow_run` and `workflow_job` -> `RunStatus`; `deployment_review` -> `Decision`. Poll remains as the reconciliation backstop, so a missed delivery self-heals. | `cargo test -p tack-api a_missed_delivery_is_recovered_by_the_next_poll` - drops a delivery deliberately and asserts the poll converges to the same row state. |
| 8.4 | Echo suppression (T1): a `ChangeOrigin` tag threaded through the mutation path so a webhook-originated write never re-fires `maybe_sync_github`; plus a `github_links.state_hash` backstop so an inbound event matching what Tack last pushed is a no-op; plus dropping events whose `sender.id` is the identity Tack pushes as. | `cargo test -p tack-api a_webhook_driven_status_change_produces_no_outbound_push` - asserts the mock GitHub received **zero** requests after an inbound close. A naive implementation loops and this test never terminates cleanly. |

**Ships alone:** yes - polling still works if webhooks are not configured.

### Phase 9 - Intervention without pause

**Goal:** fail-closed human-in-the-loop inside a run, on a provider with no pause API.

| # | Item | Verification command |
|---|---|---|
| 9.1 | `POST /api/fleet/runs/{correlation_id}/decisions` (raise, authenticated by the run credential) and `GET /api/fleet/runs/{correlation_id}/decisions/{id}/verdict?wait=<secs>` (bounded long-poll). Resolution stays on `/api/decisions/{id}` behind `TACK_ORCH_APPROVAL_TOKEN` - **the run credential can raise a decision and can never answer one.** | `cargo test -p tack-api a_run_credential_cannot_resolve_its_own_decision` - a 403 with the decision left pending. |
| 9.2 | The ceiling, stated and enforced: the reference hook sets `timeout: 600` explicitly (the Claude Code default for a `command` hook is 600 s, per-hook configurable) and requests `wait=540`, leaving headroom for the round trip. 540 s is far under the 6 h GitHub-hosted job cap and the 5-day self-hosted cap, so the wait can never be what kills a job. **Expiry is fail-closed**: the endpoint returns `deny`, the hook exits 2, the tool call is blocked, and an `orch_events` row records the expiry. | `cargo test -p tack-api an_expired_decision_returns_deny_and_writes_an_audit_row` |
| 9.3 | The item's landing state on expiry: `on_blocked` if set, else the item stays put and the decision is recorded as expired - never silently "done". | `cargo test -p tack-api an_expired_decision_never_moves_an_item_to_a_done_status` |
| 9.4 | Reference `PreToolUse` hook script and the cost note in the operator guide: the wait is **paid idle runner time**, so a handful of runs parked on human decisions can cost more than the work. Recommend raising decisions at genuine gates, not per tool call. | `mdbook build docs/book`; `rg -n "paid idle" docs/book/src/user-guide/` |

**Ships alone:** yes - a repo with no `PreToolUse` hook is unaffected.

### Phase 10 - The GitHub pipeline, both directions

**Goal:** finish the bridge whose first foot shipped as Phase 21 v1.

| # | Item | Verification command |
|---|---|---|
| 10.1 | Migrations 044-048, **one `ALTER` each**: `github_links.host` (default `'github.com'`), `.node_id`, `.last_synced_at`, `.remote_updated_at`, `.state_hash`. Plus migration 049, a **non-unique** index on `(host, repo, issue_number)` - a unique index could fail on a user's existing duplicates, and a failed statement in this migration runner bricks the boot loop. Uniqueness is enforced in the repo layer, which logs when it finds more than one. | `cargo test -p tack-db github_links_reverse_lookup_returns_a_deterministic_row_and_logs_a_duplicate` |
| 10.2 | Credential precedence, decided and written down: `TACK_GITHUB_TOKEN`/`TACK_GITHUB_API_BASE` (`config.rs:66,71`) remain the fallback for import and issue push; a control plane's own `secrets`/`config` win where a plane is involved. Two token sources with different scopes exist today and the rule was never stated. | `cargo test -p tack-api plane_credentials_win_over_the_global_github_token` |
| 10.3 | Inbound `issues` and `issue_comment` -> item create/update, applied **through `tack-core`** so workflow rules hold, with `ItemSource::Github` preserved (`models.rs:129-131`) so the trust boundary is not laundered. | `cargo test -p tack-api an_issue_edited_on_github_updates_the_item_without_bypassing_the_workflow_engine` - asserts an illegal transition is rejected, not forced. |
| 10.4 | Outbound item -> issue create, **per project, opt-in, off by default**. | `cargo test -p tack-api item_create_does_not_touch_github_unless_the_project_opted_in` |
| 10.5 | `pull_request` and `check_suite`/`check_run` -> item state: PR opened moves the item and links the PR; checks running -> verifying; check failed -> failed with the run link; PR merged -> done with evidence (SHA, run URL, artifacts) persisted as an `orch_events` row. | `cargo test -p tack-api a_merged_pr_completes_the_item_with_sha_run_url_and_artifacts` - asserts all three evidence fields are non-empty. |
| 10.6 | Decision mirroring: a blocking decision appears in Tack's inbox **and** as a comment plus a label on the issue; resolving on either side reflects on the other. | `cargo test -p tack-api resolving_on_github_resolves_in_tack_and_vice_versa_without_an_echo` |
| 10.7 | Retry and rate limits for the outbound path. Today `maybe_sync_github` is `tokio::spawn`'d and never awaited, with zero retry and no persisted failure record (`items.rs:281-303`) - unlike the auto-dispatch hook beside it, which writes an `auto_dispatch_failed` event. Add a bounded retry honouring `Retry-After` and `x-ratelimit-reset`, and record failures as `orch_events`. No new dependency: `tower` is already a workspace dep with `features = ["full"]`. | `cargo test -p tack-api a_rate_limited_push_retries_and_records_a_failure_event` |
| 10.8 | Rewrite `docs/GITHUB-SYNC.md` for v2 and update `docs/book/src/roadmap.md`'s Phase 21 entry. | `mdbook build docs/book` |

**Ships alone:** yes - every direction is individually gated.

---

## 5. Change surface

### Touched, per phase

| Phase | Files | Why |
|---|---|---|
| 0 | `crates/tack-orch/tests/**` (new), `.github/workflows/ci.yml` | Tests and gates only. |
| 1 | `tack-orch/src/lib.rs`, `tack-orch/src/adapters/{mod,registry,github_actions}.rs`, `tack-db/src/migrations.rs`, `tack-api/src/{orch_store,dispatcher,remote_backup}.rs`, `tack-api/src/handlers/{orch,provisioning}.rs`, `frontend/src/shared/orch/capabilities.ts` + the eight wire-boundary `api.ts` files | Capability type, plane config/secrets, one registry replacing four `match` sites, backup scrubbing. |
| 2 | `tack-db/src/{migrations,repo/items,repo/orch}.rs`, `tack-api/src/{router,middleware}.rs`, `tack-api/src/handlers/{items,orch,projects}.rs`, `tack-cli/src/{client,mcp}.rs` | `version` columns, ETag/If-Match, CORS, MCP header support. |
| 3 | `tack-db/src/migrations.rs` only | Two table rebuilds and a boot-safety guard. |
| 4 | `tack-orch/src/{lib,reconciler}.rs`, `tack-orch/src/adapters/docket.rs`, `tack-api/src/{orch_store,dispatcher,sprint_dispatch,openapi}.rs`, `tack-api/src/handlers/{orch,provisioning,economics}.rs`, `tack-core/src/models.rs`, `docs/openapi.json`, `frontend/src/{features/fleet,shared/orch,shared/dispatch,shared/agentActivity}/**`, `CHANGELOG.md` | The reshape. Widest single phase by design - the alternative is two coexisting trait surfaces. |
| 5 | `tack-api/src/handlers/orch.rs`, `tack-api/src/router.rs`, `frontend/src/features/approvals/**` | Decision kinds and routes. |
| 6 | `tack-orch/src/adapters/github_actions.rs`, `tack-api/src/handlers/ingest.rs` (new), `tack-api/src/router.rs`, `tack-db/src/migrations.rs`, `docs/examples/github-actions/**` (new), `frontend/src/features/settings/orchestrationSettings/**`, `frontend/src/shared/orch/providers/github-actions/**` (new) | The second adapter and its ingest path. |
| 7 | `tack-core/src/model_policy.rs` (new), `tack-db/src/migrations.rs`, `tack-api/src/handlers/{orch,economics}.rs`, `frontend/src/features/settings/**`, `frontend/src/features/item-detail/**` | Model policy, gateway config, the first real token roll-up. |
| 8 | `tack-api/src/handlers/webhooks.rs` (new), `tack-api/src/{router,github_sync}.rs`, `tack-api/src/handlers/items.rs` | Inbound deliveries and echo suppression. |
| 9 | `tack-api/src/handlers/{ingest,orch}.rs`, `docs/examples/hooks/**` (new) | Raise/await/expire. |
| 10 | `tack-db/src/{migrations,repo/github_links}.rs`, `tack-api/src/{github_sync,webhook}.rs`, `tack-api/src/handlers/{items,webhooks,import_github}.rs`, `docs/GITHUB-SYNC.md` | The two-way pipeline. |

### Deliberately NOT touched

| Not touched | Why |
|---|---|
| `crates/tack-core/src/{workflow,dependency,vocabulary}.rs` | The domain engine is provider-agnostic already and correct. Status changes keep going through `validate_transition`; nothing in this plan writes a status by raw SQL. |
| `crates/tack-db/src/repo/{items,sprints,boards,comments,attachments,custom_fields,roles,templates}.rs` | Only `repo/items.rs`'s version bump is in scope. The rest of the data layer has no orchestration concern. |
| `handlers/{alexa,backup,export,boards_multi,comments,custom_fields,dependencies,roles,sprints,templates}.rs` | Untouched by any objective. `alexa.rs` in particular is left alone despite being the existing inbound precedent - reusing its suffix-match exemption is explicitly rejected in 6.6. |
| `handlers/import_linear.rs` | Linear import is out of scope; it has no `TACK_LINEAR_API_BASE` test override and adding one is a separate concern. |
| The provisioning wizard's user-facing flow | It works and users have it. It moves off the trait to a provider-specific route in 4.2; its UI becomes the docket fragment. This plan does not redesign it. |
| The 4x/5x duplicated frontend helpers (`ControlPlaneHealth`, `relativeTime`, `HEALTH_LABEL`, `formatBudgetCap`) | `architecture.test.ts:36-58` forces the duplication and consolidating it is opportunistic. Only helpers a phase actually needs move to `shared/`. |
| `adapters/prometheus.rs` | 319 lines, dependency-free, and D7 keeps its only consumer. Unchanged. |
| The economics selection-bias and min-sample discipline (`handlers/economics.rs:78-95`) | It is the most carefully argued numeric honesty in the codebase. Phase 7 feeds it real numbers; it does not change how it withholds them. |
| Per-user identity | Out of scope by the brief. See 9.1 and T6. |

---

## 6. How docket is proven not to have regressed

**The specific test: `crates/tack-orch/tests/docket_tick_contract_test.rs`, and it does not
exist today. Phase 0 writes it first.**

Not "we ran the tests" - here is precisely what it catches and why the existing suite does
not. It drives one full reconciler tick (`reconcile_once` plus the whole persist phase)
against a `wiremock` docket and an in-memory SQLite, and snapshots two artifacts to
`crates/tack-orch/tests/golden/`:

1. **The ordered list of HTTP requests the tick issued** - method, path, sorted query,
   which headers were present, canonicalised body. Ordered and counted, not just a set.
2. **The resulting rows** of `orch_runs`, `orch_approvals`, `orch_events`, `orch_metrics`
   and `orch_trace_cursors`, deterministically sorted.

Across five scenarios: cold start, warm cursor, **rewound cursor**, a plane with zero
linked projects, a plane with three.

The rule that makes it an oracle rather than a snapshot: **the golden files must be
byte-identical from Phase 0 through Phase 4.** CI enforces it with the same shape as the
existing OpenAPI gate (`ci.yml:31-35`):

```
UPDATE_GOLDEN=1 cargo test -p tack-orch --test docket_tick_contract_test
git diff --exit-code crates/tack-orch/tests/golden/
```

### Why the obvious alternative is not enough

A per-method wire-contract test (which Phase 0 also writes, item 0.2, as a secondary) can
be defeated by three refactors that a reviewer would plausibly accept:

| Wrong refactor | Method golden | Tick golden |
|---|---|---|
| Implement reshaped `events()` as a straight delegation to the same `/traces/{project}?since=` request, then re-scope `reconcile_once` to iterate active runs instead of `list_linked_projects`. Steady state = 3 linked projects, 0 active runs, so the tick issues **zero** trace calls where it issued three. Trace ingestion silently stops for every user. | identical | **fails** - the ordered request list loses three entries |
| Drop the `occurred_at < retention_cutoff` guard (`reconciler.rs:1157-1163`). A rewound cursor resurrects rows already rolled into `orch_events_daily` and purged; the next `rollup_and_purge_orch_events` counts their cost a second time. | identical | **fails** - the rewound-cursor scenario's row snapshot gains rows |
| Change `derive_event_id`'s separator, field order (`reconciler.rs:1090-1104`) or namespace bytes (`:1046`). Every previously-ingested event re-inserts under a fresh id on the first poll after upgrade. | identical | **fails** - plus item 0.3's pinned literal fails independently |

### The one regression neither golden can see, and the test that does

`DispatchedTaskResponse.approval_token` (`handlers/orch.rs:1650`) is recovered by
`dispatcher.rs:350-373`'s follow-up `list_tasks` call. Reshape the trait without widening
`dispatch()`'s ack and that field becomes permanently `null` - **the field still exists, so
the OpenAPI drift gate stays green**, and an approval-gated dispatch quietly takes the
`on_running` branch (`dispatcher.rs:399`) and moves the card to the wrong column.

That is caught only at the API layer, by a test named in item 4.5:

```
cargo test -p tack-api dispatch_ack_carries_the_approval_token_without_a_second_call
```

which asserts both that `approval_token` is non-null on a `waiting_approval` dispatch **and**
that wiremock observed exactly one docket request. It is the plan's canonical
green-CI-broken-product regression, and it is why "the OpenAPI spec did not change" is
never accepted here as evidence.

### The rest of the safety net, unchanged and re-run at every phase

- `cargo test -p tack-orch` - the 37 `DocketAdapter` wiremock tests and the ~40
  `reconciler.rs` state-machine tests. **Verdict assertions at `reconciler.rs:1998-2107`
  may not change.** Renaming `evaluate`'s input fields is fine; if an asserted *verdict*
  has to move, docket's behaviour moved and the change is wrong.
- `cargo test -p tack-api` - the 14 orchestration test files (~4500 lines), in particular
  `orch_dispatch_test.rs`, `sprint_dispatch_test.rs`, `orch_terminal_status_test.rs`,
  `orch_approvals_test.rs`, `provisioning_test.rs`.
- `cargo test -p tack-api a_status_map_saved_before_this_release_behaves_identically`
  (item 4.4) - the guarantee that adding `on_blocked`/`on_timed_out` does not change any
  `status_map` already in a user's database.
- `cargo llvm-cov -p tack-orch --fail-under-lines 70` - a floor that does not exist today.

---

## 7. Traps

**T1 - Echo loop.** Three layers, because any one alone is defeatable (item 8.4).
(a) A `ChangeOrigin` tag threaded through the mutation path, so a webhook-originated write
never reaches `maybe_sync_github` (`items.rs:281`). (b) `github_links.state_hash`: an
inbound event whose state equals what Tack last pushed is a no-op - this catches the loop
even when the origin tag is lost across a process boundary. (c) Drop deliveries whose
`sender.id` is the identity Tack pushes as. Note `ItemSource` cannot serve here: it is
written once at creation and `update_item` has no code path that touches the column
(`orchestration.md:361-390`). The verification is adversarial: `cargo test -p tack-api
a_webhook_driven_status_change_produces_no_outbound_push` asserts the mock GitHub received
**zero** requests.

**T2 - Concurrent writes with no concurrency control.** Phase 2, per D4: `version` columns,
`ETag`, `If-Match`, `412`, and an absent header preserving today's behaviour exactly. Two
honest limits are written into the docs rather than papered over (item 2.6): the reconciler
calls `dispatcher::apply_mapped_status` directly from `orch_store.rs:610` with no request
and no `If-Match` - **the largest single mutator of `items.status` is outside this control
by design** - and `propagate_parent_completion` (`items.rs:311-327`) mutates a *parent*
item on a child's PATCH, so a parent's ETag changes with no caller having touched it. Item
2.5 closes the worst gap: `tack-cli`'s `client.rs:103` cannot set a header at all, so every
MCP write is currently unconditionally last-write-wins - the agent-versus-human race is
precisely the one this exists for, and the agent path was the unprotected one.

**T3 - Retries creating duplicates.** Four independent mechanisms, one per source.
Webhook redelivery: dedupe on `X-GitHub-Delivery` (item 8.2). Pushed telemetry: a
caller-supplied `event_id` behind a partial unique index on
`(control_plane_id, external_id)` (items 6.3, 6.5). Dispatch: the existing
`ACTIVE_TASK_STATUSES` in-flight guard (`dispatcher.rs:128`) and the process-wide
`DISPATCH_LOCKS` mutex (`:158`) both survive the reshape unchanged. Correlation bind: the
nonce is **single-use** and consumed (item 6.2), so a retried bind is a 403, not a second
run. The verification for each is a replay that asserts a `SELECT COUNT(*)` did not move.

**T4 - Ephemeral runners and the event cursor.** `events(scope, cursor)` for GitHub Actions
does **not** read logs. `GET .../runs/{id}/logs` is a `302` whose link expires in one
minute and whose archive is deleted on the retention schedule (default 90 days, public
1-90, private 1-400) - it is a zip of text, not an ordered stream, and building on it would
make history evaporate. Instead `EventScope::Run` derives events from
`GET .../runs/{id}/jobs`: a small, bounded, monotone list of jobs and steps, each with
`status`, `conclusion`, `started_at`, `completed_at`. The cursor is the highest
`(job_id, step_number)` observed - a genuine resume cursor over a non-stream, and still
opaque to Tack per the discipline `TracesPage::next` established (`lib.rs:546-570`). The
pushed telemetry channel has an entirely separate cursor: **Tack's own row id**, never the
provider's, because Tack minted those rows. `capabilities().event_scope` is what tells the
reconciler which of the two shapes it is dealing with, and `orch_trace_cursors` keeps its
`(control_plane_id, remote_project)` key for `Project` scope - the key
`migrations.rs:619-624` argues for explicitly - gaining a run-scoped key alongside.

**T5 - Credential custody.** What Tack holds after this plan: control-plane tokens
(`control_planes.token`), provider secrets (`control_planes.secrets`, new), a global GitHub
PAT (`TACK_GITHUB_TOKEN`), an optional gateway spend-query credential, and per-run
credentials. Every one of them is in `tack.db`, and **`tack.db` is a file.** Concretely:
- Copying the binary is harmless; copying `tack.db` hands over every stored credential.
  That is true today and this plan does not change it - it changes how much is in there,
  which is why item 1.3 extends `remote_backup.rs::scrub_snapshot_secrets` in the **same
  commit** that adds the column. That function's own doc at `:257-261` states the rule and
  the plan follows it: null before the `VACUUM` at `:331`, row survives, operator re-enters.
- A gateway credential landing in `app_meta` must also join `SENSITIVE_META_KEYS`
  (`:262`) - item 7.5.
- **Workflow inputs are visible in the run's UI and logs.** No vendor key, gateway key or
  API token ever travels as an input. Only the non-secret `tack_run_id` nonce does. The
  run's *own* credential is obtained by exchanging that nonce plus one static repo secret
  at `POST /api/fleet/runs/bind` (item 6.2), so the credential itself never appears
  anywhere loggable.
- Because the nonce is caller-supplied, anyone with `actions:write` could forge one - hence
  the bind endpoint verifies it was minted by Tack, for that plane, and is still unbound,
  and consumes it. Three negative cases are tested.
- **Documented hole:** Actions secrets are unavailable to `pull_request` runs from forks.
  Those runs cannot bind and will render "not instrumented" - which is correct, and item
  6.7 says so in the operator guide rather than leaving it to be discovered.
- **Restore hazard, previously silent:** a restored backup has `secrets IS NULL`, and
  `orch_store.rs:172-218` currently `continue`s past a failed adapter construction with
  only a `warn!` - the plane vanishes from polling invisibly. Item 1.4 adds
  `health = 'unconfigured'` so the operator sees it.

**T6 - Without per-user identity, evidence has no actor.** Confirmed as a **risk and a
dependency, not solved here** - the brief puts it out of scope and I am not smuggling it
in. What this plan does instead is refuse to imply attribution it does not have: item 5.4
records the *surface* a decision was resolved through (UI, API, GitHub) and the UI never
renders a user name. One shared `TACK_ORCH_APPROVAL_TOKEN` means anyone holding it can
resolve any decision, unattributed. That is fine for a solo operator and **not** fine for
the governance positioning, so it appears in section 9 as a blocking dependency for that
claim, not for this code.

**T7 - Retention and growth.** Pushed telemetry changes the growth curve qualitatively: a
`PostToolUse` hook can emit hundreds of events per run, where docket's trace poll emits
tens. `TACK_ORCH_EVENT_RETENTION_DAYS` (90) governs *age*, not *rate*, so it is not
sufficient alone. Three additions: (a) a per-run event cap enforced at ingest, with the
overflow counted rather than stored and disclosed through the existing `events_truncated`
mechanism the UI already renders (`AgentActivityTab.tsx:134-139`); (b) pushed events go
through the **same** rollup-then-purge sweep, so `orch_events_daily` stays the single
long-term aggregate; (c) the documented consequence stays documented - the daily aggregate
drops `item_id`, so per-item history past the window is *not recoverable at any
granularity*, as `user-guide/orchestration.md:471-475` already states. `orch_tasks` remains
never purged, which is why tokens, cost and lead time survive while rework rate truncates.

**T8 - Public contract compatibility.** Broken, with notice (D3). 91 operations, 21 tagged
`orchestration`, all reshaped in place in Phase 4; `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts` regenerated in the same commit; `CHANGELOG.md`
carries a breaking-change section naming every changed operation. Justified by 0.1.0-beta.6
with orchestration off by default, and it leaves no legacy shapes to maintain. The two CI
drift gates (`ci.yml:31-35` and `ci.yml:164-171`) make the break **visible and deliberate**
rather than accidental - an unintended shape change fails the same gate. One thing the
gates cannot see, called out separately in section 6: a field that survives but whose value
dies.

**T9 - A blocking hook is a distributed lock with a billing meter attached.** The ceiling
is stated, enforced and layered (items 9.2, 9.3):
- The reference hook sets `timeout: 600` **explicitly** rather than relying on the Claude
  Code default, and requests `wait=540`, leaving headroom for the round trip. A hook that
  outlives its own timeout is killed and its verdict lost, so the wait must be strictly
  inside it.
- 540 s is far under the 6 h GitHub-hosted job cap and the 5-day self-hosted cap, so the
  wait can never be the thing that kills a job. Minutes, not hours, deliberately.
- **Expiry is fail-closed**: the endpoint returns `deny`, the hook exits 2, the tool call is
  blocked, and an `orch_events` row records the expiry with its reason.
- **Where the item lands:** `on_blocked` if the project's `status_map` sets it, otherwise
  the item does not move and the decision is recorded as expired. Never silently "done" -
  asserted by `an_expired_decision_never_moves_an_item_to_a_done_status`.
- The cost is named in the operator guide: the wait is **paid idle runner time**, so
  decisions belong at genuine gates, not per tool call.

**T10 - Compute is cheap and intelligence is not.** Two meters, two vendors, and the UI
**never aggregates them into one number**. Runner minutes come from GitHub
(`GET .../runs/{id}/timing`); token spend comes from pushed telemetry or a gateway. Each
figure carries its source label, and the existing discipline that
`formatEstimatedCost` must always contain the word "estimated" and never the word "spend"
(`fleet/format.test.ts`) extends to a source label per figure. There is no combined "total
cost" tile, and no "cost" column that silently sums both. At realistic volumes the runner
is low single-digit percent of the total, so a combined figure would be dominated by, and
therefore hide, the meter that matters.

**T11 - Gateway unavailability is a fleet-wide outage.** Tack never routes model traffic
(non-negotiable 2), so it cannot fail a request at dispatch time - the run itself discovers
the gateway is down. What Tack does own is the decision to *start* a run it cannot measure.
**The plan refuses.** When a project has a gateway configured and a pre-dispatch reachability
probe fails, dispatch returns a new outcome `gateway_unreachable` (HTTP 200, branch on
`outcome` like the existing six) and no run is started. The argument for refusing over
falling back to a direct vendor key: a run that silently bypasses the gateway is
**unmeasured and uncapped** - it produces exactly the "shows zero, actually spent money"
failure this codebase's numeric-honesty rules exist to prevent, and it does so at the moment
the operator has least visibility. Queuing is rejected for a different reason: Tack has no
queue and no replay logic anywhere (the reconciler exists specifically so it needs none), and
adding one for this case would be the largest new mechanism in the plan for the rarest
event. A project with no gateway configured is unaffected - the probe does not run.

**T12 - Telemetry depends on the target repo being instrumented, and Tack cannot verify
that.** "Not instrumented" is a first-class rendered state, distinct from "idle", and it is
established by evidence rather than inferred from silence: the run binds at its first step
(item 6.2), so a bind is proof of instrumentation and its absence is the signal. Two
conflations are avoided explicitly:
- A run parked on a required-reviewer environment sits in `waiting` for up to 35 days and
  is exactly what the decision inbox exists to surface. The timer is suppressed whenever
  the run's status is `waiting` or a `Decision` is open against it
  (`a_waiting_run_is_never_labelled_not_instrumented`).
- A run that goes `queued -> cancelled` without ever starting never arms an
  `in_progress`-based timer, so the reaper runs off a **dispatch-time deadline** instead,
  and leaves an `orch_events` row explaining why the card stopped
  (`a_run_cancelled_before_starting_is_reaped_and_leaves_an_event`). Without this the card
  sits in "In Progress" forever with nothing recording the reason -
  `orch_store.rs:~358`'s `let Some(item_id) = ... else { continue }` means
  `reconcile_terminal_status_map` never even runs for an unbound run.

---

## 8. Type C decisions detected

### Asked and answered before planning (D1-D7)

| # | Decision | Options weighed | Chosen | Cost of the choice |
|---|---|---|---|---|
| D1 | Final public shape of `ControlPlane` | one breaking reshape / additive-and-deprecate / capabilities-only | **breaking reshape** | one wide, risky commit; mitigated by the Phase 0 oracle |
| D2 | Adding an inbound event ingest endpoint | new token / reuse `TACK_API_TOKEN` / per-plane HMAC / do not add | **add it** | a new public surface and a new auth surface |
| D3 | Published route shapes | additive+aliases / `/api/v2` / break with notice | **break with notice** | every external client of 21 operations must update |
| D4 | Concurrency control | `version` column / `updated_at` ETag / idempotency-only / defer | **`version` column** | three additive migrations; does not cover non-HTTP writers |
| D5 | Auth model for runs | one scoped run credential / three secrets / ingest-only | **scoped run credential** | new minting, expiry and revocation machinery |
| D6 | Non-additive migrations | rebuild / new tables + dual-read / namespaced ids | **rebuild two tables** | the only irreversible step in the plan; see 10.3 |
| D7 | Plane-wide metrics | keep on trait / docket-only route / drop | **keep on trait** | one trait method most providers will not implement |

### Detected during planning, decided here with the reasoning shown

**8.1 - One `ALTER` per migration name.** `migrations.rs:82-109` runs each statement
individually **with no wrapping transaction**, and records the migration name only after
every statement succeeds. A multi-`ALTER` migration failing on statement three records
nothing; the next boot re-runs statement one, hits SQLite's `duplicate column name`, and
**the server never boots again**, with no down-migration. Both existing `ALTER` migrations
(029 at `:653`, 030 at `:665`) are single statements. Options were "batch them for fewer
migration names" versus "one each". Chosen: one each. It is more migration names and it is
the difference between a failed upgrade and a bricked install.

**8.2 - `orch_events.source` vocabulary is three values, not two.** The claim that
`orch_events.run_id` is always NULL and `id` is always a UUIDv5 is **true only of the
reconciler's ingestion path**. `orch_store.rs:663-681` writes `Uuid::new_v4()` with
`run_id: Some(..)` for `status_map_skipped_human_override`; `dispatcher.rs:~555` and the
auto-dispatch hook in `items.rs` write `new_v4()` with `run_id: None`. So the vocabulary is
`'poll' | 'push' | 'local'`, and item 6.4 backfills existing locally-minted rows keyed on
`event_type`. A `NOT NULL DEFAULT 'poll'` alone would mislabel every one of them - **and
this is a write to data already in a user's database**, which is why it is here and not
treated as ordinary.

**8.3 - The bind and ingest routes sit outside both existing gates.** `router.rs:323`
layers `require_token` over the whole `/api` router and `orch_routes` adds
`require_orch_enabled` (409). Leaving the new routes inside means a run needs
`TACK_API_TOKEN` as well as its own credential, **and** that flipping the UI orchestration
toggle 409s in-flight handshakes and re-labels every live run "not instrumented". Options:
a fourth entry in `middleware.rs:20-34`, or a separate sub-router. Chosen: **a separate
sub-router**, because those exemptions are `path().ends_with(...)` **suffix matches** and an
exemption for `/events` would silently exempt any current or future path ending in the same
string. Item 6.6 adds `no_other_route_became_exempt`, which enumerates the router and
asserts the exemption set is exactly the four intended paths.

**8.4 - `handlers/items.rs:371` gates auto-dispatch on `state.config.orch_enable`, not
`effective_orch_enabled`.** Every HTTP route uses the effective value
(`handlers/settings.rs:265`); the auto-dispatch hook uses the raw env flag and therefore
**ignores the UI toggle today**. Harmless-ish with docket; with a GitHub Actions plane it
means a workflow dispatched automatically while the UI reports orchestration off. Treated
as type C rather than an ordinary bug fix because it changes the behaviour of a shipped
feature: an operator who set `TACK_ORCH_ENABLE=1` and turned orchestration off in the UI
currently still gets auto-dispatch, and will stop getting it. Fixed in Phase 1, called out
in `CHANGELOG.md`.

**8.5 - `POST /api/templates/{id}/provision` changes shape.** Provisioning leaves the trait
(item 4.2) and becomes provider-specific. Under D3 this is in-scope breakage, but it is
named separately because it is the one orchestration operation that creates **real,
irreversible infrastructure**, and its confirmation copy ("This creates real infrastructure
and cannot be automatically undone", `ProvisioningWizard.tsx:386`) must survive the move
intact.

**8.6 - New environment variable, fail-closed.** `TACK_ORCH_RUN_BOOTSTRAP_TOKEN` gates the
nonce-exchange endpoint. Its default follows `TACK_ORCH_APPROVAL_TOKEN`'s precedent, not
`TACK_API_TOKEN`'s: **unset means the endpoint refuses, always** - the opposite of
`middleware.rs:36-39`, where an unconfigured `api_token` allows everything. A new auth
surface must not inherit the permissive default.

**8.7 - No new `Cargo.toml` dependency is required, and that is a constraint on the design,
not an observation.** `reqwest` 0.12, `serde`, `serde_json`, `hmac` 0.13, `sha2` 0.11,
`hex` 0.4, `uuid` (v4+v5), `tower` (features `full`, so retry is available), `wiremock` 0.6
cover everything. Two things this forecloses: `octocrab` (forbidden anyway by
`crates/tack-orch/Cargo.toml:20-23`, "never a second HTTP client crate"), and
`jsonwebtoken` - which is why GitHub **App** authentication is an open question (9.1)
rather than a plan decision.

**8.8 - Nothing is renamed in a user's database.** `orch_approvals.token` stays the primary
key and the URL segment even as the concept becomes "decision". `orch_runs.run_id` becomes
`external_run_id` **only** inside a table this plan rebuilds wholesale (D6), with the value
copied across, so no row loses its identity.

---

## 9. Open questions

These are things the brief does not answer and I want decided before starting. None blocks
Phase 0.

**9.1 - GitHub authentication: fine-grained PAT or GitHub App?** *(blocks Phase 6.)*
A PAT needs no new dependency and works today with `TACK_GITHUB_TOKEN`'s existing plumbing,
but it shares an identity with a human, which weakens `sender.id`-based echo suppression
(T1 layer c) and inherits that human's rate limit. A GitHub App gives a distinct bot
identity, per-repo installation scoping and a higher rate limit - but it needs
`jsonwebtoken` for the installation-token flow, which is a new `Cargo.toml` dependency and
therefore type C. My recommendation is **fine-grained PAT for Phase 6**, with App support as
a later capability, because layers (a) and (b) of the echo suppression do not depend on
identity.

**9.2 - Should `orch_links` stop being one-link-per-project?** Its primary key is
`project_id` (`migrations.rs:423`). A project that has both a docket pod and a GitHub
Actions workflow cannot be expressed. **I have not planned for this** - the brief does not
ask for it, and it is a third table rebuild. But it is the most likely thing to be wanted
the day after Phase 6 ships, and deciding it now is much cheaper than deciding it after.

**9.3 - Which retention window governs pushed telemetry?** T7 reuses
`TACK_ORCH_EVENT_RETENTION_DAYS` (90). Pushed events arrive at a far higher rate than
polled ones, so 90 days of them may be the wrong default for a single-binary SQLite install.
Options: reuse the existing window; add `TACK_ORCH_PUSH_RETENTION_DAYS` with a shorter
default; or keep one window and rely solely on the per-run cap. I lean toward the per-run
cap plus the existing window, but the sizing is a judgement about real installs that I do
not have data for.

**9.4 - Where does the workflow file name live?** `orch_links.provider_config` as JSON
(flexible, invisible to the schema) or a dedicated column (typed, queryable, but a docket
link would carry a NULL GitHub column). Same question for `harness`, which item 7.7 plans
as a dedicated column deliberately - "one field, so a future change is a config edit rather
than a refactor". Consistency argues for one answer to both.

**9.5 - Does the governance positioning need per-user identity before this cycle can be
marketed?** T6 is out of scope for the code and I have kept it out. But "this feature cost
4.2M tokens across 3 runs and 2 reworks, and Ana approved the irreversible step" is a
materially stronger claim than the same sentence without a name, and the second half is not
buildable on one shared token. Not a code question - a sequencing question about what this
cycle is allowed to claim.

**9.6 - Is a two-release break acceptable if Phase 6 forces a second reshape?** D3 breaks
the contract once, in Phase 4. Section 10.2 explains why a second break is a live
possibility. If one break is the budget, Phase 4 should be held back and merged into Phase
6 - which makes one very large commit instead of two, and loses the "docket golden
unchanged" gate as a standalone checkpoint. I recommend keeping them separate and accepting
the risk, but it is a trade I should not make alone.

---

## 10. What can go wrong

Five concrete risks of **this** plan, not of software in general.

**10.1 - The Phase 4 golden becomes unfalsifiable the moment it legitimately changes.**
The entire safety argument for the reshape is "the tick golden did not move". But some
change during Phase 4 will plausibly *require* it to move - say the reshaped reconciler
issues `GET /health` before `GET /status.json` where it previously issued them in the other
order. At that moment there is no way to distinguish "the golden legitimately changed" from
"we broke it", and the temptation is to regenerate and move on. **Mitigation, and it must be
a rule rather than an intention:** the golden may only change in a commit that changes
nothing else, whose message states the behavioural difference and why it is safe, and whose
diff is read line by line. If more than two such commits appear in Phase 4, the reshape is
not preserving behaviour and should be stopped.

**10.2 - The trait is designed against one real implementor, so Phase 6 may force a second
reshape - after the contract has already been broken once.** Item 1.6's compile-only GitHub
Actions stub reduces this by making "both adapters compile" a Phase 4 gate, but a stub
cannot discover what a real adapter discovers: that `health()` must not touch the runner
list because it needs repo-admin; that there is no plane-wide usage figure; that `waiting`
is a decision and not a state. Each of those was found by reading the API this week; the
next three will be found by writing the adapter. **If Phase 6 forces a trait change, D3's
single break becomes two breaks in consecutive releases** - which is a worse outcome than
either option I offered when asking. Question 9.6 exists because of this.

**10.3 - Phase 3's rebuilds are the only irreversible step, and the recovery path depends on
a backup the operator may not have taken.** `migrations.rs` has no transactions and no
down-migrations. A `DROP TABLE orch_runs` that succeeds followed by a `RENAME` that fails
leaves the data gone. Item 3.3's guard refuses to boot rather than compounding the damage,
and item 3.4 tells the operator to back up first - but "we told them to" is not a recovery
plan. **The honest statement is that Phase 3 can lose orchestration history for an operator
who upgrades without a backup on a machine that loses power mid-migration.** Item counts,
not correctness, are what is at risk (items, projects, sprints are untouched), which is why
I still recommend it over the namespaced-ids alternative - but it should be shipped in a
release of its own, not bundled.

**10.4 - Everything good about the GitHub Actions adapter depends on the target repo being
instrumented, and that is three separate things the repo owner must do.** Add a workflow,
add a hook, set a secret. If that friction is too high in practice, the adapter degrades to
lifecycle-only - queued/running/succeeded/failed and nothing else - and the positioning
claim that motivates the whole plan ("this feature cost 4.2M tokens across 3 runs and 2
reworks") stays unprovable for the new provider. D5 cuts the secret count from three to one,
which is the single biggest lever available, and item 6.7 ships a copy-pasteable reference.
But the plan cannot make a stranger's repo instrument itself, and the risk that adapter 2
ends up *technically* proving the trait while *practically* proving nothing about the
product claim is real.

**10.5 - Phase 7 turns on a measurement that has always read zero, with no baseline to check
it against.** `orch_tasks.tokens_in`/`tokens_out` have been written as literal `0` since
they were added and updated by nothing (`dispatcher.rs:382`; `rg -l 'tokens_in' crates/`
shows no other writer). Every figure in the Fleet view, the Budget panel and the Economics
page currently reads a structural zero, which means the whole numeric surface has never once
been exercised against real data. When Phase 7 lands, every number becomes non-zero at once
and **a units error would ship looking entirely plausible** - input counted as output,
cumulative counted as delta, a per-attempt figure summed across retries. The existing
min-sample and selection-bias discipline (`handlers/economics.rs:78-95`) protects against
over-claiming from small samples; it does not protect against a number that is simply wrong
by a factor. Phase 7 needs a hand-verified reconciliation against one real run's provider
figures before the Economics page is allowed to render the new values - and that check is
not something CI can do for us.

