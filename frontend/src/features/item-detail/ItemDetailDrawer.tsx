import { type Component, createResource, createSignal, createEffect, untrack, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import Drawer from '../../shared/ui/Drawer';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import { api } from '../../shared/api';
import { isItemVersionConflict } from '../../shared/api/items';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import ExecutionTimeline from '../../shared/runWithAgent/ExecutionTimeline';
import ItemHeader from './ItemHeader';
import DetailsTab from './tabs/DetailsTab';
import ActivityTab from './tabs/ActivityTab';
import DependenciesTab from './tabs/DependenciesTab';
import FilesTab from './tabs/FilesTab';
import FieldsTab from './tabs/FieldsTab';

const BASE_TABS: TabItem[] = [
  { id: 'details', label: 'Details' },
  { id: 'activity', label: 'Activity' },
  // "Execution" — the neutral execution domain (`ExecutionRequest`/
  // `ExecutionAttempt` via `tack-runner`). Always present — an item with
  // zero execution requests is still a real, honest state worth a visible
  // empty tab (`ExecutionTimeline`'s own `EmptyState`), not a hidden one.
  { id: 'execution', label: 'Execution' },
  { id: 'dependencies', label: 'Dependencies' },
  { id: 'files', label: 'Files' },
  { id: 'fields', label: 'Fields' },
];

/**
 * Item detail drawer. Mounted once at the app root; opens whenever the
 * `?item=<id>` query param is present (deep-linkable / shareable), fetches the
 * item, and exposes inline header editing + a tab bar. Built on the kit Drawer
 * (ESC + focus return).
 */
/** Every tab id this drawer can land on directly. */
const DEEP_LINKABLE_TAB_IDS = new Set(BASE_TABS.map((t) => t.id));

function tabFromSearchParam(value: string | string[] | undefined): string {
  return typeof value === 'string' && DEEP_LINKABLE_TAB_IDS.has(value) ? value : 'details';
}

const ItemDetailDrawer: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const itemId = () => (searchParams.item as string | undefined) || undefined;

  const [item, { mutate, refetch }] = createResource(
    itemId,
    (id) => (id ? api.items.get(id) : null),
  );

  // Defaults to `details`, but a `?tab=` alongside `?item=` (the attempt-state
  // chip on a Board card, or `onCreated` switching this same drawer to
  // `execution` after a fresh run) opens straight to that tab instead.
  //
  // Tracks `itemId()` only, a one-shot-per-open pattern — NOT
  // `searchParams.tab` itself, and the param is read `untrack`ed and
  // immediately cleared once applied. Every
  // other "open an item" call site across the app (Timeline, Calendar,
  // List, `Board.tsx`'s own card-body click, `DependenciesTab`) calls
  // `setSearchParams({ item: id })` with no `tab` key, and `setSearchParams`
  // merges rather than replaces — so without the clear, a `tab=execution`
  // left over from one chip click would silently redirect every
  // subsequently opened item to the Execution tab. The one known gap this
  // leaves: re-clicking the chip for the item ALREADY open (itemId
  // unchanged) does not re-apply the tab — a narrower, and much less
  // surprising, edge case than the leak this avoids.
  const [activeTab, setActiveTab] = createSignal(tabFromSearchParam(untrack(() => searchParams.tab)));
  createEffect(() => {
    const id = itemId();
    if (!id) return;
    const requestedTab = untrack(() => searchParams.tab);
    setActiveTab(tabFromSearchParam(requestedTab));
    if (requestedTab !== undefined) setSearchParams({ tab: undefined }, { replace: true });
  });

  const close = () => setSearchParams({ item: undefined });

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

  return (
    <Drawer isOpen={!!itemId()} onClose={close} title="Item details" width="md">
      <Show
        when={item()}
        fallback={
          <p class="py-8 text-center text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
            {item.loading ? 'Loading…' : 'Item not found.'}
          </p>
        }
      >
        {(it) => (
          <div class="space-y-6">
            <ItemHeader item={it()} onPatch={patch} />

            {/* Run with agent — the neutral execution surface
                (`ExecutionRequest`/`ExecutionAttempt` via `tack-runner`). A
                successful run switches straight to the "Execution" tab so
                the request that just appeared is immediately visible,
                without a page navigation. */}
            <div
              class="flex flex-wrap items-center gap-3 rounded-lg border p-3"
              style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
            >
              <RunWithAgentButton
                itemId={it().id}
                itemTitle={it().title}
                projectId={it().project_id}
                onCreated={() => setActiveTab('execution')}
              />
            </div>

            <Tabs tabs={BASE_TABS} active={activeTab()} onChange={setActiveTab}>
              <Show when={activeTab() === 'details'}>
                <DetailsTab item={it()} onDescriptionChange={onDescriptionChange} />
              </Show>
              <Show when={activeTab() === 'activity'}>
                <ActivityTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'execution'}>
                <ExecutionTimeline itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'dependencies'}>
                <DependenciesTab item={it()} />
              </Show>
              <Show when={activeTab() === 'files'}>
                <FilesTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'fields'}>
                <FieldsTab item={it()} />
              </Show>
            </Tabs>
          </div>
        )}
      </Show>
    </Drawer>
  );
};

export default ItemDetailDrawer;

/** Helper for host views: the query patch that opens the drawer for an item. */
export const openItemParam = (itemId: string) => ({ item: itemId });
