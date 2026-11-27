import { type Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { A, useNavigate, useParams, useSearchParams } from '@solidjs/router';
import { Badge, Button, LoadError, Skeleton } from '../../shared/ui';
import { api } from '../../shared/api';
import type { ChangeSet } from '../../shared/api/changeSets';
import type { AutonomySettings } from '../../shared/api/autonomy';
import type { InboxNotification } from '../../shared/api/notifications';
import ChangeSetPanel from '../../shared/runWithAgent/ChangeSetPanel';
import { proposerName } from '../../shared/runWithAgent/changeSetOps';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import { changeSetIdOf, notificationHref, projectWaits, refreshChangeSets, waitingChangeSetsOf } from '../../shared/state/waiting';
import { changeSetsOffReason } from '../../shared/state/changeSets';
import { useProject } from '../../shared/state/projectContext';
import { shortItemId } from '../../shared/shortItemId';
import { waitedFor, wordsOf } from './words';

type Filter = 'waiting' | 'decided';

const FILTERS: ReadonlyArray<{ id: Filter; label: string }> = [
  { id: 'waiting', label: 'Waiting' },
  { id: 'decided', label: 'Decided' },
];

/** The limit a project sets on what waits for a person; the API serves it with the budgets once it exists. */
type WaitingBudgets = AutonomySettings['budgets'] & { waiting_max?: number | null };

/** One change set that waits, in the one panel that applies, edits or turns it down. */
const ChangeSetBody: Component<{ changeSetId: string; open: boolean }> = (p) => {
  const [set, { refetch }] = createResource(() => p.changeSetId, (id) => api.changeSets.get(id));
  return (
    <Show when={p.open}>
      <div class="mt-4 min-w-0" data-testid="needs-you-change-set">
        <Show when={!set.error} fallback={<LoadError what="the change set" error={set.error} onRetry={() => void refetch()} />}>
          <Show when={set()} fallback={<Skeleton />}>
            {(cs) => (
              <ChangeSetPanel
                changeSet={cs()}
                onChange={() => {
                  void refreshChangeSets();
                  void refetch();
                }}
              />
            )}
          </Show>
        </Show>
      </div>
    </Show>
  );
};

const Wait: Component<{
  n: InboxNotification;
  title: string | undefined;
  /** The first one in the list is the next thing to do: its action is the solid button. */
  next: boolean;
  /** The change set the address asks for starts open. */
  opened: boolean;
}> = (p) => {
  const navigate = useNavigate();
  const words = () => wordsOf(p.n.kind);
  const isChangeSet = () => p.n.kind === 'change_set';
  const [shown, setShown] = createSignal(p.opened);
  const href = () => notificationHref(p.n);
  const what = () => p.title ?? (p.n.item_id ? shortItemId(p.n.item_id) : null);
  return (
    <li
      data-testid="needs-you-row"
      data-kind={p.n.kind}
      class="min-w-0 py-5"
      style={{ 'border-bottom': '1px solid var(--color-border-light)' }}
    >
      <div class="flex min-w-0 flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <div class="flex min-w-0 flex-col gap-1.5">
          <div class="flex min-w-0 flex-wrap items-center gap-2">
            <Badge tone="warning">{words().label}</Badge>
            <span class="text-small" style={{ color: 'var(--color-text-secondary)' }} title={relativeTimeFromIso(p.n.created_at)}>
              {waitedFor(p.n.created_at)}
            </span>
          </div>
          <Show when={p.n.item_id && what()} fallback={<p class="text-reading" style={{ color: 'var(--color-text-primary)' }}>{p.n.title}</p>}>
            <A
              href={`/projects/${p.n.project_id}/items/${p.n.item_id}`}
              class="min-w-0 truncate text-reading font-semibold hover:underline"
              style={{ color: 'var(--color-text-primary)' }}
              data-testid="needs-you-item"
            >
              {what()}
            </A>
          </Show>
          <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{p.n.cause}</p>
        </div>
        <Show
          when={isChangeSet()}
          fallback={
            <Show when={href()}>
              <Button
                size="sm"
                variant={p.next ? 'primary' : 'secondary'}
                aria-label={`${words().action}: ${what() ?? p.n.title}`}
                onClick={() => navigate(href()!)}
              >
                {words().action}
              </Button>
            </Show>
          }
        >
          <Button
            size="sm"
            variant={p.next ? 'primary' : 'secondary'}
            aria-expanded={shown()}
            aria-label={`${shown() ? 'Hide' : words().action}: ${what() ?? p.n.title}`}
            onClick={() => setShown(!shown())}
          >
            {shown() ? 'Hide the changes' : words().action}
          </Button>
        </Show>
      </div>
      <Show when={isChangeSet()}>
        <ChangeSetBody changeSetId={changeSetIdOf(p.n)} open={shown()} />
      </Show>
    </li>
  );
};

/** Every change set that was applied or turned down, each folded into a line that opens its panel. */
const Decided: Component<{ projectId: string }> = (p) => {
  const [list, { refetch }] = createResource(
    () => p.projectId,
    (id) => api.changeSets.list(id, ['applied', 'rejected'], 100),
  );
  const sets = (): ChangeSet[] => (list.error || !Array.isArray(list()) ? [] : list()!);
  return (
    <Show when={!list.error} fallback={<LoadError what="the change sets" error={list.error} onRetry={() => void refetch()} />}>
      <Show when={!list.loading || list()} fallback={<Skeleton />}>
        <Show
          when={sets().length > 0}
          fallback={
            <p data-testid="needs-you-decided-empty" class="text-body" style={{ color: 'var(--color-text-secondary)' }}>
              No change set has been applied or turned down yet.
            </p>
          }
        >
          <ul class="flex min-w-0 flex-col" style={{ 'border-top': '1px solid var(--color-border-light)' }}>
            <For each={sets()}>
              {(cs) => (
                <li
                  data-testid="needs-you-decided"
                  id={`change-set-${cs.id}`}
                  class="min-w-0 py-3"
                  style={{ 'border-bottom': '1px solid var(--color-border-light)' }}
                >
                  <details class="min-w-0">
                    <summary class="cursor-pointer text-body" style={{ color: 'var(--color-text-primary)' }}>
                      {proposerName(cs)} · {cs.status === 'applied' ? 'Applied' : 'Turned down'} {relativeTimeFromIso(cs.decided_at ?? cs.created_at)} · {cs.reason}
                    </summary>
                    <div class="mt-4">
                      <ChangeSetPanel changeSet={cs} />
                    </div>
                  </details>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </Show>
  );
};

/**
 * What waits for a person in one project, oldest first: each wait with its task, what kind it is,
 * how long it has waited and its one action. The other view lists the change sets already decided.
 */
const NeedsYou: Component = () => {
  const params = useParams();
  const [search] = useSearchParams();
  const { project } = useProject();
  const projectId = () => params.id as string;
  const [filter, setFilter] = createSignal<Filter>('waiting');
  const [tasks] = createResource(projectId, (id) => api.items.list(id).catch(() => []));
  const [autonomy] = createResource(projectId, (id) => api.autonomy.get(id).catch(() => null));

  const waits = () => projectWaits(projectId());
  const titles = createMemo(() => new Map((tasks() ?? []).map((i) => [i.id, i.title])));
  const limit = () => (autonomy()?.budgets as WaitingBudgets | undefined)?.waiting_max ?? null;
  const opened = () => (typeof search.open === 'string' ? search.open : undefined);
  const offReason = () => changeSetsOffReason(project() ?? undefined, waitingChangeSetsOf(projectId()));

  const count = () => {
    const n = waits().length;
    const max = limit();
    if (max !== null) {
      return `${n} of ${max} waiting for you${n >= max ? ': auto-start is paused until one is decided' : ''}`;
    }
    return n === 0 ? null : `${n} waiting for you`;
  };

  return (
    <div class="min-w-0 rounded-lg px-6 py-8 sm:px-12 sm:py-10" style={{ 'background-color': 'var(--color-bg-elevated)' }} data-testid="needs-you-page">
      <header class="flex min-w-0 flex-col gap-2">
        <h1 class="text-title" style={{ color: 'var(--color-text-primary)' }}>Needs you</h1>
        <Show when={count()}>
          <p data-testid="needs-you-count" class="text-reading" style={{ color: 'var(--color-warning-700)' }}>{count()}</p>
        </Show>
      </header>

      <div role="group" aria-label="Show" class="mt-8 flex flex-wrap gap-2">
        <For each={FILTERS}>
          {(f) => (
            <button
              type="button"
              aria-pressed={filter() === f.id}
              class="rounded-md border px-3 py-1 text-small"
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

      <div class="mt-8 min-w-0">
        <Show
          when={filter() === 'waiting'}
          fallback={
            <>
              <Show when={offReason()}>
                {(why) => (
                  <p data-testid="needs-you-off" class="mb-6 text-body" style={{ color: 'var(--color-text-secondary)' }}>
                    {why()}{' '}
                    <A href={`/projects/${projectId()}/settings?tab=automation`} class="underline" style={{ color: 'var(--color-text-primary)' }}>
                      Open Automation
                    </A>
                  </p>
                )}
              </Show>
              <Decided projectId={projectId()} />
            </>
          }
        >
          <Show
            when={waits().length > 0}
            fallback={
              <p data-testid="needs-you-empty" class="text-body" style={{ color: 'var(--color-text-secondary)' }}>
                Nothing waits for you in this project.
              </p>
            }
          >
            <ul class="flex min-w-0 flex-col" style={{ 'border-top': '1px solid var(--color-border-light)' }}>
              <For each={waits()}>
                {(n, i) => (
                  <Wait
                    n={n}
                    title={n.item_id ? titles().get(n.item_id) : undefined}
                    next={i() === 0}
                    opened={n.kind === 'change_set' && changeSetIdOf(n) === opened()}
                  />
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </div>
    </div>
  );
};

export default NeedsYou;
