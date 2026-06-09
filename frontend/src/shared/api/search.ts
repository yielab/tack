import { request } from './client';
import type { Item } from '../types';

export const search = {
  global: (query: string, workspaceId?: string) => {
    const params = new URLSearchParams({ q: query });
    if (workspaceId) params.append('workspace_id', workspaceId);
    return request<Item[]>(`/search?${params}`);
  },

  inProject: (projectId: string, query: string) => {
    const params = new URLSearchParams({ q: query });
    return request<Item[]>(`/projects/${projectId}/search?${params}`);
  },
};
