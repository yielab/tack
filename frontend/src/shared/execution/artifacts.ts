// Wire-format boundary for verified artifact download (TODO.md III-F4):
// `GET /executions/{request_id}/attempts/{attempt_number}/artifacts/
// {artifact_id}/content` — mounted in production by the Wave 5 integrator
// (III-F6/F6a) at `crates/tack-api/src/handlers/runner_protocol/
// artifact_download.rs`, proven through the real router by
// `crates/tack-api/tests/f6a_artifact_wiring_test.rs`.
//
// **There is no artifact *discovery* endpoint anywhere in this codebase.**
// `execution_artifacts` has exactly one read path
// (`get_execution_artifact_by_attempt_number`, a single-row lookup by a
// caller-supplied `artifact_id`) — no `GET .../artifacts` list route exists
// (confirmed: neither `crates/tack-api/src/handlers/executions.rs` nor
// `runner_protocol.rs` defines one, and III-F2's own handoff never claims
// one). An `artifact_id` is runner-generated and opaque; today it can only
// reach an operator out-of-band (e.g. read off a runner's own logs, or a
// future normalized-timeline event that happens to carry one — see
// `ArtifactDownloadPanel.tsx`'s own doc comment for the best-effort
// convention this UI applies for that case). See this card's handoff,
// "Schema/API/contract change requested from another owner", for the
// concrete list-endpoint request this gap produces — the same shape of gap
// III-F1's decisions surface has (see `decisions.ts`'s header comment).
//
// Two HTTP outcomes distinguish two genuinely different server states, per
// this card's acceptance bar ("artifact failure visible") and III.2 rule 7
// ("unmeasured is nullable" applied to presence, not just numbers):
//   - `404 not_found` — no artifact manifest exists under this id at all.
//   - `409 conflict` — the manifest exists, but its content has not been
//     verified (streamed + checksummed) yet; genuinely different from
//     "gone", per `artifact_download.rs`'s own doc comment.

import { ApiError, apiUrl, requestBlob } from '../api/client';

/** Path segment builder shared by every artifact-content operation below —
 *  kept in one place so the URL shape can never drift between the fetch
 *  path and the debug/manual-open path. */
function artifactContentPath(requestId: string, attemptNumber: number, artifactId: string): string {
  return `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(
    String(attemptNumber),
  )}/artifacts/${encodeURIComponent(artifactId)}/content`;
}

export const artifactsApi = {
  /** Fetches the verified artifact content as a `Blob`, carrying the
   *  operator's bearer token (unlike a plain `<a href download>`, which
   *  cannot attach an `Authorization` header — see this card's handoff for
   *  why a fetch+blob download was chosen over the simpler anchor-tag
   *  pattern `FilesTab.tsx` uses for attachments). Throws `ApiError` with
   *  the real status on any non-2xx response — callers should check
   *  {@link isArtifactNotFound}/{@link isArtifactContentNotVerified} to
   *  render the two distinct failure states named above. */
  download: (requestId: string, attemptNumber: number, artifactId: string) =>
    requestBlob(artifactContentPath(requestId, attemptNumber, artifactId)),
  /** The raw API URL — exposed for tests and for a future caller that wants
   *  to open the resource directly (e.g. a same-tab preview when no bearer
   *  token is configured on this deployment). Not used for the primary
   *  download path above, which needs the auth header a plain URL can't
   *  carry. */
  contentUrl: (requestId: string, attemptNumber: number, artifactId: string) =>
    apiUrl(artifactContentPath(requestId, attemptNumber, artifactId)),
};

/** No manifest exists under this `(request_id, attempt_number, artifact_id)`
 *  triple — a genuinely missing resource. */
export function isArtifactNotFound(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

/** The manifest exists, but its content has not been verified (streamed +
 *  checksummed) yet — distinct from "gone"; a caller should offer to retry
 *  rather than treat this as permanent. */
export function isArtifactContentNotVerified(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409;
}
