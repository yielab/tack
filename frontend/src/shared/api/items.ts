import { request } from './client';
import type { Item, CreateItem, UpdateItem } from '../types';

export const items = {
  list: (projectId: string) =>
    request<Item[]>(`/projects/${projectId}/items`),

  get: (id: string) => request<Item>(`/items/${id}`),

  create: (projectId: string, data: CreateItem) =>
    request<{ id: string }>(`/projects/${projectId}/items`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  update: (id: string, data: UpdateItem) =>
    request<Item>(`/items/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),

  remove: (id: string) => request<void>(`/items/${id}`, { method: 'DELETE' }),
};
