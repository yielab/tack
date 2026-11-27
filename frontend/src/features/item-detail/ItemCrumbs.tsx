import { type Component, type JSX } from 'solid-js';
import ItemPath from '../../shared/ui/ItemPath';
import type { Item } from '../../shared/types';

/** Where the item sits (the groups above it as links) with the item's quiet actions at the end of the row. */
const ItemCrumbs: Component<{
  item: Item;
  byId: ReadonlyMap<string, Item>;
  /** Copy link, Open as a page and the "⋯" menu. */
  actions?: JSX.Element;
}> = (props) => (
  <div class="flex min-w-0 items-center gap-2">
    <div class="min-w-0 flex-1">
      <ItemPath item={props.item} byId={props.byId} projectId={props.item.project_id} />
    </div>
    <div class="flex flex-none items-center gap-1">{props.actions}</div>
  </div>
);

export default ItemCrumbs;
