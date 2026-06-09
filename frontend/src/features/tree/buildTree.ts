import type { Item } from '../../shared/types';

export interface TreeNode extends Item {
  children: TreeNode[];
  depth: number;
}

/**
 * Build a nested hierarchy from the flat, ordered item list returned by
 * `GET /projects/{id}/items/tree`. Items whose parent is missing (or null)
 * become roots. Insertion order (sort_order from the API) is preserved.
 */
export function buildTree(items: Item[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  for (const item of items) byId.set(item.id, { ...item, children: [], depth: 0 });

  const roots: TreeNode[] = [];
  for (const node of byId.values()) {
    const parent = node.parent_id ? byId.get(node.parent_id) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }

  // Stamp depth for indentation.
  const stamp = (node: TreeNode, depth: number) => {
    node.depth = depth;
    for (const child of node.children) stamp(child, depth + 1);
  };
  for (const root of roots) stamp(root, 0);

  return roots;
}
