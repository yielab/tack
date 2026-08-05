// Shared data hook backing the Board/List/Table badge chips. One bulk fetch
// per project (`GET /projects/{id}/agent-activity`, see `./api.ts`) instead
// of N per-item calls — the three views each mount this hook independently,
// so each gets its own resource/cache, but all three read the same wire
// boundary and the same state-derivation function, which is the whole point
// of "one shared AgentStateChip ... no per-view reimplementation" (TODO.md
// card B5).

import { createResource, createMemo, type Accessor } from 'solid-js';
import { agentActivityApi, type AgentBadgeRow } from './api';
import { deriveAgentChipState } from './format';
import type { AgentChipState } from '../ui/AgentStateChip';

export interface AgentBadgeInfo {
  state: AgentChipState;
  /** Raw wire status, kept for a tooltip (e.g. distinguishing `blocked` from
   *  a plain `failed` even though both render the `failed` chip). */
  remoteStatus: string;
  attempt: number;
}

export interface AgentActivityMap {
  /** `undefined` = no agent activity for this item (render no chip). */
  stateFor: (itemId: string) => AgentBadgeInfo | undefined;
  /**
   * Re-fetch the bulk badge rows. Card B4 (Wave 2, realtime broadcast, task
   * 34.5): callers wire this to `BoardEvent::AgentRunUpdated`/
   * `ApprovalPending` arriving over `shared/realtime/boardSocket.ts` so a
   * badge updates without a page refresh, the same way `useProjectItems`'s
   * `refetch` already does for the item list itself.
   */
  refetch: () => void;
  /**
   * `true` once this same bulk fetch has resolved successfully — i.e.
   * orchestration is enabled on this server and the project is reachable.
   * Added by card C4 (Wave 3, dispatch UI + security gating) as the cheap,
   * already-in-flight signal for "should dispatch controls render at all,"
   * reusing this hook's existing request instead of adding a second probe.
   * `false` while loading and on ANY failure, not just a 404 — the same
   * conservative "if we can't positively confirm it's on, don't show a
   * privileged control" posture this card's brief calls for (TODO.md §0 rule
   * 8: off by default). `stateFor`'s own fail-open behavior is unaffected —
   * a missing badge is still never worth degrading the view over, but
   * showing a dispatch button that's about to 404 is a different, worse
   * failure mode than a missing chip.
   */
  orchAvailable: () => boolean;
}

/**
 * Fetches the project's bulk agent-activity rows and exposes an
 * item-id → badge-info lookup. Fails open: a 404 (orchestration disabled,
 * the default for every existing install — TODO.md §0 rule 8) or any other
 * request error is treated as "no rows", not surfaced as an error — a
 * badge's absence is never worth degrading Board/List/Table over.
 */
export function useAgentActivityMap(projectId: Accessor<string | undefined>): AgentActivityMap {
  const [resource, { refetch }] = createResource(projectId, (id) =>
    agentActivityApi.listForProject(id)
  );

  const byItem = createMemo(() => {
    const map = new Map<string, AgentBadgeRow>();
    if (resource.error) return map; // fail open — see doc comment above
    for (const row of resource()?.rows ?? []) map.set(row.item_id, row);
    return map;
  });

  return {
    stateFor: (itemId: string) => {
      const row = byItem().get(itemId);
      if (!row) return undefined;
      return {
        state: deriveAgentChipState(row.remote_status),
        remoteStatus: row.remote_status,
        attempt: row.attempt,
      };
    },
    refetch: () => void refetch(),
    orchAvailable: () => !resource.loading && resource.error === undefined,
  };
}
