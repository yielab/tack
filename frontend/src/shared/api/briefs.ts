import { request } from './client';
import type { components } from './schema.gen';

export type ItemBrief = components['schemas']['ItemBrief'];
export type UpsertItemBrief = components['schemas']['UpsertItemBrief'];
export type AcceptanceCriterion = components['schemas']['AcceptanceCriterion'];
export type BriefConstraint = components['schemas']['Constraint'];
export type BriefRisk = components['schemas']['Risk'];

export const briefs = {
  /** An item's brief. 404 (`ApiError.status`) when it has none yet. */
  get: (itemId: string) => request<ItemBrief>(`/items/${itemId}/brief`),

  /** What the agent will read for this item: the text the server composes for a run. */
  agentContext: (itemId: string) => request<components['schemas']['AgentContextResponse']>(`/items/${itemId}/agent-context`),

  /** Create or replace the brief in one write. */
  put: (itemId: string, data: UpsertItemBrief) =>
    request<ItemBrief>(`/items/${itemId}/brief`, {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
};
