import { createResource, For, Show, createMemo } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { useProject } from '../../shared/state/projectContext';
import { computeDashboardStats } from './computeStats';
import { useVocab } from '../../shared/vocab/useVocab';
import { Button, EmptyState, Skeleton } from '../../shared/ui';
import { IconOverview } from '../../shared/ui/icons';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { typeBadgeTone } from '../../shared/ui/TypeBadge';
import type { JSX } from 'solid-js';
import type { Priority } from '../../shared/types';

export default function Dashboard() {
  const params = useParams();
  const projectId = params.id!;
  const navigate = useNavigate();

  const { project } = useProject();
  const { t } = useVocab();
  const [items] = createResource(() => api.items.list(projectId));

  // Computed statistics (pure aggregation in computeStats.ts).
  const stats = createMemo(() =>
    computeDashboardStats(items() || [], project()?.workflow?.statuses || [], new Date()),
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
    <div class="flex flex-col gap-[18px]">
      {/* Header */}
      <div>
        <h1 class="m-0 text-content" style={{ 'font-size': '34px' }}>
          Overview
        </h1>
        <p class="mt-0.5 text-[13px] text-content-muted">
          Project statistics and progress
        </p>
      </div>

      {/* Loading */}
      <Show when={items.loading && !items()}>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3.5" aria-hidden="true">
          <For each={[1, 2, 3, 4]}>{() => <Skeleton height="90px" class="rounded-[24px]!" />}</For>
        </div>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-3.5" aria-hidden="true">
          <For each={[1, 2]}>{() => <Skeleton height="180px" class="rounded-[28px]!" />}</For>
        </div>
      </Show>

      {/* No items yet */}
      <Show when={!items.loading && (items() ?? []).length === 0}>
        <div class="rounded-[28px] bg-panel">
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
        </div>
      </Show>

      {/* Stats Grid */}
      <Show when={(items() ?? []).length > 0}>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3.5">
          <StatTile label="Total Items" accent>
            {stats().totalItems}
          </StatTile>
          <StatTile label="Completed">
            {stats().doneItems}
          </StatTile>
          <StatTile label="Completion Rate">
            {stats().completionRate}%
          </StatTile>
          <StatTile
            label="Completed (7 / 30 days)"
            sub={<>+{stats().recentItems} added in 7 days</>}
          >
            {stats().throughput7} <span class="text-lg text-content-subtle">/ {stats().throughput30}</span>
          </StatTile>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
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

          {/* Priority Distribution — same colours as PriorityDot */}
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
                  label={type.name}
                  capitalize
                  value={`${type.count} (${pct(type.count)}%)`}
                  percent={pct(type.count)}
                  color={typeBarColor(type.name)}
                />
              )}
            </For>
          </MeterCard>

          {/* Story Points Progress */}
          <Show when={stats().totalEstimate > 0}>
            <MeterCard title={`${t('story_points')} Progress`}>
              <div class="flex items-center justify-between text-[13px]">
                <span class="text-content-muted">Total Points</span>
                <span class="font-heading text-2xl text-content">
                  {stats().totalEstimate}
                </span>
              </div>
              <div class="flex items-center justify-between text-[13px]">
                <span class="text-content-muted">Completed</span>
                <span class="font-heading text-2xl" style={{ color: 'var(--color-success-600)' }}>
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
      </Show>{/* end: items > 0 */}
    </div>
  );
}

/** Bar colour per item type — the same hue family TypeBadge uses; neutral
 *  types fall back to the secondary text tone so the bar stays visible. */
function typeBarColor(key: string): string {
  const fg = typeBadgeTone(key).fg;
  return fg === 'var(--color-text-secondary)' ? 'var(--color-text-tertiary)' : fg;
}

function StatTile(props: { label: string; accent?: boolean; sub?: JSX.Element; children: JSX.Element }) {
  return (
    <div
      class="rounded-[28px] px-5 py-[18px] flex flex-col gap-1.5"
      style={props.accent
        ? { 'background-color': 'var(--color-primary-600)', color: 'var(--color-on-accent)' }
        : { 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-primary)' }}
    >
      <p
        class="text-xs font-bold"
        style={{ color: props.accent ? 'var(--color-on-accent)' : 'var(--color-text-secondary)' }}
      >
        {props.label}
      </p>
      <p class="font-heading leading-none" style={{ 'font-size': '40px' }}>
        {props.children}
      </p>
      <Show when={props.sub}>
        <p class="text-xs text-content-subtle">{props.sub}</p>
      </Show>
    </div>
  );
}

function MeterCard(props: { title: string; children: JSX.Element }) {
  return (
    <div class="rounded-[28px] bg-panel px-5 py-[18px] flex flex-col gap-2.5">
      <h2 class="text-lg text-content">{props.title}</h2>
      {props.children}
    </div>
  );
}

function Meter(props: { label: string; value: string; percent: number; color: string; capitalize?: boolean }) {
  return (
    <div class="flex flex-col gap-1">
      <div class="flex items-center text-[13px]">
        <span class="text-content" classList={{ capitalize: !!props.capitalize }}>{props.label}</span>
        <span class="ml-auto text-content-muted">{props.value}</span>
      </div>
      <div class="w-full h-[7px] rounded-full bg-app">
        <div class="h-full rounded-full" style={{ width: `${props.percent}%`, background: props.color }} />
      </div>
    </div>
  );
}
