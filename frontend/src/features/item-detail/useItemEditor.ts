import { createEffect, createResource, type Accessor } from 'solid-js';
import { api } from '../../shared/api';
import { isItemVersionConflict } from '../../shared/api/items';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { useExecutionStore } from '../../shared/state/executionContext';
import { isTerminalStateString } from '../../shared/runWithAgent/shared';

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

  // The run moves the item (status, assignee, side details). The execution store
  // already re-reads the item's runs every few seconds, so the item is re-read when
  // one of them changes state, and no longer once they are all finished.
  const executions = useExecutionStore();
  let seenFor: string | undefined;
  let seenRuns: string | undefined;
  createEffect(() => {
    const id = itemId();
    if (id !== seenFor) {
      seenFor = id;
      seenRuns = undefined;
    }
    if (!id) return;
    const runs = executions.requestsForItem(id).filter((r) => r.summary);
    const signature = runs.map((r) => `${r.summary!.request_id}:${r.summary!.state}`).join('|');
    const before = seenRuns;
    seenRuns = signature;
    if (before === undefined || before === signature) return;
    // Opening a finished history is not a change; a run that started or moved is.
    if (before !== '' || runs.some((r) => !isTerminalStateString(r.summary!.state))) void refetch();
  });

  // Optimistic PATCH: apply locally, persist, then notify the host (or roll back).
  const patch = async (p: UpdateItem) => {
    const current = item();
    if (!current) return;
    mutate({ ...current, ...p } as Item);
    try {
      // Editing an item changes none of its children, and the answer to a PATCH does not count them.
      const updated = { ...(await api.items.update(current.id, p)), progress: current.progress };
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
