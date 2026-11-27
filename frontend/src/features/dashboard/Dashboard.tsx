import { createMemo, createSignal, For, Match, Show, Switch } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Select } from '../../shared/ui';
import Progress from './Progress';
import AgentsWork from './AgentsWork';
import AutonomyByDomain from './AutonomyByDomain';

/** The three parts of the Overview, in the order the switch lists them; Progress is the one the address names when it names none. */
const PARTS = [
  { id: 'progress', label: 'Progress' },
  { id: 'agents', label: "Agents' work" },
  { id: 'autonomy', label: 'Autonomy by domain' },
] as const;

type Part = (typeof PARTS)[number]['id'];

function partOf(raw: string | string[] | undefined): Part {
  const id = Array.isArray(raw) ? raw[0] : raw;
  return PARTS.find((p) => p.id === id)?.id ?? 'progress';
}

/** The periods the agents' work and the autonomy by domain are read over, in days; `all` is all time. */
const WINDOWS = [
  { value: 'all', label: 'All time' },
  { value: '7', label: 'Last 7 days' },
  { value: '28', label: 'Last 28 days' },
  { value: '90', label: 'Last 90 days' },
];

function sinceOf(period: string): string | undefined {
  return period === 'all' ? undefined : new Date(Date.now() - Number(period) * 86_400_000).toISOString();
}

/** How the project is going: its progress, how well the agents' work is going, and what each domain is trusted with. */
export default function Dashboard() {
  const [params, setParams] = useSearchParams();
  const part = createMemo(() => partOf(params.part));
  const [period, setPeriod] = createSignal('all');
  const since = createMemo(() => sinceOf(period()));

  return (
    <div class="flex flex-col gap-8">
      <header class="flex flex-col gap-4">
        <div>
          <h1 class="m-0 text-title text-content">Overview</h1>
          <p class="mt-1 text-small text-content-muted">How the plan is going and how well the agents' work is going</p>
        </div>
        <div class="flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
          <div
            role="group"
            aria-label="Part of the overview"
            data-testid="overview-parts"
            class="inline-flex gap-0.5 rounded-md border p-0.5"
            style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
          >
            <For each={PARTS}>
              {(p) => (
                <button
                  type="button"
                  aria-pressed={part() === p.id}
                  data-testid={`overview-part-${p.id}`}
                  class="h-8 rounded-md px-3.5 text-small focus:outline-none focus-visible:ring-2"
                  style={{
                    'background-color': part() === p.id ? 'var(--color-bg-base)' : 'transparent',
                    color: part() === p.id ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
                    'font-weight': part() === p.id ? 700 : 500,
                    border: 0,
                    cursor: 'pointer',
                    '--tw-ring-color': 'var(--color-focus-ring)',
                  }}
                  onClick={() => setParams({ part: p.id === 'progress' ? undefined : p.id }, { replace: true })}
                >
                  {p.label}
                </button>
              )}
            </For>
          </div>
          <Show when={part() !== 'progress'}>
            <div class="w-48">
              <Select label="Period" options={WINDOWS} value={period()} onChange={(e) => setPeriod(e.currentTarget.value)} />
            </div>
          </Show>
        </div>
      </header>

      <Switch>
        <Match when={part() === 'progress'}>
          <Progress />
        </Match>
        <Match when={part() === 'agents'}>
          <AgentsWork since={since()} />
        </Match>
        <Match when={part() === 'autonomy'}>
          <AutonomyByDomain since={since()} />
        </Match>
      </Switch>
    </div>
  );
}
