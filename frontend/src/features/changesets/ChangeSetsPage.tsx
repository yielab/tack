import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { A, useParams, useSearchParams } from '@solidjs/router';
import { Button, LoadError, Skeleton } from '../../shared/ui';
import { api } from '../../shared/api';
import type { ChangeSet, ChangeSetStatus } from '../../shared/api/changeSets';
import ChangeSetPanel from '../../shared/runWithAgent/ChangeSetPanel';
import { proposerName } from '../../shared/runWithAgent/changeSetOps';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import { refreshChangeSets, waitingChangeSetsOf } from '../../shared/state/waiting';
import { changeSetsOffReason } from '../../shared/state/changeSets';
import { useProject } from '../../shared/state/projectContext';

type Filter = 'waiting' | 'applied' | 'rejected' | 'all';

const FILTERS: ReadonlyArray<{ id: Filter; label: string; statuses?: ChangeSetStatus[] }> = [
  { id: 'waiting', label: 'Waiting for you', statuses: ['waiting', 'partly'] },
  { id: 'applied', label: 'Applied', statuses: ['applied'] },
  { id: 'rejected', label: 'Turned down', statuses: ['rejected'] },
  { id: 'all', label: 'All' },
];

const EMPTY: Record<Filter, string> = {
  waiting: 'No change set waits for you.',
  applied: 'No change set has been applied yet.',
  rejected: 'No change set has been turned down.',
  all: 'No agent has proposed a change in this project yet.',
};

/**
 * A project's change sets by status, each in the one panel that applies, edits or rejects it. A
 * decided one folds into a line so the ones that wait stay in view.
 */
const ChangeSetsPage: Component = () => {
  const params = useParams();
  const [search] = useSearchParams();
  const { project } = useProject();
  const projectId = () => params.id as string;
  // A link from the inbox names the change set; it may no longer wait, so the list opens on all.
  const opened = () => (typeof search.open === 'string' ? search.open : undefined);
  const [filter, setFilter] = createSignal<Filter>(opened() ? 'all' : 'waiting');
  const [list, { refetch }] = createResource(
    () => [projectId(), filter()] as const,
    ([id, f]) => api.changeSets.list(id, FILTERS.find((x) => x.id === f)?.statuses, 100),
  );
  const sets = (): ChangeSet[] => (list.error || !Array.isArray(list()) ? [] : list()!);
  const offReason = () => changeSetsOffReason(project() ?? undefined, waitingChangeSetsOf(projectId()));
  const decided = (s: ChangeSet) => s.status === 'applied' || s.status === 'rejected';

  return (
    <div class="flex min-w-0 flex-col gap-4 px-6 py-7 sm:px-12" data-testid="change-sets-page">
      <div class="flex flex-wrap items-end justify-between gap-3">
        <div class="min-w-0">
          <h1 class="m-0 text-content leading-tight" style={{ 'font-size': '2.125rem', color: 'var(--color-text-primary)' }}>
            Change sets
          </h1>
          <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            What agents propose to change on the board. Nothing applies until you say so, except what the project's rung lets through.
          </p>
        </div>
        <Button size="sm" variant="secondary" onClick={() => void refetch()}>
          Refresh
        </Button>
      </div>

      <Show when={offReason()}>
        {(why) => (
          <p data-testid="change-sets-off" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            {why()}{' '}
            <A href={`/projects/${projectId()}/settings?tab=automation`} class="underline" style={{ color: 'var(--color-text-primary)' }}>
              Open Automation
            </A>
          </p>
        )}
      </Show>

      <div role="group" aria-label="Show" class="flex flex-wrap gap-2">
        <For each={FILTERS}>
          {(f) => (
            <button
              type="button"
              aria-pressed={filter() === f.id}
              class="rounded-full border px-3 py-1 text-sm"
              style={{
                'border-color': filter() === f.id ? 'var(--color-primary-600)' : 'var(--color-border-medium)',
                'background-color': filter() === f.id ? 'var(--color-accent-soft)' : 'transparent',
                color: filter() === f.id ? 'var(--color-accent-ink)' : 'var(--color-text-primary)',
                'font-weight': filter() === f.id ? 700 : 500,
              }}
              onClick={() => setFilter(f.id)}
            >
              {f.label}
            </button>
          )}
        </For>
      </div>

      <Show when={!list.error} fallback={<LoadError what="the change sets" error={list.error} onRetry={() => void refetch()} />}>
        <Show when={!list.loading || list()} fallback={<Skeleton />}>
          <Show when={sets().length > 0} fallback={<p data-testid="change-sets-empty" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>{EMPTY[filter()]}</p>}>
            <ul class="flex min-w-0 flex-col gap-3">
              <For each={sets()}>
                {(cs) => (
                  <li data-testid="change-set-item" id={`change-set-${cs.id}`} class="min-w-0">
                    <Show
                      when={decided(cs) && cs.id !== opened()}
                      fallback={<ChangeSetPanel changeSet={cs} onChange={() => void refreshChangeSets()} />}
                    >
                      <details class="min-w-0 rounded-item p-3" style={{ border: '1px solid var(--color-border-light)' }}>
                        <summary class="cursor-pointer text-sm" style={{ color: 'var(--color-text-primary)' }}>
                          {proposerName(cs)} · {cs.status === 'applied' ? 'Applied' : 'Turned down'} {relativeTimeFromIso(cs.decided_at ?? cs.created_at)} · {cs.reason}
                        </summary>
                        <div class="mt-3">
                          <ChangeSetPanel changeSet={cs} />
                        </div>
                      </details>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </Show>
    </div>
  );
};

export default ChangeSetsPage;
