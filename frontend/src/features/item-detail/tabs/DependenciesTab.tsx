import {
  type Component,
  createResource,
  createSignal,
  createMemo,
  For,
  Show,
} from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { api, ApiError } from '../../../shared/api';
import { Button, Select, EmptyState, Icons } from '../../../shared/ui';
import type { Item, Dependency, DependencyType } from '../../../shared/types';

export interface DependenciesTabProps {
  item: Item;
}

type Direction = 'blocks' | 'blockedBy';

interface Link {
  dep: Dependency;
  otherId: string;
}

/** Classify a dependency relative to `itemId`. */
function classify(dep: Dependency, itemId: string): { dir: Direction | 'related'; otherId: string } {
  const isSource = dep.source_item_id === itemId;
  const other = isSource ? dep.target_item_id : dep.source_item_id;
  if (dep.dependency_type === 'blocks') {
    return { dir: isSource ? 'blocks' : 'blockedBy', otherId: other };
  }
  if (dep.dependency_type === 'is_blocked_by') {
    return { dir: isSource ? 'blockedBy' : 'blocks', otherId: other };
  }
  return { dir: 'related', otherId: other };
}

/** Dependencies tab: blocks / blocked-by lists, an item picker to add,
 * cycle-error surfaced inline, links open the target item's drawer. */
const DependenciesTab: Component<DependenciesTabProps> = (props) => {
  const [, setSearchParams] = useSearchParams();

  const [deps, { refetch }] = createResource(
    () => props.item.id,
    (id) => api.dependencies.list(id),
  );
  const [projectItems] = createResource(
    () => props.item.project_id,
    (pid) => api.items.list(pid),
  );

  const titleOf = (id: string) =>
    projectItems()?.find((i) => i.id === id)?.title ?? id;

  const blocks = createMemo<Link[]>(() =>
    (deps() ?? [])
      .map((dep) => ({ dep, c: classify(dep, props.item.id) }))
      .filter((x) => x.c.dir === 'blocks')
      .map((x) => ({ dep: x.dep, otherId: x.c.otherId })),
  );
  const blockedBy = createMemo<Link[]>(() =>
    (deps() ?? [])
      .map((dep) => ({ dep, c: classify(dep, props.item.id) }))
      .filter((x) => x.c.dir === 'blockedBy')
      .map((x) => ({ dep: x.dep, otherId: x.c.otherId })),
  );

  const candidates = createMemo(() =>
    (projectItems() ?? []).filter((i) => i.id !== props.item.id),
  );

  const [target, setTarget] = createSignal('');
  const [direction, setDirection] = createSignal<Direction>('blocks');
  const [error, setError] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const add = async () => {
    const t = target();
    if (!t) return;
    setError('');
    setBusy(true);
    const dependency_type: DependencyType =
      direction() === 'blocks' ? 'blocks' : 'is_blocked_by';
    try {
      await api.dependencies.create(props.item.id, { target_item_id: t, dependency_type });
      setTarget('');
      await refetch();
    } catch (err) {
      // Server rejects cycles (and other invalids) with a 400 — show it inline.
      setError(err instanceof ApiError ? err.message : 'Failed to add dependency');
    } finally {
      setBusy(false);
    }
  };

  const remove = async (depId: string) => {
    await api.dependencies.remove(props.item.id, depId);
    await refetch();
  };

  const openItem = (id: string) => setSearchParams({ item: id });

  const linkList = (links: Link[]) => (
    <ul class="space-y-2">
      <For each={links}>
        {(link) => (
          <li class="flex items-center justify-between gap-2 rounded-[20px] py-2.5 pl-4 pr-2.5 text-sm" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
            <button
              type="button"
              class="min-w-0 text-left font-semibold hover:underline"
              style={{ color: 'var(--color-accent-ink)' }}
              onClick={() => openItem(link.otherId)}
            >
              {titleOf(link.otherId)}
            </button>
            <button
              type="button"
              aria-label="Remove dependency"
              onClick={() => void remove(link.dep.id)}
              style={{ color: 'var(--color-text-tertiary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
              class="grid h-7 w-7 flex-none place-items-center rounded-full hover:bg-[var(--color-bg-subtle)] focus:outline-none focus-visible:ring-2"
            >
              <Icons.IconClose size={12} stroke-width={2.2} />
            </button>
          </li>
        )}
      </For>
    </ul>
  );

  return (
    <div class="space-y-4">
      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Blocks
        </h3>
        <Show when={blocks().length > 0} fallback={<EmptyHint text="This item doesn't block anything." />}>
          {linkList(blocks())}
        </Show>
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Blocked by
        </h3>
        <Show when={blockedBy().length > 0} fallback={<EmptyHint text="Nothing is blocking this item." />}>
          {linkList(blockedBy())}
        </Show>
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Add dependency
        </h3>
        <Show when={candidates().length > 0} fallback={<EmptyState icon={<Icons.IconLink size={28} />} title="No other items to link" />}>
          <div class="flex items-end gap-2">
            <Select
              class="w-32"
              label="Direction"
              value={direction()}
              onChange={(e) => setDirection(e.currentTarget.value as Direction)}
              options={[
                { value: 'blocks', label: 'Blocks' },
                { value: 'blockedBy', label: 'Blocked by' },
              ]}
            />
            <Select
              class="flex-1"
              label="Item"
              value={target()}
              onChange={(e) => setTarget(e.currentTarget.value)}
            >
              <option value="">— Select an item —</option>
              <For each={candidates()}>
                {(i) => <option value={i.id}>{i.title}</option>}
              </For>
            </Select>
            <Button onClick={() => void add()} loading={busy()} disabled={busy() || !target()}>
              Add
            </Button>
          </div>
        </Show>
        <Show when={error()}>
          <p class="rounded-[20px] px-4 py-2.5 text-sm font-medium" style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}>
            {error()}
          </p>
        </Show>
      </section>
    </div>
  );
};

const EmptyHint: Component<{ text: string }> = (props) => (
  <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
    {props.text}
  </p>
);

export default DependenciesTab;
