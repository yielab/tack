import { createMemo, createResource } from 'solid-js';
import { api } from '../../shared/api';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import type { Item } from '../../shared/types';
import type { ItemWithBrief } from '../../shared/types';

/** An item of the project's list: as the board served it, or one made on the screen before the board answered. */
export type ListedItem = Item | ItemWithBrief;

/**
 * The project's items, which name an item's ancestors and carry its `brief_summary`. A screen under the
 * board's tabs already has them; the drawer sits outside those tabs, so it reads them itself.
 */
export function useProjectItemList(item: () => Item) {
  const shared = (() => {
    try {
      return useProjectItems().items;
    } catch {
      return null;
    }
  })();
  const [own] = createResource(
    () => (shared ? null : `${item().project_id}:${item().updated_at}`),
    () => api.items.list(item().project_id).catch(() => [] as Item[]),
  );
  const items = (): ListedItem[] | undefined => (shared ? shared() : own());
  const byId = createMemo(() => new Map((items() ?? []).map((i) => [i.id, i] as const)));
  return { items, byId };
}
