import { type Component, createResource, createSignal, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import Drawer from '../../shared/ui/Drawer';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import ItemHeader from './ItemHeader';
import DetailsTab from './tabs/DetailsTab';
import ActivityTab from './tabs/ActivityTab';
import DependenciesTab from './tabs/DependenciesTab';
import FilesTab from './tabs/FilesTab';
import FieldsTab from './tabs/FieldsTab';

const TABS: TabItem[] = [
  { id: 'details', label: 'Details' },
  { id: 'activity', label: 'Activity' },
  { id: 'dependencies', label: 'Dependencies' },
  { id: 'files', label: 'Files' },
  { id: 'fields', label: 'Fields' },
];

/**
 * Item detail drawer (T-506). Mounted once at the app root; opens whenever the
 * `?item=<id>` query param is present (deep-linkable / shareable), fetches the
 * item, and exposes inline header editing + a tab bar. Built on the kit Drawer
 * (ESC + focus return).
 */
const ItemDetailDrawer: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const itemId = () => (searchParams.item as string | undefined) || undefined;

  const [item, { mutate, refetch }] = createResource(
    itemId,
    (id) => (id ? api.items.get(id) : null),
  );

  const [activeTab, setActiveTab] = createSignal('details');

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
      toast.error(err instanceof Error ? err.message : 'Failed to update item');
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
            <Tabs tabs={TABS} active={activeTab()} onChange={setActiveTab}>
              <Show when={activeTab() === 'details'}>
                <DetailsTab item={it()} onDescriptionChange={onDescriptionChange} />
              </Show>
              <Show when={activeTab() === 'activity'}>
                <ActivityTab itemId={it().id} />
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
