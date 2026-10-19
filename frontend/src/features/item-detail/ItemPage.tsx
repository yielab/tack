import { type Component, Show } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import { Button, Icons } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import ItemBody, { tabFromSearchParam } from './ItemBody';
import { useItemEditor } from './useItemEditor';

/**
 * An item on a page of its own, `/projects/:id/items/:itemId`: the same body
 * the drawer shows, with a side column for the properties. `?tab=` picks the tab
 * and follows it, so the address is always a link to what is on screen.
 */
const ItemPage: Component = () => {
  const params = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const editor = useItemEditor(() => params.itemId);
  const item = editor.item;

  const copyLink = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      toast.success('Link copied');
    } catch {
      toast.error('Could not copy the link');
    }
  };

  return (
    <Show
      when={item()}
      fallback={
        <p class="py-8 text-center text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
          {item.loading ? 'Loading…' : 'Item not found.'}
        </p>
      }
    >
      {(it) => (
        <ItemBody
          item={it()}
          editor={editor}
          layout="page"
          tab={tabFromSearchParam(searchParams.tab)}
          onTabChange={(id) => setSearchParams({ tab: id === 'details' ? undefined : id }, { replace: true })}
          headerActions={
            <Button size="sm" variant="secondary" onClick={() => void copyLink()}>
              <span class="inline-flex items-center gap-1.5">
                <Icons.IconLink size={14} /> Copy link
              </span>
            </Button>
          }
        />
      )}
    </Show>
  );
};

export default ItemPage;
