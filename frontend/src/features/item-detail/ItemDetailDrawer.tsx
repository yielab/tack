import { type Component, createSignal, createEffect, untrack, Show } from 'solid-js';
import { useSearchParams, useNavigate } from '@solidjs/router';
import Drawer from '../../shared/ui/Drawer';
import Button from '../../shared/ui/Button';
import ItemBody, { tabFromSearchParam } from './ItemBody';
import { useItemEditor } from './useItemEditor';

/**
 * Item detail drawer. Mounted once at the app root; opens whenever the
 * `?item=<id>` query param is present (deep-linkable / shareable), fetches the
 * item, and shows the item screen's body in one column. "Expand" moves it to
 * the item's page. Built on the kit Drawer (ESC + focus return).
 */
const ItemDetailDrawer: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const itemId = () => (searchParams.item as string | undefined) || undefined;

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
  // `?run=` is read and cleared the same way: it names the run the Execution tab opens on.
  const [activeRun, setActiveRun] = createSignal<string | undefined>(undefined);
  createEffect(() => {
    const id = itemId();
    if (!id) return;
    const requestedTab = untrack(() => searchParams.tab);
    const requestedRun = untrack(() => searchParams.run);
    setActiveTab(tabFromSearchParam(requestedTab));
    setActiveRun(typeof requestedRun === 'string' ? requestedRun : undefined);
    if (requestedTab !== undefined || requestedRun !== undefined) setSearchParams({ tab: undefined, run: undefined }, { replace: true });
  });

  const close = () => setSearchParams({ item: undefined });

  const editor = useItemEditor(itemId);
  const item = editor.item;

  const navigate = useNavigate();
  const expand = () => {
    const it = item();
    if (!it) return;
    const run = activeTab() === 'execution' && activeRun() ? `&run=${encodeURIComponent(activeRun()!)}` : '';
    const tab = activeTab() === 'details' ? '' : `?tab=${activeTab()}${run}`;
    navigate(`/projects/${it.project_id}/items/${it.id}${tab}`);
  };

  return (
    <Drawer
      isOpen={!!itemId()}
      onClose={close}
      title="Item details"
      width="lg"
      actions={
        <Show when={item()}>
          <Button size="sm" variant="secondary" onClick={expand}>
            Expand
          </Button>
        </Show>
      }
    >
      <Show
        when={item()}
        fallback={
          <p class="py-8 text-center text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
            {item.loading ? 'Loading…' : 'Item not found.'}
          </p>
        }
      >
        {(it) => (
          <ItemBody item={it()} editor={editor} layout="drawer" tab={activeTab()} onTabChange={(id) => { setActiveTab(id); setActiveRun(undefined); }} run={activeRun()} onRunChange={setActiveRun} />
        )}
      </Show>
    </Drawer>
  );
};

export default ItemDetailDrawer;
