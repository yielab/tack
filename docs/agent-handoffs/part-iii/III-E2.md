# III-E2 handoff

- **Base SHA / branch / final SHA:** base `b6dd0370564a3a4461b05d98d51d9e77c6d231c0` ("docs: bring CLAUDE.md up to date with the runner fleet") on `plan/harness-agnostic-agent-fleet`, worked in an isolated worktree on `agent/iii-e2-frontend-state`. Two commits: `e7daa22` (the feature itself, including this handoff) and `22ab5ae` ("fix(frontend): remove stray NUL byte in capabilities.ts" — a template-literal space character was corrupted into a NUL byte during an in-flight edit, which made git report the file as binary; caught by inspecting `git show --stat` after the first commit, fixed with no behavior change, verified 0 NUL bytes in the final committed blob and all tests still passing). **Final SHA: `22ab5ae`.**
- **Files changed (must equal ownership list):** new `frontend/src/shared/execution/{types,api,capabilities,cache,realtime,store,index}.ts` plus their co-located `*.test.ts` files, and this handoff. Nothing under `frontend/src/features/**`, `frontend/src/shared/api/schema.gen.ts`, `docs/openapi.json`, any Rust crate, or any other handoff/TODO.md was touched — confirmed by `git status --porcelain` (see Checklist).
- **Contract fixtures consumed:** `docs/contracts/runner-v1/capabilities.json` (grounds `types.ts`'s `RunnerCapabilities` and every `capabilities.ts` function), `docs/contracts/runner-v1/lifecycle-transitions.json` (grounds `ExecutionState`/`TERMINAL_EXECUTION_STATES`), `docs/contracts/runner-v1/errors/*.json` (grounds `StableErrorCode` and `store.ts`'s conflict/error classification), `docs/contracts/runner-v1/claim.response.json` (cross-checked `ExecutionRequestSnapshot`/`AttemptSnapshot` field names against `crates/tack-orch/src/execution/types.rs`). None of these fixtures were modified.
- **Behavior implemented:** see "Public surface" and "Tasks, one by one" below.
- **Tests added and exact commands/results:** `cd frontend && npx vitest run src/shared/execution` — **73 passed, 0 failed** (5 files: `api.test.ts` 15, `cache.test.ts` 12, `capabilities.test.ts` 18, `realtime.test.ts` 9, `store.test.ts` 19). `npm run type-check` (`tsc -b`) — clean. `npm test` (full frontend suite, `vitest run`) — **555 passed, 0 failed** across 65 files (my 73 plus the pre-existing 482), confirming no regression to any file I don't own.
- **Failure/adversarial case proved:** see "Adversarial proofs" below — the ordering-guard test that initially caught a real bug in my own first draft, the localeCompare non-determinism caught by a test, and the double-dispose/no-leak proof for realtime.
- **Schema/API/contract change requested from another owner:** five items, all detailed under "Gaps found and requested" below — summarized: (1) no `GET /runners` (list) endpoint exists at all; (2) `GET /executions/{id}` returns only 5 scalar columns, no attempt/event/decision data; (3) `GET /runner-fleets/{id}` (single fleet + roster) doesn't exist, only list; (4) `POST /executions/{id}/cancel`'s response `state` field is not a real `ExecutionState` value and disagrees with what a subsequent `GET` shows; (5) every operator execution/fleet/runner/profile route's OpenAPI schema is `{}` (empty) in `docs/openapi.json`, so `schema.gen.ts` has no generated types for this domain at all — corroborated independently by III-C1's own handoff ("List/detail DTOs are card-local and will need C5's generated-schema integration").
- **Known limitations or `not_measured` fields:** `attemptsFor()` always returns the typed `{status: 'not_available', reason: ...}` placeholder — there is no attempt-read endpoint to back a real value (see gap 2). `RunnerCapabilities`-based capability selection in `capabilities.ts` is fully implemented and tested against the frozen fixture shape but has no live data source yet (see gap 1) — any UI built on it today would need an operator to hand it synthetic data, or wait for the endpoint. `realtime.ts` is a bounded poll, not a push channel — see its header comment for why, and the request for a future `execution_updated`-style channel.
- **Secrets/logging review:** `EnrollRunnerResult.enrollment_token` (`api.ts`) is documented as a raw, one-time secret mirroring the backend's own "never readable again" guarantee; nothing in `store.ts`/`cache.ts`/`realtime.ts` persists it — the store never even has a code path that touches `runnersApi.enroll`'s result (that composition is left to E3). No console/logger call anywhere in this module logs a full request/response body; `console.error` calls (in `boardSocket.ts`-style listener-exception guards in `realtime.ts`) log only the caught `Error`, never wire payloads. `ApiError` messages (server-authored, potentially containing operator-visible but non-secret text like "Execution already reached a terminal state...") flow into `NormalizedExecutionError.message`, matching how every other `shared/*` module in this codebase already treats `ApiError.message`.
- **Safe merge order and likely conflicts:** no dependency on E1/E5's Rust changes and no shared-file overlap with either (E1 owns `crates/tack-orch/src/scheduler/**`; E5 owns `tack-cli`). Safe to merge in any order relative to them. E3 and E4 depend on this card per TODO.md's Wave 4 graph (`E1 E2 E5 → E3 E4 → E6`) — they should rebase onto this branch (or the wave integrator's merge of it) before starting, and should import only from `frontend/src/shared/execution/index.ts` (the barrel), not individual files, per that file's own header note. No conflict expected with any other Wave 4 card since `frontend/src/shared/execution/**` is exclusively mine per TODO.md III.3.
- **Checklist:** no unowned files (`git status --porcelain` shows only new files under `frontend/src/shared/execution/` and this handoff); no live secret (see Secrets/logging review); no panic stub (no `throw new Error('not implemented')`/TODO-stub anywhere — every unsupported/unavailable case is a typed value, e.g. `AttemptAvailability`); no blind retry (nothing in this module retries automatically — `store.ts`'s `cancel()` explicitly de-duplicates a *concurrent* call rather than retrying a failed one, and callers decide their own retry policy).

## Public surface (for E3/E4)

Import everything from `frontend/src/shared/execution/index.ts`. The barrel's own header comment repeats this; noted here too since E3/E4 build directly on it without reading this file's implementation.

### Types (`types.ts`)

Hand-typed, not generated (see "Gap 5" below for why) — mirrors `crates/tack-orch/src/execution/{types,capabilities}.rs` and the frozen `docs/contracts/runner-v1/` fixtures field-for-field, cited per type in the file:

- `ExecutionState` (the frozen 10-state lifecycle), `TERMINAL_EXECUTION_STATES`, `isTerminalExecutionState()`.
- `RunnerSelector` (tagged `exact_runner | fleet | any`).
- `RunnerCapabilities` and its parts (`CapabilitySupport`, `CapabilityValue`, `FeatureCapabilities`, `Concurrency`, `ModelCombination`, `HarnessCapability`, `CapabilityLimits`) — byte-shape-compatible with `docs/contracts/runner-v1/capabilities.json`.
- `MeasurementSource`, `Measurement<T>`, `Usage` — nullable-with-provenance usage figures.
- `StableErrorCode` — all 15 frozen v1 error codes.

### API wrappers (`api.ts`)

One object per operator resource, each method returning either the parsed body (`request<T>`) or `{data, headers}` (`requestWithHeaders<T>`, used for every `GET`/`list` so a future response header is never silently dropped):

- `executionsApi`: `list()`, `get(requestId)`, `create(input)`, `cancel(requestId)`, `requeue(requestId, input)`.
- `fleetsApi`: `list()`, `create(input)`.
- `agentProfilesApi`: `list()`, `create(input)`.
- `modelProfilesApi`: `list()`, `create(input)`.
- `runnersApi`: `enroll(input)`, `revokeRunner(runnerId)`, `revokeEnrollmentToken(runnerId, tokenId)`. **No `list()`** — see Gap 1.

Every wrapper's request/response shape is copied field-for-field from the live Rust handler (`crates/tack-api/src/handlers/executions.rs` / `runner_admin.rs`), cited by line number in each type's doc comment, since `docs/openapi.json` has no schema for this domain yet.

### Capability selector (`capabilities.ts`)

Pure functions, no I/O, taking `RunnerCapabilities[]` as a plain argument:

- `gateFeature(capabilities, feature)` / `gateFeatureAcrossRunners(snapshots, feature)` → `CapabilityGate { enabled, reason }` for `cancel`/`resume`/`decisions`/`artifacts`/`usage`.
- `listReportedHarnessKinds(snapshots)`, `harnessProbeStatus(snapshots, harnessKind)`.
- `listModelCombinationsForHarness(snapshots, harnessKind)` → deduplicated, supporter-counted combinations.
- `isCombinationSupported(snapshots, harnessKind, modelProvider, modelId)` → `{supported, reason, supportingRunnerCount}` — **the function E4's submit-gate should call.** Always returns a typed reason, whether supported or not.

### Cache primitives (`cache.ts`)

`VersionedCache<T>` and `SequenceAllocator` — framework-agnostic building blocks proving "a stale event can never overwrite a newer snapshot" in isolation. `store.ts` composes both; a future card needing the same guarantee for a different keyed resource can reuse them directly.

### Realtime (`realtime.ts`)

`createExecutionRealtime(options)` → `{status, onInvalidate, dispose}` — a bounded-poll invalidation source (default every 4s: one `{scope:'list'}` event plus one `{scope:'request', requestId}` per id returned by `options.watchedRequestIds()`). `dispose()` is idempotent. See its header comment for why this is polling rather than a WebSocket (no push channel exists for this domain — Gap 6 below).

### Store (`store.ts`)

`createExecutionStore()` → `ExecutionStore`:

```ts
interface ExecutionStore {
  requests(): ReadonlyMap<string, ExecutionRequestRecord>;
  requestsForItem(itemId: string): ExecutionRequestRecord[];
  getRequest(requestId: string): ExecutionRequestRecord | undefined;
  listStatus(): 'idle' | 'loading' | 'ready' | 'error';
  listError(): NormalizedExecutionError | undefined;
  loadList(): Promise<void>;
  loadOne(requestId: string): Promise<void>;
  create(input: CreateExecutionInput): Promise<CreateExecutionResult>;
  cancel(requestId: string): Promise<void>;
  requeue(requestId: string, input: RequeueExecutionInput): Promise<void>;
  attemptsFor(requestId: string): AttemptAvailability; // always {status:'not_available', reason} today
  connectRealtime(realtime: ExecutionRealtime): () => void;
}

interface ExecutionRequestRecord {
  status: 'ready' | 'error';
  summary: ExecutionSummary | undefined;
  error: NormalizedExecutionError | undefined;
  cancellation: CancellationState; // { requested, pending, conflict, error }
  fetchedAt: number;
}
```

**One instance per app/test, shared via a context Provider** (the same shape as `shared/state/projectItemsContext.tsx` wraps `api.items.list`) — E3/E4 should not each call `createExecutionStore()` independently, or they get divergent copies, defeating the "one consistent state" acceptance bar. This card does not add that Provider itself (a Provider lives under `frontend/src/shared/state/` or a feature folder, both outside `frontend/src/shared/execution/**`'s ownership) — flagged as a small, deliberate omission for E3/E4 (or E6) to wire, not an oversight: the store factory is the reusable primitive, and exactly one Provider component wrapping it is a two-line addition whichever card first needs it in a component tree.

## Tasks, one by one

1. **API wrappers preserving headers/errors** — every `list`/`get` call uses `requestWithHeaders` (proven in `api.test.ts`'s "preserves response headers" test, which injects a custom header and asserts it survives); every error is a real `ApiError` thrown by `shared/api/client.ts`, never caught and downgraded.
2. **Item execution store** — `store.ts`'s `requestsForItem(itemId)`.
3. **Capability selector** — `capabilities.ts`, described above.
4. **Request/attempt cache** — `cache.ts`'s `VersionedCache`/`SequenceAllocator`, composed by `store.ts`. The *attempt* half is the typed `AttemptAvailability` placeholder (Gap 2 blocks a real one).
5. **One realtime subscription/invalidation path** — `realtime.ts` + `store.ts#connectRealtime`.
6. **Optimistic cancellation with rollback and explicit conflict/error state** — `store.ts#cancel`: sets `{requested:true, pending:true}` synchronously before the network call; on success leaves `pending:true` until a fresh fetch confirms `cancellation_requested_at` (see Gap 4 for why it doesn't trust the cancel response's own `state`); on failure rolls back to `{requested:false, pending:false}` and sets either `conflict:true` (server returned `ApiError.code === 'conflict'`, matching the real terminal-state 409) or `error` (any other failure) — never both, never neither.

## Adversarial proofs

- **`cache.test.ts`** doesn't just assert the guard works — `proves the guard is load-bearing: without it, the stale write would win` demonstrates the failure mode a naive `Map`-based cache would have, so the passing guard test isn't vacuous.
- **`store.test.ts`**'s ordering-guard test (`a slow, earlier-issued fetch arriving after a later one must not overwrite it`) caught a real bug in my first draft: I originally allocated the sequence number *after* `await`ing the fetch (at resolution time) rather than before issuing it, which inverts the guarantee — a late-resolving *older* request would win, not lose. The fix (`store.ts`'s `GLOBAL_SEQUENCE_KEY` — one shared counter for every fetch, allocated at issue time) and its doc comment explain why a per-request-id counter alone can't work for `loadList()`, which doesn't know its row keys until the response arrives.
- **`realtime.test.ts`**: `dispose()` is idempotent (`clearIntervalImpl` spy called exactly once across two `dispose()` calls) and stops further ticks from reaching listeners (`no leak`); a listener that throws does not stop other listeners or crash the tick.
- **`store.test.ts`**'s cancel tests distinguish conflict (`ApiError.code === 'conflict'`) from a generic 500 by asserted, disjoint final states — `{conflict:true, error:undefined}` vs `{conflict:false, error:{...}}` — and a concurrent second `cancel()` call is proved to be a no-op (`mockedApi.cancel` called exactly once) rather than racing.
- **`capabilities.test.ts`** proves a harness report carrying a `probe_error` is excluded from `isCombinationSupported`/`listModelCombinationsForHarness` even when its (stale) `model_combinations` would otherwise match — a runner claiming a combination through a failed probe must never look "supported."

## Gaps found and requested from another owner

All five below block a *fully live* E3/E4 today; none blocked this card's own deliverable, since every function here is complete and tested against the real, current backend behavior or the frozen contract fixtures.

1. **No `GET /runners` (or per-fleet roster) endpoint.** `crates/tack-api/src/handlers/runner_admin.rs` registers only `POST /runners/enrollment`, `POST /runners/{id}/enrollment-tokens/{token_id}/revoke`, `POST /runners/{id}/revoke` — confirmed by reading the file's `routes()` function and grepping the whole crate for a `GET` on `/runners`. `agent_runners.capability_snapshot` (migration 040) is write-only from the operator's perspective. This blocks E3's "health/capacity/protocol/harness display" and any live wiring of `capabilities.ts` — the pure logic is ready, the data source is not.
2. **`GET /executions/{id}` returns 5 scalar columns only** (`request_id, item_id, state, cancellation_requested_at, created_at` — `executions.rs`'s `get_execution`), never selector, agent profile, requested harness/model, repository, permission policy, budgets, or any attempt/event/decision/artifact data, even though every one of those columns/tables already exists (`execution_requests`, `execution_attempts`, `execution_events`, `execution_artifacts`, `execution_decisions` — migrations 044-048). This is what forces `store.ts#attemptsFor` to always return the typed `not_available` placeholder.
3. **`GET /runner-fleets/{id}`** (single fleet + roster) doesn't exist, only the list endpoint (which itself returns only `fleet_id, name, concurrency_limit, default_policy` — no member roster).
4. **`POST /executions/{id}/cancel`'s response is inconsistent with `GET`.** `executions.rs`'s `request_cancellation` hardcodes `"state":"cancellation_requested"` in its success body — a string that is not a member of the frozen 10-state `ExecutionState` lifecycle (`docs/contracts/runner-v1/lifecycle-transitions.json` has no such state; cancellation intent is the separate `cancellation_requested_at` timestamp column, which `list`/`get` already expose correctly). A client that calls `cancel()` then immediately `get()`s will see the row's *real* current lifecycle state (e.g. still `"running"`) with `cancellation_requested_at` populated — not a `"cancellation_requested"` state. `store.ts` works around this by never merging the cancel response's `state` field into the cached `ExecutionSummary` (see `CancellationState`'s doc comment) — but the wire inconsistency itself is a small, easy fix for whoever owns `executions.rs` next: have the handler either omit `state` from the cancel response, or echo the row's real `state`/`cancellation_requested_at` pair instead of a synthetic string.
5. **Every operator execution/fleet/runner/profile route has an empty (`{}`) OpenAPI schema** in `docs/openapi.json` for every request/response body — confirmed by reading the generated spec directly (`python3 -c "import json; ..."` against `docs/openapi.json`, every `/api/executions*`, `/api/runner-fleets`, `/api/agent-profiles`, `/api/model-profiles`, `/api/runners/*` path). `frontend/src/shared/api/schema.gen.ts` therefore has no generated type for this entire domain (`content: { "application/json": unknown }` for every 200). This is why `types.ts`/`api.ts` are hand-typed rather than importing `components['schemas'][...]`, matching the existing precedent `shared/dispatch/api.ts` and `shared/orch/capabilities.ts` already set for domains reconciled against generated JSON without a matching TS type. **Independently corroborated**: III-C1's own handoff already flagged this ("List/detail DTOs are card-local and will need C5's generated-schema integration"). Per TODO.md's Wave 4 dependency graph, E6 is the card that does "route/spec/generated updates ... only after E1-E5" — this is a concrete, scoped punch list for that work: add `utoipa` schema annotations to `crates/tack-api/src/handlers/executions.rs` and `runner_admin.rs`'s response/request types, regenerate `docs/openapi.json` and `schema.gen.ts`, and this file's hand-typed shapes can be swapped for generated ones (they were written to match field-for-field, so the swap should be mechanical).
6. **No realtime/push channel for this domain.** `BoardEvent` (`shared/types/index.ts`, backed by `crates/tack-api/src/handlers/websocket.rs`) is the only push channel in the codebase, scoped to PM board changes plus Part II's docket-orchestration mirror (`agent_run_updated`/`approval_pending` — a *different* domain, `orch_runs`/`orch_tasks`, not `execution_requests`/`execution_attempts`; reusing it would be exactly the vocabulary collision III.0 warns against). `realtime.ts` implements the same subscribe/dispose contract over a bounded poll instead, documented in its own header as a stand-in. A future `execution_updated`-style `BoardEvent` variant (or a dedicated WebSocket) could replace `createExecutionRealtime`'s interior with zero change to `store.ts#connectRealtime`'s call site.

## Secondary, already-worked-around limitation

`shared/api/client.ts`'s `ApiError` (not owned by this card) extracts only `status` and `code` from the protocol error envelope — `retryable` and the per-code `details` object (e.g. `invalid_transition`'s `{from, to}`, `stale_lease`'s `{attempt_id, current_fencing_token}`) are silently dropped by `toApiError`. This card's conflict/error-state distinction only needs `code` (proven sufficient in `store.test.ts`), so it wasn't a blocker — but a future card wanting to render, say, "cannot transition from running to succeeded" using the server's own `{from,to}` detail would need `shared/api/client.ts` extended first. Not requested as an action item since no task in this card's scope needed it; noted for whoever next touches error rendering in this domain.
