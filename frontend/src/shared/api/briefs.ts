import { request } from './client';
import type { components } from './schema.gen';

export type ItemBrief = components['schemas']['ItemBrief'];
export type UpsertItemBrief = components['schemas']['UpsertItemBrief'];
export type AcceptanceCriterion = components['schemas']['AcceptanceCriterion'];
export type BriefConstraint = components['schemas']['Constraint'];
export type BriefRisk = components['schemas']['Risk'];

/** What the run dialog holds, so the server can word the run's token limit: the agent chosen, and the limit typed or cleared. */
export interface AgentContextRun {
  harness?: string;
  tokens?: number;
  noLimit?: boolean;
}

export const briefs = {
  /** An item's brief. 404 (`ApiError.status`) when it has none yet. */
  get: (itemId: string) => request<ItemBrief>(`/items/${itemId}/brief`),

  /** What the agent will read for this item: the text the server composes for a run. */
  agentContext: (itemId: string, profileId?: string, run?: AgentContextRun) => {
    const params = new URLSearchParams();
    if (profileId) params.set('profile_id', profileId);
    if (run?.harness) params.set('harness', run.harness);
    if (run?.noLimit) params.set('no_limit', 'true');
    else if (run?.tokens !== undefined) params.set('tokens', String(run.tokens));
    const query = params.toString();
    return request<components['schemas']['AgentContextResponse']>(`/items/${itemId}/agent-context${query ? `?${query}` : ''}`);
  },

  /** Create or replace the brief in one write. */
  put: (itemId: string, data: UpsertItemBrief) =>
    request<ItemBrief>(`/items/${itemId}/brief`, {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
};
