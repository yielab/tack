// Who would do each open card of a project (`GET /api/projects/{id}/routing`): the one answer, worked
// out by the server with the choice a run takes. Flow and the Map draw it; they never decide it.

import type { components } from '../api/schema.gen';
import { request } from '../api/client';

export type Routed = components['schemas']['Routed'];

export const routingApi = {
  /** For each open card: the profile that would do it and where it runs, or why nobody would. */
  list: (projectId: string): Promise<Routed[]> => request<Routed[]>(`/projects/${projectId}/routing`),
};

/** How far a parent's children are, as a card and the item page say it: "1 of 2 children done · saas-build, claude-code".
 *  Null for an item without children. */
export function childrenText(progress: components['schemas']['ChildrenProgress'] | null | undefined): string | null {
  if (!progress || progress.total <= 0) return null;
  const done = `${progress.done} of ${progress.total} ${progress.total === 1 ? 'child' : 'children'} done`;
  return progress.teams.length > 0 ? `${done} · ${progress.teams.join(', ')}` : done;
}
