// The one place the web derives anything from the shape of the tree. Whether an item holds work
// (`node`) and how far its work is (`progress`) come from the server; what a screen may work out
// itself is who an item sits under and what sits under a group, because every view already holds
// the project's items.

/** What the tree helpers need of an item. */
export interface TreeItem {
  id: string;
  parent_id?: string | null;
}

/**
 * The items an item sits under, outermost first: the project's top group, then the next, down to
 * its own parent. Empty for an item at the top. A parent the map lacks (a hidden item) ends the path
 * there, and a loop in the data, which the server refuses, cannot make it run on.
 */
export function ancestorsOf<T extends TreeItem>(item: TreeItem, byId: ReadonlyMap<string, T>): T[] {
  const path: T[] = [];
  const seen = new Set<string>([item.id]);
  let parentId = item.parent_id;
  while (parentId && !seen.has(parentId)) {
    const parent = byId.get(parentId);
    if (!parent) break;
    seen.add(parent.id);
    path.unshift(parent);
    parentId = parent.parent_id;
  }
  return path;
}

/**
 * Everything under a group at any depth, in the order `items` lists them. The group itself is not
 * in it.
 */
export function itemsUnder<T extends TreeItem>(group: TreeItem, items: readonly T[]): T[] {
  const childrenOf = new Map<string, T[]>();
  for (const item of items) {
    if (!item.parent_id) continue;
    const siblings = childrenOf.get(item.parent_id);
    if (siblings) siblings.push(item);
    else childrenOf.set(item.parent_id, [item]);
  }
  const under = new Set<string>();
  const pending = [group.id];
  while (pending.length > 0) {
    for (const child of childrenOf.get(pending.pop()!) ?? []) {
      if (child.id === group.id || under.has(child.id)) continue;
      under.add(child.id);
      pending.push(child.id);
    }
  }
  return items.filter((item) => under.has(item.id));
}
