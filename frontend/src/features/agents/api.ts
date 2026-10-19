// Wire-format boundary for the embedded-runner control surface (a UI-only
// user turns the in-process runner on and off on a loopback bind, without a
// console). Every assumption about `GET`/`PUT /api/local-runner` lives in
// this one file — `ExecutionToggle.tsx` only ever imports types and
// functions from here, never building a request body or reading a raw wire
// field itself.
//
// ── Why these routes can be a genuine 404 ───────────────────────────────
//
// Unlike the orchestration settings route (deliberately reachable even
// when orchestration is off, so an operator can discover the toggle),
// `/api/local-runner*` is absent — not present-and-refusing — on any
// non-loopback bind, or when the process serving the API never wired an
// embedded runner in at all (`crates/tack-api/src/router.rs`'s
// `local_runner_available` check, `build_router`). An embedded runner
// executes arbitrary agent processes on the host serving the UI, so it must
// not even be discoverable from outside the machine it runs on.
// `isLocalRunnerUnavailable` below is the one place that 404 is
// interpreted — the toggle explains this is a remote-runner deployment
// instead of a bare error, pointing at the Advanced section's enrollment flow.
//
// ── The `GET/PUT /api/local-runner` contract ────────────────────────────
//
//   GET  /api/local-runner  -> { enabled, state, since }
//     `enabled` is the persisted preference (survives a restart); `state`
//     ("running" | "stopped") is the live runtime, which can briefly lag
//     `enabled` right after boot.
//   PUT  /api/local-runner  body: { enabled: boolean } -> 204
//     Persists the preference and starts/stops the runner to match, with no
//     restart.
//

import { ApiError, request } from '../../shared/api/client';

export type LocalRunnerState = 'running' | 'stopped';

export interface LocalRunnerStatus {
  enabled: boolean;
  state: LocalRunnerState;
  since: string | null;
  /** How many runs this computer works at once. */
  max_concurrent_attempts: number;
  /** How many it is running right now. */
  active_attempts: number;
  /** The settings file the runner read; null when it runs on defaults (no docket, no verifier). */
  config_file?: string | null;
}

export interface UpdateLocalRunnerBody {
  enabled: boolean;
}

/** True when a request failed because this deployment has no reachable
 *  embedded-runner control surface — either the server isn't bound to
 *  loopback, or the process serving it never wired one in at all. Both
 *  render as a plain 404 with no error envelope (see this file's header) —
 *  the canonical check every caller of this API should use instead of
 *  matching on `err.status === 404` itself. */
export function isLocalRunnerUnavailable(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

export const localRunnerApi = {
  get: () => request<LocalRunnerStatus>('/local-runner'),

  update: (enabled: boolean) =>
    request<void>('/local-runner', {
      method: 'PUT',
      body: JSON.stringify({ enabled } satisfies UpdateLocalRunnerBody),
    }),

  testRun: (harnessKind: string) =>
    request<{ request_id: string }>('/local-runner/test-run', {
      method: 'POST',
      body: JSON.stringify({ harness_kind: harnessKind }),
    }),
};
