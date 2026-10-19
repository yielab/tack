import { createResource, type Accessor } from 'solid-js';
import { api } from '../../shared/api';
import { isItemVersionConflict } from '../../shared/api/items';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';

/**
 * One item and what is loaded with it, for the drawer and the page alike: the
 * item, its dependency edges, and the optimistic edits that save them.
 */
export function useItemEditor(itemId: Accessor<string | undefined>) {
  const [item, { mutate, refetch }] = createResource(
    itemId,
    (id) => api.items.get(id),
  );
  const [dependencies, { refetch: refetchDependencies }] = createResource(
    itemId,
    (id) => api.dependencies.list(id),
  );

  // Optimistic PATCH: apply locally, persist, then notify the host (or roll back).
  const patch = async (p: UpdateItem) => {
    const current = item();
    if (!current) return;
    mutate({ ...current, ...p } as Item);
    try {
      const updated = await api.items.update(current.id, p);
      mutate(updated);
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT, { detail: updated }));
    } catch (err) {
      void refetch(); // reconcile to the server value
      toast.error(
        isItemVersionConflict(err)
          ? 'This item changed elsewhere. It has been refreshed; review it and retry your edit.'
          : err instanceof Error ? err.message : 'Failed to update item',
      );
    }
  };

  // Debounce description edits so we don't PATCH on every keystroke.
  let descTimer: ReturnType<typeof setTimeout> | undefined;
  const onDescriptionChange = (html: string) => {
    if (descTimer) clearTimeout(descTimer);
    descTimer = setTimeout(() => void patch({ description: html }), 600);
  };

  return { item, dependencies, refetchDependencies, patch, onDescriptionChange };
}

export type ItemEditor = ReturnType<typeof useItemEditor>;
