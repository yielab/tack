import { type Component, For, Show, createEffect, createMemo, createResource, createSignal, onCleanup, onMount } from 'solid-js';
import { A } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, Fold, RunState, type RunStateName } from '../../shared/ui';
import { itemHref } from '../../shared/ui/ItemPath';
import ProgressBar, { needsYouText, progressText } from '../../shared/ui/ProgressBar';
import { IconPlus } from '../../shared/ui/icons';
import { toast } from '../../shared/ui/toast';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import { cardRun, newestRunByItem, stateRank, type CardRun } from '../../shared/runWithAgent/cardRun';
import { useExecutionStore } from '../../shared/state/executionContext';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { useProject } from '../../shared/state/projectContext';
import type { Item } from '../../shared/types';

export interface GroupProgressProps {
  /** The group, as the server read it: its `progress` is drawn as given. */
  item: Item;
  /** Told after an item is added under the group, so the page reads the group's progress again. */
  onChanged?: () => void;
}

interface Section {
  /** The run state the work is in, or none for work with no run. */
  state: RunStateName | null;
  items: Item[];
}

/**
 * What a group's page leads with: how far its work is, the work itself under the Work states the List
 * uses (what needs a person first), and the two things to do with a group, Add item and Plan with agent.
 * The work comes from the server's `under` read, so a group never works out what lies under it.
 */
const GroupProgress: Component<GroupProgressProps> = (props) => {
  const { vocabulary } = useProject();
  const executions = useExecutionStore();
  const pointsWord = () => vocabulary()?.story_points?.toLowerCase();

  // The work is read again whenever the group's progress changes or any item is edited.
  const [stamp, setStamp] = createSignal(0);
  onMount(() => {
    const again = () => setStamp((n) => n + 1);
    window.addEventListener(ITEM_UPDATED_EVENT, again);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, again));
  });
  const progress = () => props.item.progress;
  const [work, { refetch }] = createResource(
    () => `${props.item.project_id}:${props.item.id}:${progress()?.total}:${progress()?.done}:${progress()?.needs_you}:${stamp()}`,
    () => api.items.list(props.item.project_id, { under: props.item.id, node: 'work' }).catch(() => [] as Item[]),
  );
  const rows = (): Item[] => (work.error ? [] : (work.latest ?? []));

  // The newest run of each item, from the store the Board and the List read.
  const watched = new Map<string, () => void>();
  createEffect(() => {
    const ids = new Set(rows().map((i) => i.id));
    for (const [id, unwatch] of watched) {
      if (!ids.has(id)) {
        unwatch();
        watched.delete(id);
      }
    }
    for (const id of ids) if (!watched.has(id)) watched.set(id, executions.watchItem(id));
  });
  onCleanup(() => {
    for (const unwatch of watched.values()) unwatch();
    watched.clear();
  });
  const newestRuns = createMemo(() => {
    const summaries = [];
    for (const record of executions.requests().values()) if (record.summary) summaries.push(record.summary);
    return newestRunByItem(summaries);
  });
  const runOf = (item: Item): CardRun | undefined => cardRun(newestRuns().get(item.id), item.needs_review === true);

  const sections = createMemo((): Section[] => {
    const by = new Map<RunStateName | null, Item[]>();
    for (const item of rows()) {
      const state = runOf(item)?.state ?? null;
      by.set(state, [...(by.get(state) ?? []), item]);
    }
    return [...by.entries()]
      .map(([state, items]) => ({ state, items }))
      .sort((a, b) => (a.state ? stateRank(a.state) : 100) - (b.state ? stateRank(b.state) : 100));
  });

  const hasRun = () => executions.requestsForItem(props.item.id).length > 0;
  const teams = () => progress()?.teams ?? [];

  const [adding, setAdding] = createSignal(false);
  const [title, setTitle] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const add = async () => {
    const text = title().trim();
    if (!text || saving()) return;
    setSaving(true);
    try {
      await api.items.create(props.item.project_id, { title: text, parent_id: props.item.id });
      toast.success('Item created');
      setTitle('');
      setAdding(false);
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT));
      props.onChanged?.();
      void refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create item');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Fold
      kind="item-progress"
      title="Progress"
      defaultOpen
      testId="item-progress"
      needs={progress() ? needsYouText(progress()!) : null}
      summary={progress() && progress()!.total > 0 ? progressText(progress()!, pointsWord()) : undefined}
    >
      <div class="space-y-4">
        <Show
          when={progress() && progress()!.total > 0}
          fallback={<p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Nothing is under {props.item.title} yet.</p>}
        >
          <ProgressBar progress={progress()} pointsWord={pointsWord()} />
        </Show>
        <Show when={teams().length > 0}>
          <p data-testid="progress-teams" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Teams: {teams().join(', ')}
          </p>
        </Show>

        <For each={sections()}>
          {(section) => (
            <section data-testid="progress-section" data-state={section.state ?? 'none'} class="min-w-0 space-y-1.5">
              <h3 class="flex items-center gap-2 text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
                <Show when={section.state} fallback={<span>No run yet</span>}>
                  {(state) => <RunState state={state()} size="chip" />}
                </Show>
                <span aria-label={`${section.items.length} items`}>{section.items.length}</span>
              </h3>
              <ul class="m-0 flex list-none flex-col gap-0.5 p-0">
                <For each={section.items}>
                  {(item) => (
                    <li class="flex min-w-0 items-baseline gap-2 text-sm">
                      <A
                        href={itemHref(item.project_id, item.id)}
                        class="min-w-0 truncate font-semibold hover:underline focus:outline-none focus-visible:ring-2"
                        style={{ color: 'var(--color-accent-ink)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                      >
                        {item.title}
                      </A>
                      <span class="flex-none text-xs" style={{ color: 'var(--color-text-tertiary)' }}>{item.status}</span>
                    </li>
                  )}
                </For>
              </ul>
            </section>
          )}
        </For>

        <div class="flex flex-wrap items-center gap-2">
          <Show
            when={adding()}
            fallback={
              <Button size="sm" variant="secondary" onClick={() => setAdding(true)} aria-label={`Add item to ${props.item.title}`}>
                <IconPlus size={13} />
                Add item
              </Button>
            }
          >
            <input
              type="text"
              aria-label="New item title"
              placeholder="Item title… (Enter to add, Esc to cancel)"
              value={title()}
              disabled={saving()}
              ref={(el) => queueMicrotask(() => el.focus())}
              onInput={(e) => setTitle(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void add();
                } else if (e.key === 'Escape') {
                  e.stopPropagation();
                  setAdding(false);
                }
              }}
              class="min-h-9 min-w-0 flex-1 rounded-full border px-3.5 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            />
            <Button size="sm" onClick={() => void add()} disabled={saving() || !title().trim()}>Add</Button>
            <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>Cancel</Button>
          </Show>
          {/* While the group has a run, the Work panel offers the next plan. */}
          <Show when={!hasRun()}>
            <RunWithAgentButton
              itemId={props.item.id}
              itemTitle={props.item.title}
              projectId={props.item.project_id}
              variant="secondary"
              label="Plan with agent"
            />
          </Show>
        </div>
      </div>
    </Fold>
  );
};

export default GroupProgress;
