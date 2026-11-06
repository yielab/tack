import { request } from './client';
import type { ItemTypeDef, ItemTypeUse } from '../types';
import type { components } from './schema.gen';

type CreateItemType = components['schemas']['CreateItemType'];
type UpdateItemType = components['schemas']['UpdateItemType'];

export const itemTypes = {
  /** The project's types in display order, each with how many items use it. */
  list: (projectId: string) => request<ItemTypeUse[]>(`/projects/${projectId}/types`),

  create: (projectId: string, data: CreateItemType) =>
    request<ItemTypeDef>(`/projects/${projectId}/types`, { method: 'POST', body: JSON.stringify(data) }),

  update: (projectId: string, key: string, data: UpdateItemType) =>
    request<ItemTypeDef>(`/projects/${projectId}/types/${encodeURIComponent(key)}`, { method: 'PATCH', body: JSON.stringify(data) }),

  /** Removes a type; `mergeInto` is the type its items move to, required while items use it. */
  remove: (projectId: string, key: string, mergeInto?: string) =>
    request<void>(
      `/projects/${projectId}/types/${encodeURIComponent(key)}${mergeInto ? `?merge_into=${encodeURIComponent(mergeInto)}` : ''}`,
      { method: 'DELETE' },
    ),
};
