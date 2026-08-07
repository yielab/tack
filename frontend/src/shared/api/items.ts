import { ApiError, request, requestWithHeaders } from './client';
import type { Item, ItemPage, ItemDetail, CreateItem, UpdateItem } from '../types';

/** Page size used when walking the paginated item-list endpoint. */
const LIST_PAGE_SIZE = 200;
const itemEtags = new Map<string, string>();

/** A 412 is intentional concurrency feedback, not a transient network error. */
export function isItemVersionConflict(error: unknown): error is ApiError {
  return error instanceof ApiError && error.status === 412;
}

async function getItemWithEtag(id: string): Promise<Item> {
  const { data, headers } = await requestWithHeaders<ItemDetail>(`/items/${id}`);
  const etag = headers.get('ETag');
  if (!etag) throw new Error('The server did not provide an ETag for this item. Refresh and retry.');
  itemEtags.set(id, etag);
  return data.item;
}

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

  get: getItemWithEtag,

  create: (projectId: string, data: CreateItem) =>
    request<Item>(`/projects/${projectId}/items`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  update: async (id: string, data: UpdateItem) => {
    // List/board responses do not carry a per-item ETag. Fetching the
    // detail first means every browser mutation still participates in the
    // server's conditional-write contract.
    if (!itemEtags.has(id)) await getItemWithEtag(id);
    const etag = itemEtags.get(id)!;
    try {
      const { data: updated, headers } = await requestWithHeaders<Item>(`/items/${id}`, {
        method: 'PATCH',
        headers: { 'If-Match': etag },
        body: JSON.stringify(data),
      });
      const nextEtag = headers.get('ETag');
      if (!nextEtag) throw new Error('The server did not provide an ETag for the updated item. Refresh and retry.');
      itemEtags.set(id, nextEtag);
      return updated;
    } catch (error) {
      // Do not retry a stale edit invisibly. The caller refreshes its view,
      // then leaves the operator to review and deliberately retry their edit.
      if (isItemVersionConflict(error)) itemEtags.delete(id);
      throw error;
    }
  },

  remove: (id: string) => request<void>(`/items/${id}`, { method: 'DELETE' }),
};
