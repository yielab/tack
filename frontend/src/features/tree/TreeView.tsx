import { type Component, createResource, createSignal, createMemo, For, Show } from 'solid-js';
import { useParams, useSearchParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState, Badge } from '../../shared/ui';
import { useVocab } from '../../shared/vocab/useVocab';
import { buildTree, type TreeNode } from './buildTree';

const TreeRow: Component<{
  node: TreeNode;
  expanded: Set<string>;
  toggle: (id: string) => void;
  open: (id: string) => void;
  typeLabel: (t: string) => { emoji: string; label: string };
}> = (props) => {
  const hasChildren = () => props.node.children.length > 0;
  const isOpen = () => props.expanded.has(props.node.id);
  const meta = () =>
    props.typeLabel(
      typeof props.node.item_type === 'string' ? props.node.item_type : 'task',
    );

  return (
    <>
      <div
        class="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-[var(--color-bg-hover)]"
        style={{ 'padding-left': `${props.node.depth * 1.5 + 0.5}rem` }}
      >
        <button
          class="h-5 w-5 flex-shrink-0 text-sm"
          style={{ color: 'var(--color-text-tertiary)', visibility: hasChildren() ? 'visible' : 'hidden' }}
          onClick={() => props.toggle(props.node.id)}
          aria-label={isOpen() ? 'Collapse' : 'Expand'}
        >
          {isOpen() ? '▾' : '▸'}
        </button>
        <span aria-hidden="true">{meta().emoji}</span>
        <button
          class="flex-1 truncate text-left text-sm hover:underline"
          style={{ color: 'var(--color-text-primary)' }}
          onClick={() => props.open(props.node.id)}
        >
          {props.node.title}
        </button>
        <Badge>{props.node.status}</Badge>
      </div>
      <Show when={hasChildren() && isOpen()}>
        <For each={props.node.children}>
          {(child) => (
            <TreeRow
              node={child}
              expanded={props.expanded}
              toggle={props.toggle}
              open={props.open}
              typeLabel={props.typeLabel}
            />
          )}
        </For>
      </Show>
    </>
  );
};

/** Hierarchical tree of a project's items (T-514). Click opens the detail drawer. */
const TreeView: Component = () => {
  const params = useParams();
  const [, setSearchParams] = useSearchParams();
  const projectId = params.id!;
  const { typeMap } = useVocab();

  const navigate = useNavigate();
  const [items] = createResource(() => api.items.tree(projectId));
  const roots = createMemo(() => buildTree(items() ?? []));

  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  const expandAll = () => setExpanded(new Set((items() ?? []).map((i) => i.id)));
  const collapseAll = () => setExpanded(new Set<string>());

  const typeLabel = (t: string) => {
    const m = typeMap()[t];
    return m ? { emoji: m.emoji, label: m.label } : { emoji: '📌', label: t };
  };
  const open = (id: string) => setSearchParams({ item: id });

  return (
    <div class="mx-auto max-w-4xl px-6 py-8">
      <div class="mb-4 flex items-center justify-between">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Tree
        </h1>
        <div class="flex gap-2">
          <Button size="sm" variant="secondary" onClick={expandAll}>Expand all</Button>
          <Button size="sm" variant="secondary" onClick={collapseAll}>Collapse all</Button>
        </div>
      </div>

      <Show
        when={roots().length > 0}
        fallback={
          <EmptyState
            icon="🌲"
            title="Nothing to show in tree view yet"
            description="Add items in Board or List, then come back here to see the hierarchy."
            action={
              <Button onClick={() => navigate(`/projects/${projectId}/board`)}>
                Go to Board
              </Button>
            }
          />
        }
      >
        <div class="rounded-lg border p-2" style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-base)' }}>
          <For each={roots()}>
            {(node) => (
              <TreeRow node={node} expanded={expanded()} toggle={toggle} open={open} typeLabel={typeLabel} />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default TreeView;
