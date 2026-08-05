// Wire-format boundary for the fleet-wide approvals inbox (TODO.md Wave 4,
// card D1, tasks 36.1/36.2). Every assumption about `GET /api/approvals` /
// `POST /api/approvals/{token}`'s request/response shapes lives in this one
// file — `ApprovalsPage.tsx` and `format.ts` only ever import types and
// functions from here, never construct a request body or read a raw field
// name themselves. Mirrors the pattern A5 set for `features/fleet/api.ts`
// and C4 repeated for `shared/dispatch/api.ts`.
//
// Both routes and every field below are copied field-for-field from the real
// Rust handler (`crates/tack-api/src/handlers/orch.rs`'s
// `PendingApprovalResponse`/`PendingApprovalListResponse`/
// `DecideApprovalRequest`/`DecideApprovalResponse`) and `docs/openapi.json`
// — not a guess (unlike A5's original Fleet draft, this file was written
// after the backend landed in the same session).

import { request, ApiError } from '../../shared/api/client';

/** One row of the fleet-wide approvals inbox, oldest-requested first.
 *  `item_id`/`item_title`/`item_status`/`project_id`/`project_name` are all
 *  `null` together for an **uncorrelated** approval — docket raised it but
 *  Tack couldn't attribute it to an item (a CLI-dispatched run, for
 *  instance). This is deliberately never filtered out: an approval Tack
 *  can't attribute is the one most likely to be silently blocking a fleet
 *  (the per-project Fleet view excludes these; this inbox is where they're
 *  meant to surface). */
export interface PendingApproval {
  token: string;
  control_plane_id: string;
  control_plane_name: string;
  item_id: string | null;
  item_title: string | null;
  item_status: string | null;
  project_id: string | null;
  project_name: string | null;
  remote_task_id: string | null;
  /** docket's `role` field — the closest thing to "which agent" docket's
   *  wire shape has (see `crates/tack-orch/src/reconciler.rs`'s ingestion
   *  comment). May be `null`. */
  agent: string | null;
  /** The gated action's description, already redacted by docket. */
  action: string | null;
  requested_at: string;
}

export interface PendingApprovalListResponse {
  rows: PendingApproval[];
  /** Whether `TACK_ORCH_APPROVAL_TOKEN` is configured on the server at all
   *  — never the value itself. `false` means nobody can grant or deny from
   *  Tack today, no matter what a caller types into the token field below;
   *  the UI uses this to decide whether to render decision controls at
   *  all. The server enforces the real check independently on every
   *  decide call regardless of this flag. */
  grant_available: boolean;
}

export type ApprovalDecisionActionValue = 'grant' | 'deny';

export interface DecideApprovalResponse {
  token: string;
  /** docket's own resulting state (`"granted"`/`"denied"`, or an
   *  unrecognised value shown as-is — same "never fail on an unknown
   *  remote value" discipline as everywhere else in this system). */
  state: string;
}

const APPROVAL_TOKEN_HEADER = 'X-Tack-Approval-Token';
const APPROVAL_TOKEN_STORAGE_KEY = 'tack_orch_approval_token';

/**
 * The operator's own copy of `TACK_ORCH_APPROVAL_TOKEN` — a **second**,
 * higher-privilege secret the server holds, deliberately separate from the
 * ordinary `TACK_API_TOKEN` `tokenStore` (`shared/api/client.ts`). Mirrors
 * that store's shape exactly (get/set, `localStorage`-backed, tolerant of
 * an unavailable store) but is never sent automatically the way the Bearer
 * token is — `approvalsApi.decide` is the only thing that ever reads it,
 * and only when actually deciding an approval, never on a plain `list()`.
 * Nothing about this value is ever logged or echoed back by the server
 * (the API's own `PendingApprovalListResponse.grant_available` boolean is
 * the only server-side signal about it that ever reaches the client).
 */
export const approvalTokenStore = {
  get(): string | null {
    try {
      return localStorage.getItem(APPROVAL_TOKEN_STORAGE_KEY);
    } catch {
      return null;
    }
  },
  set(token: string | null): void {
    try {
      if (token) localStorage.setItem(APPROVAL_TOKEN_STORAGE_KEY, token);
      else localStorage.removeItem(APPROVAL_TOKEN_STORAGE_KEY);
    } catch {
      /* ignore — localStorage may be unavailable */
    }
  },
};

/** True when the request failed because orchestration is disabled
 *  server-side (`TACK_ORCH_ENABLE` unset ⇒ every orch route 404s, TODO.md §0
 *  rule 8) — the default for every existing install. Mirrors
 *  `features/fleet/api.ts#isOrchDisabled` / `shared/agentActivity/api.ts`'s
 *  identically-named function (duplicated rather than imported — see those
 *  files' own notes on `architecture.test.ts`'s feature-isolation rule). */
export function isOrchDisabled(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

/** True when a decision was rejected because `X-Tack-Approval-Token` was
 *  missing, wrong, or `TACK_ORCH_APPROVAL_TOKEN` isn't configured on the
 *  server at all — never a transport/server error. */
export function isApprovalTokenRejected(err: unknown): boolean {
  return err instanceof ApiError && err.status === 403;
}

/** True when docket reports the approval was already decided (granted,
 *  denied, or expired) by someone/something else before this request
 *  reached it — a normal race for a fleet-wide inbox, not a bug. The caller
 *  should drop the stale row, not show a scary error. */
export function isApprovalAlreadyDecided(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409;
}

/** True when the token is unknown to Tack's own mirror, or docket itself no
 *  longer recognises it (e.g. an illegal decision on an already-resolved
 *  token) — same "drop the stale row" treatment as
 *  {@link isApprovalAlreadyDecided}. */
export function isApprovalGone(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

export const approvalsApi = {
  list: () => request<PendingApprovalListResponse>('/approvals'),
  decide: (token: string, action: ApprovalDecisionActionValue) => {
    const headers = new Headers();
    const approvalToken = approvalTokenStore.get();
    if (approvalToken) headers.set(APPROVAL_TOKEN_HEADER, approvalToken);
    return request<DecideApprovalResponse>(`/approvals/${encodeURIComponent(token)}`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ action }),
    });
  },
};
