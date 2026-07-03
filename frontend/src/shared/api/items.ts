import { request } from './client';
import type { Item, ItemPage, ItemDetail, CreateItem, UpdateItem } from '../types';

/** Page size used when walking the paginated item-list endpoint. */
const LIST_PAGE_SIZE = 200;

export const items = {
  /**
   * Fetch every item in a project. The endpoint is paginated
   * (`{ data, total, page, per_page }`); we walk pages until we've collected
   * `total` items so projects with >100 items are not silently truncated.
   * Returns a flat `Item[]` so callers stay unchanged.
   */
  list: async (projectId: string): Promise<Item[]> => {
    const all: Item[] = [];
    for (let page = 1; ; page += 1) {
      const res = await request<ItemPage>(
        `/projects/${projectId}/items?page=${page}&per_page=${LIST_PAGE_SIZE}`,
      );
      const batch = res.data ?? [];
      all.push(...batch);
      const total = res.total ?? all.length;
      if (batch.length === 0 || all.length >= total) break;
    }
    return all;
  },

  get: (id: string) =>
    request<ItemDetail>(`/items/${id}`).then((r) => r.item),

  create: (projectId: string, data: CreateItem) =>
    request<Item>(`/projects/${projectId}/items`, {
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
