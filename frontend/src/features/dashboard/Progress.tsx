import { createEffect, createResource, For, Show, createMemo, onCleanup } from 'solid-js';
import { A, useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { useProject } from '../../shared/state/projectContext';
import { useExecutionStore } from '../../shared/state/executionContext';
import { computeDashboardStats, computeRunStats, workOf, TOKENS_BY_TEAM_UNAVAILABLE, type RunOutcome, type RunStats } from './computeStats';
import { useVocab } from '../../shared/vocab/useVocab';
import { Button, EmptyState, Skeleton } from '../../shared/ui';
import { IconOverview } from '../../shared/ui/icons';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { typeTone } from '../../shared/ui/TypeTile';
import { useItemTypes } from '../../shared/vocab/useItemTypes';
import type { TypeColor } from '../../shared/types';
import type { JSX } from 'solid-js';
import type { Item, Priority } from '../../shared/types';
import Card from '../../shared/ui/Card';
import ProgressBar from '../../shared/ui/ProgressBar';
import TypeTile from '../../shared/ui/TypeTile';
import { itemHref } from '../../shared/ui/ItemPath';

/** The runs bar's segments, in the order the board lists its states. Colours are the run-state tokens. */
const RUN_OUTCOMES: ReadonlyArray<{ id: RunOutcome; label: string }> = [
  { id: 'done', label: 'Done' },
  { id: 'review', label: 'Ready for review' },
  { id: 'needs', label: 'Needs you' },
  { id: 'running', label: 'Queued or running' },
  { id: 'failed', label: 'Failed' },
  { id: 'cancelled', label: 'Cancelled' },
];

export default function Progress() {
  const params = useParams();
  const projectId = params.id!;
  const navigate = useNavigate();

  const { project, vocabulary } = useProject();
  const { t } = useVocab();
  const pointsWord = () => vocabulary()?.story_points?.toLowerCase();
  const types = useItemTypes();
  const [items] = createResource(() => api.items.list(projectId));
  const executions = useExecutionStore();

  // The runs row reads each item's latest run through the store the board fills, so the
  // project's runs come in one batched request, the same one the board's cards use.
  createEffect(() => {
    const stops = workOf(items() ?? []).map((item) => executions.watchItem(item.id));
    onCleanup(() => stops.forEach((stop) => stop()));
  });

  // Computed statistics (pure aggregation in computeStats.ts).
  const stats = createMemo(() =>
    computeDashboardStats(items() || [], project()?.workflow?.statuses || [], new Date()),
  );

  // The store holds every run any view has loaded, so only this project's items count.
  const runStats = createMemo(() =>
    computeRunStats(
      workOf(items() || []),
      [...executions.requests().values()].flatMap((record) => (record.summary ? [record.summary] : [])),
      new Date(),
    ),
  );

  const getStatusCategoryColor = (category: string) => {
    switch (category) {
      case 'done': return 'var(--color-success-600)';
      case 'in_progress': return 'var(--color-primary-600)';
      case 'todo': return 'var(--color-accent2)';
      default: return 'var(--color-primary-600)';
    }
  };

  const pct = (n: number) => (stats().totalItems > 0 ? Math.round((n / stats().totalItems) * 100) : 0);

  return (
    <div class="flex flex-col gap-12">
      {/* Loading */}
      <Show when={items.loading && !items()}>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4" aria-hidden="true">
          <For each={[1, 2, 3]}>{() => <Skeleton height="90px" class="rounded-lg!" />}</For>
        </div>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4" aria-hidden="true">
          <For each={[1, 2]}>{() => <Skeleton height="180px" class="rounded-lg!" />}</For>
        </div>
      </Show>

      {/* No items yet */}
      <Show when={!items.loading && (items() ?? []).length === 0}>
        <Card class="p-6">
          <EmptyState
            icon={<IconOverview size={28} />}
            title="No data to show yet"
            description="Statistics appear here once you add items to your project. Start in Board or List."
            action={
              <Button variant="secondary" size="sm" onClick={() => navigate(`/projects/${projectId}/board`)}>
                Go to Board
              </Button>
            }
          />
        </Card>
      </Show>

      <Show when={(items() ?? []).length > 0}>
        {/* What needs a person comes first, then how far the work is. */}
        <section aria-label="Where the work stands" class="flex flex-col gap-4">
          <Show when={runStats().withRuns > 0}>
            <NeedsYouStrip count={runStats().needYou} projectId={projectId} />
          </Show>
          <StatTile label="Completion Rate" lead>
            {stats().completionRate}%
          </StatTile>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
            <StatTile label="Total Items">
              {stats().totalItems}
            </StatTile>
            <StatTile label="Completed">
              {stats().doneItems}
            </StatTile>
            <StatTile
              label="Completed (7 / 30 days)"
              sub={<>+{stats().recentItems} added in 7 days</>}
            >
              {stats().throughput7} <span class="text-lead text-content-subtle">/ {stats().throughput30}</span>
            </StatTile>
          </div>
        </section>

        {/* Runs: hidden until the project has run anything, so a board that never ran has no empty row. */}
        <Show when={runStats().withRuns > 0}>
          <RunsRow stats={runStats()} projectId={projectId} />
        </Show>

        {/* Groups: absent in a project without any, so it looks as it always did. */}
        <Show when={stats().groups.length > 0}>
          <GroupsPanel groups={stats().groups} pointsWord={pointsWord()} />
        </Show>

        <section aria-labelledby="breakdown-heading" class="flex flex-col gap-4">
          <h2 id="breakdown-heading" class="text-section text-content">Breakdown</h2>
          <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
            {/* Status Distribution */}
            <MeterCard title="Status Distribution">
              <For each={stats().byStatus}>
                {(status) => (
                  <Meter
                    label={status.name}
                    value={`${status.count} (${pct(status.count)}%)`}
                    percent={pct(status.count)}
                    color={getStatusCategoryColor(status.category)}
                  />
                )}
              </For>
            </MeterCard>

            {/* Priority Distribution: same colours as PriorityDot */}
            <MeterCard title="Priority Distribution">
              <For each={Object.entries(stats().byPriority)}>
                {([priority, count]) => (
                  <Meter
                    label={priority}
                    capitalize
                    value={`${count} (${pct(count)}%)`}
                    percent={pct(count)}
                    color={priorityColor(priority as Priority)}
                  />
                )}
              </For>
            </MeterCard>

            {/* Type Distribution */}
            <MeterCard title="Item Types">
              <For each={stats().byType}>
                {(type) => (
                  <Meter
                    label={types.name(type.name)}
                    value={`${type.count} (${pct(type.count)}%)`}
                    percent={pct(type.count)}
                    color={typeBarColor(types.color(type.name))}
                  />
                )}
              </For>
            </MeterCard>

            {/* Story Points Progress */}
            <Show when={stats().totalEstimate > 0}>
              <MeterCard title={`${t('story_points')} Progress`}>
                <div class="flex items-center justify-between text-small">
                  <span class="text-content-muted">Total Points</span>
                  <span class="font-heading text-step text-content">
                    {stats().totalEstimate}
                  </span>
                </div>
                <div class="flex items-center justify-between text-small">
                  <span class="text-content-muted">Completed</span>
                  <span class="font-heading text-step" style={{ color: 'var(--color-success-600)' }}>
                    {stats().completedEstimate}
                  </span>
                </div>
                <Meter
                  label="Progress"
                  value={`${Math.round((stats().completedEstimate / stats().totalEstimate) * 100)}%`}
                  percent={(stats().completedEstimate / stats().totalEstimate) * 100}
                  color="var(--color-success-600)"
                />
              </MeterCard>
            </Show>
          </div>
        </section>
      </Show>{/* end: items > 0 */}
    </div>
  );
}

/** Bar colour per item type — the same hue family TypeTile uses; neutral
 *  types fall back to the tertiary text tone so the bar stays visible. */
function typeBarColor(color: TypeColor): string {
  const fg = typeTone(color).fg;
  return fg === 'var(--color-text-secondary)' ? 'var(--color-text-tertiary)' : fg;
}

/** What waits for a person in this project right now, in amber and first; with nothing waiting, one quiet line. */
function NeedsYouStrip(props: { count: number; projectId: string }) {
  return (
    <Show
      when={props.count > 0}
      fallback={<p class="m-0 text-small text-content-muted">Nothing needs you now.</p>}
    >
      <div
        data-testid="overview-needs-you"
        class="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border px-5 py-4"
        style={{ 'background-color': 'var(--color-warning-50)', 'border-color': 'var(--color-warning-100)' }}
      >
        <p class="m-0 text-reading font-semibold" style={{ color: 'var(--color-warning-700)' }}>
          {props.count} {props.count === 1 ? 'item needs' : 'items need'} you now
        </p>
        <p class="m-0 text-small text-content-muted">A question to answer or a run ready for review.</p>
        <A
          href={`/projects/${props.projectId}/needs-you`}
          class="ml-auto text-small font-semibold hover:underline focus:outline-none focus-visible:ring-2"
          style={{ color: 'var(--color-warning-700)', '--tw-ring-color': 'var(--color-focus-ring)' }}
        >
          Open Needs you
        </A>
      </div>
    </Show>
  );
}

/**
 * The runs row: this week's runs as one bar with the count of each outcome in words beside it, and tokens
 * per team. That last tile is disabled with its reason, since the figure cannot be made from what this page
 * loads. The agents' work is one part over, linked from here.
 */
function RunsRow(props: { stats: RunStats; projectId: string }) {
  const week = () => props.stats.week;
  const weekTotal = () => RUN_OUTCOMES.reduce((sum, outcome) => sum + week()[outcome.id], 0);
  // The bar is a graphic, so its counts are spelled out for a screen reader.
  const barLabel = () =>
    weekTotal() === 0
      ? 'Runs started in the last 7 days: none.'
      : `Runs started in the last 7 days: ${RUN_OUTCOMES.filter((o) => week()[o.id] > 0)
          .map((o) => `${week()[o.id]} ${o.label.toLowerCase()}`)
          .join(', ')}.`;

  return (
    <section aria-labelledby="runs-heading" class="flex flex-col gap-4">
      <div class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h2 id="runs-heading" class="text-section text-content">Runs</h2>
        <A
          href={`/projects/${props.projectId}/overview?part=agents`}
          class="text-small font-semibold hover:underline focus:outline-none focus-visible:ring-2"
          style={{ color: 'var(--color-accent-ink)', '--tw-ring-color': 'var(--color-focus-ring)' }}
        >
          Agents' work
        </A>
      </div>

      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <Card class="flex min-w-0 flex-col gap-3 p-5 lg:col-span-2">
          <p class="m-0 text-small font-semibold text-content-muted">Runs started in the last 7 days</p>
          <div role="img" aria-label={barLabel()} class="flex h-2 w-full overflow-hidden rounded-full bg-app">
            <For each={RUN_OUTCOMES}>
              {(outcome) => (
                <Show when={week()[outcome.id] > 0}>
                  <div class="h-full" style={{ flex: week()[outcome.id], background: `var(--run-${outcome.id}-ink)` }} />
                </Show>
              )}
            </For>
          </div>
          <ul class="m-0 grid list-none grid-cols-2 gap-x-4 gap-y-1 p-0 text-small sm:grid-cols-3">
            <For each={RUN_OUTCOMES}>
              {(outcome) => (
                <li class="flex min-w-0 items-center gap-2">
                  <span
                    aria-hidden="true"
                    class="h-2.5 w-2.5 flex-none rounded-full"
                    style={{ background: `var(--run-${outcome.id}-ink)` }}
                  />
                  <span class="min-w-0 text-content" style={{ 'overflow-wrap': 'anywhere' }}>
                    {outcome.label}
                  </span>
                  <span class="ml-auto text-content-muted">{week()[outcome.id]}</span>
                </li>
              )}
            </For>
          </ul>
          <p class="m-0 text-small text-content-subtle">Each item counts once, by its latest run.</p>
        </Card>

        <StatTile label="Tokens per team" unavailable sub={TOKENS_BY_TEAM_UNAVAILABLE}>
          —
        </StatTile>
      </div>
    </section>
  );
}

/** Each top-level group with how far its work is, the ones that need a person first. */
function GroupsPanel(props: { groups: Item[]; pointsWord?: string }) {
  return (
    <section aria-labelledby="groups-heading" class="flex flex-col gap-4">
      <h2 id="groups-heading" class="text-section text-content">Groups</h2>
      <Card class="p-5">
        <ul class="m-0 flex list-none flex-col gap-3 p-0">
          <For each={props.groups}>
            {(group) => (
              <li data-testid="group-row" class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
                <TypeTile type={group.item_type} class="flex-none" />
                <A
                  href={itemHref(group.project_id, group.id)}
                  class="min-w-0 truncate text-body font-semibold hover:underline focus:outline-none focus-visible:ring-2"
                  style={{ color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                >
                  {group.title}
                </A>
                <Show
                  when={(group.progress?.total ?? 0) > 0}
                  fallback={<span class="text-small text-content-muted">No work yet</span>}
                >
                  <ProgressBar progress={group.progress} pointsWord={props.pointsWord} class="ml-auto" />
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Card>
    </section>
  );
}

function StatTile(props: {
  label: string;
  /** The one figure the page leads with: set at the title size. */
  lead?: boolean;
  /** A figure this page cannot make: the tile is shown disabled, and its `sub` says why. */
  unavailable?: boolean;
  sub?: JSX.Element;
  children: JSX.Element;
}) {
  return (
    <Card
      class="flex min-w-0 flex-col gap-1.5 p-5"
      role={props.unavailable ? 'group' : undefined}
      aria-label={props.unavailable ? props.label : undefined}
      aria-disabled={props.unavailable ? 'true' : undefined}
    >
      <p class="m-0 text-small font-semibold text-content-muted">{props.label}</p>
      <p
        class="m-0 font-heading leading-none"
        classList={{ 'text-title': props.lead, 'text-step': !props.lead }}
        style={{ color: props.unavailable ? 'var(--color-text-tertiary)' : undefined }}
      >
        {props.children}
      </p>
      <Show when={props.sub}>
        <p class="m-0 text-small text-content-subtle" style={{ 'overflow-wrap': 'anywhere' }}>{props.sub}</p>
      </Show>
    </Card>
  );
}

function MeterCard(props: { title: string; children: JSX.Element }) {
  return (
    <Card class="flex flex-col gap-3 p-5">
      <h3 class="m-0 text-lead text-content">{props.title}</h3>
      {props.children}
    </Card>
  );
}

function Meter(props: { label: string; value: string; percent: number; color: string; capitalize?: boolean }) {
  return (
    <div class="flex flex-col gap-1">
      <div class="flex items-center text-small">
        <span class="text-content" classList={{ capitalize: !!props.capitalize }}>{props.label}</span>
        <span class="ml-auto text-content-muted">{props.value}</span>
      </div>
      <div class="w-full h-[7px] rounded-full bg-app">
        <div class="h-full rounded-full" style={{ width: `${props.percent}%`, background: props.color }} />
      </div>
    </div>
  );
}
