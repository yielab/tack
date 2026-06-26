import { request } from './client';
import type { Sprint, SprintStatus } from '../types';

interface SprintInput {
  name: string;
  goal?: string;
  start_date?: string;
  end_date?: string;
}

export const sprints = {
  list: (projectId: string) =>
    request<Sprint[]>(`/projects/${projectId}/sprints`),

  get: (id: string) => request<Sprint>(`/sprints/${id}`),

  create: (projectId: string, data: SprintInput) =>
    request<Sprint>(`/projects/${projectId}/sprints`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  // NOTE: the backend currently only exposes PATCH /sprints/{id}/status
  // (see router.rs). A full-sprint edit endpoint does not exist yet; this
  // preserves the existing page behavior until a full edit endpoint exists.
  update: (id: string, data: SprintInput) =>
    request<Sprint>(`/sprints/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),

  setStatus: (id: string, status: SprintStatus | string) =>
    request<Sprint>(`/sprints/${id}/status`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
};
