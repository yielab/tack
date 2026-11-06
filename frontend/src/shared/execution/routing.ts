// Who would do each open card of a project (`GET /api/projects/{id}/routing`): the one answer, worked
// out by the server with the choice a run takes. Flow and the Map draw it; they never decide it.

import type { components } from '../api/schema.gen';
import { request } from '../api/client';

export type Routed = components['schemas']['Routed'];

export const routingApi = {
  /** For each open card: the profile that would do it and where it runs, or why nobody would. */
  list: (projectId: string): Promise<Routed[]> => request<Routed[]>(`/projects/${projectId}/routing`),
};
