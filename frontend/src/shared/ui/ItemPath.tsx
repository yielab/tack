import { type Component, For, Show } from 'solid-js';
import { A } from '@solidjs/router';
import clsx from 'clsx';
import { ancestorsOf, type TreeItem } from '../tree';

/** The item page's address, the one place a path link is built. */
export const itemHref = (projectId: string, itemId: string): string => `/projects/${projectId}/items/${itemId}`;

export interface ItemPathProps {
  /** The item whose ancestors are shown. */
  item: TreeItem;
  /** Every item the view holds, by id: the path is read from these. */
  byId: ReadonlyMap<string, TreeItem & { title: string }>;
  projectId: string;
  class?: string;
}

/**
 * The items an item sits under as links, outermost first ("Checkout › Payments"). It draws nothing
 * for an item at the top. It is not the app's navigation: `Breadcrumb` is, and stays apart.
 */
const ItemPath: Component<ItemPathProps> = (props) => {
  const path = () => ancestorsOf(props.item, props.byId);
  return (
    <Show when={path().length > 0}>
      <nav aria-label="Path" data-testid="item-path" class={clsx('min-w-0 text-xs', props.class)}>
        <ol class="m-0 flex min-w-0 list-none flex-wrap items-center gap-x-1 p-0">
          <For each={path()}>
            {(ancestor, i) => (
              <li class="flex min-w-0 items-center gap-1">
                <Show when={i() > 0}>
                  <span aria-hidden="true" style={{ color: 'var(--color-text-tertiary)' }}>›</span>
                </Show>
                <A
                  href={itemHref(props.projectId, ancestor.id)}
                  title={ancestor.title}
                  onClick={(e) => e.stopPropagation()}
                  class="truncate font-medium hover:underline focus:outline-none focus-visible:ring-2"
                  style={{ 'max-width': '16ch', color: 'var(--color-text-secondary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                >
                  {ancestor.title}
                </A>
              </li>
            )}
          </For>
        </ol>
      </nav>
    </Show>
  );
};

export default ItemPath;
