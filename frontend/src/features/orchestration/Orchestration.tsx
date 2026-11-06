import { type Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { A, useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { request } from '../../shared/api/client';
import {
  agentProfilesApi,
  createExecutionRealtime,
  executionsApi,
  routingApi,
  podsOnly,
  runnersApi,
  teamsApi,
  type AgentProfileSummary,
  type ExecutionSummary,
  type HarnessCapability,
  type RunnerSummary,
  type TeamEntry,
} from '../../shared/execution';
import type { AutonomySettings } from '../../shared/api/autonomy';
import { createBoardSocket } from '../../shared/realtime/boardSocket';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useProjectHasRun } from '../../shared/state/projectHasRun';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { stateRank } from '../../shared/runWithAgent/cardRun';
import { useVocab } from '../../shared/vocab/useVocab';
import { createConfirm, EmptyState, LoadError, RunState, Skeleton, type RunStateName } from '../../shared/ui';
import { NO_AGENT_YET } from '../../shared/ui/WorkTabs';
import { toast } from '../../shared/ui/toast';
import type { ItemDetail } from '../../shared/types';
import {
  blockersOf,
  buildFlow,
  candidateIds,
  capacityText,
  moveTarget,
  movesBetween,
  placements,
  withProfile,
  type Flow,
  type FlowFilter,
  type FlowScope,
} from './flow';
import { buildCompare, buildMap, scopeOfNode } from './map';
import { FlowUiContext, type FlowUi, type MoveSource } from './context';
import FlowView from './Flow';
import MapView from './Map';
import Compare from './Compare';

const MODE_KEY = 'tack_orchestration_mode';
type Mode = 'flow' | 'map';

/** The mode left on last time in this browser; Flow when nothing was saved or storage is closed. */
function readMode(): Mode {
  try {
    return localStorage.getItem(MODE_KEY) === 'map' ? 'map' : 'flow';
  } catch {
    return 'flow';
  }
}

function saveMode(mode: Mode): void {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    /* a convenience, not state */
  }
}

/** How many task details are read at once. */
const DETAIL_PARALLEL = 6;
/** How long a task that moved keeps the mark that makes it arrive with a transition. */
const ARRIVAL_MS = 1500;

interface Loaded<T> {
  value: () => T | undefined;
  error: () => unknown;
  load: () => Promise<void>;
}

/** A value read from the server and kept while it is read again: a refresh never blanks the view. */
function loader<T>(read: () => Promise<T>): Loaded<T> {
  const [value, setValue] = createSignal<{ v: T }>();
  const [error, setError] = createSignal<unknown>();
  return {
    value: () => value()?.v,
    error,
    load: async () => {
      try {
        setValue({ v: await read() });
        setError(undefined);
      } catch (err) {
        setError(err);
      }
    },
  };
}

// Ranked as the Board and the List rank runs, what needs a person first; the tasks nobody will take come last.
interface Counter { filter: FlowFilter; label: string; state?: RunStateName; count: (f: Flow) => number; note?: string }
const COUNTERS: Counter[] = ([
  { filter: 'queued', label: 'Queued', state: 'queued', count: (f) => f.counts.queued },
  { filter: 'nobody', label: 'Nobody will take it', count: (f) => f.counts.nobody, note: 'Tasks that go to no lane, so nothing will start them.' },
  { filter: 'needs', label: 'Needs you', state: 'needs', count: (f) => f.counts.needs, note: 'The run asks you a question and waits for your answer.' },
  { filter: 'review', label: 'Ready for review', state: 'review', count: (f) => f.counts.review, note: 'The run finished and its work waits for you to look at it.' },
  { filter: 'running', label: 'Running', state: 'running', count: (f) => f.counts.running },
] as Counter[]).sort((a, b) => (a.state ? stateRank(a.state) : Infinity) - (b.state ? stateRank(b.state) : Infinity));

/** The row under the title: how many tasks need you, run, wait and have nobody, and what the project and the computers can carry. */
const Summary: Component<{
  flow: Flow;
  active: ReadonlySet<FlowFilter>;
  onToggle: (filter: FlowFilter) => void;
  capacity: string;
}> = (props) => (
  <div class="flex min-w-0 flex-col gap-2" data-testid="flow-summary">
    <div role="group" aria-label="Work" class="flex min-w-0 flex-wrap items-center gap-2">
      <span class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>Work</span>
      <For each={COUNTERS}>
        {(c) => {
          const count = () => c.count(props.flow);
          const on = () => props.active.has(c.filter);
          const unusable = () => count() === 0 && !on();
          const warn = () => (c.filter === 'needs' || c.filter === 'review' || c.filter === 'nobody') && count() > 0;
          return (
            <button
              type="button"
              aria-pressed={on()}
              disabled={unusable()}
              data-testid={`flow-counter-${c.filter}`}
              title={unusable() ? `No tasks: ${c.label.toLowerCase()}` : c.note}
              onClick={() => props.onToggle(c.filter)}
              class="inline-flex min-h-7 items-center gap-1.5 rounded-pill px-3 text-xs font-semibold focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-60"
              style={{
                'background-color': on() ? 'var(--color-accent-soft)' : warn() ? 'var(--run-needs-soft)' : 'var(--color-bg-subtle)',
                color: warn() ? 'var(--run-needs-ink)' : on() ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
                'box-shadow': on() ? 'inset 0 0 0 2px var(--color-accent-line)' : warn() ? 'inset 0 0 0 1px var(--run-needs-line)' : 'none',
              }}
            >
              <Show when={c.state}>
                {(s) => (
                  <span aria-hidden="true" class="inline-flex">
                    <RunState state={s()} size="dot" />
                  </span>
                )}
              </Show>
              <span class="text-sm font-bold tabular-nums">{count()}</span> {c.label}
              <Show when={c.note}>
                <span class="sr-only">{`. ${c.note}`}</span>
              </Show>
            </button>
          );
        }}
      </For>
    </div>
    <p class="text-small" data-testid="flow-capacity" style={{ color: 'var(--color-text-secondary)' }}>
      {props.capacity}
    </p>
  </div>
);

/**
 * Orchestration, in its Flow mode: which team is doing what, at which step of its own process, what
 * waits on a person and what nobody will take. It reads the project's items, runs, profiles and roles,
 * and the teams any agent reports, and follows the realtime stream. Map, beside it in the header's
 * switch, draws the structure behind the lanes and Compare sets the teams side by side; a box of the
 * Map brings Flow back narrowed to what passes through it.
 */
const Orchestration: Component = () => {
  const params = useParams();
  const [, setSearchParams] = useSearchParams();
  const { project } = useProject();
  const { items, refetch } = useProjectItems();
  const hasRun = useProjectHasRun();
  const { t } = useVocab();
  const projectId = () => params.id as string;
  const [ask, ConfirmHost] = createConfirm();

  const runs = loader(() => executionsApi.listByProject(projectId()).then((r) => (r.data.data ?? []) as ExecutionSummary[]));
  // A local agent is a team of one on the Agents page; here it is a lane of its own, drawn from the routes.
  const teams = loader<TeamEntry[]>(() => teamsApi.list().then(podsOnly));
  const profiles = loader<AgentProfileSummary[]>(() => agentProfilesApi.list().then((r) => r.data.data ?? []));
  const roles = loader(() => api.roles.list(projectId()));
  const routing = loader(() => routingApi.list(projectId()));
  const autonomy = loader<AutonomySettings | null>(() => api.autonomy.get(projectId()).catch(() => null));
  const runners = loader<RunnerSummary[] | null>(() => runnersApi.list().then((r) => r.data.data ?? [], () => null));

  const reload = () => {
    void runs.load();
    void teams.load();
    void routing.load();
  };
  createEffect(() => {
    projectId();
    void runs.load();
    void teams.load();
    void routing.load();
    void profiles.load();
    void roles.load();
    void autonomy.load();
    void runners.load();
  });

  // The stream: executions are polled, board changes arrive on the socket and from the item drawer.
  onMount(() => {
    const realtime = createExecutionRealtime();
    const off = realtime.onInvalidate((event) => {
      if (event.scope === 'list') reload();
    });
    const onItemUpdated = () => {
      void refetch();
      reload();
    };
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => {
      off();
      realtime.dispose();
      window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    });
  });
  createEffect(() => {
    const pid = projectId();
    if (!pid) return;
    const socket = createBoardSocket(pid);
    const off = socket.onEvent(() => {
      void refetch();
      reload();
    });
    onCleanup(() => {
      off();
      socket.close();
    });
  });

  const [now, setNow] = createSignal(Date.now());
  onMount(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    onCleanup(() => clearInterval(timer));
  });

  const statuses = createMemo(() => [...(project()?.workflow?.statuses ?? [])].sort((a, b) => a.order - b.order));
  const startStatus = () => project()?.auto_start_status ?? (statuses().find((s) => s.category === 'todo') ?? statuses()[0])?.name ?? null;

  // The blockers of the tasks that matter, read one task at a time and kept until it changes.
  interface Detail {
    blockers: string[];
  }
  const [details, setDetails] = createSignal<ReadonlyMap<string, Detail>>(new Map());
  const requested = new Map<string, string>();
  const waiting: string[] = [];
  let reading = 0;
  const pump = () => {
    while (reading < DETAIL_PARALLEL && waiting.length > 0) {
      const id = waiting.shift()!;
      reading += 1;
      request<ItemDetail>(`/items/${id}`)
        .then((d) =>
          setDetails((prior) =>
            new Map(prior).set(id, {
              blockers: blockersOf(id, d.dependencies ?? []),
            }),
          ),
        )
        .catch(() => requested.delete(id))
        .finally(() => {
          reading -= 1;
          pump();
        });
    }
  };
  createEffect(() => {
    const list = items();
    const known = runs.value();
    if (!list || !known) return;
    for (const id of candidateIds(list, known, statuses(), startStatus())) {
      const version = list.find((i) => i.id === id)?.updated_at ?? '';
      if (requested.get(id) === version) continue;
      requested.set(id, version);
      if (!waiting.includes(id)) waiting.push(id);
    }
    pump();
  });

  const ready = () => !!(items() && runs.value() && teams.value() && profiles.value() && routing.value());
  const flow = createMemo<Flow | null>(() => {
    if (!ready()) return null;
    const d = details();
    return buildFlow({
      items: items()!,
      routing: new Map(routing.value()!.map((r) => [r.item_id, r])),
      blockers: new Map([...d].map(([id, v]) => [id, v.blockers])),
      runs: runs.value()!,
      profiles: profiles.value()!,
      teams: teams.value()!,
      projectRoles: (roles.value() ?? []).map((r) => ({ name: r.name, profileId: r.agent_profile_id ?? null })),
      defaultProfileId: project()?.default_profile_id ?? null,
      statuses: statuses(),
      startStatus: startStatus(),
      autoStart: !!project()?.auto_start_enabled,
      autonomy: autonomy.value() ? { level: autonomy.value()!.level, teams: autonomy.value()!.teams ?? [] } : null,
    });
  });

  const [mode, setModeSignal] = createSignal<Mode>(readMode());
  // What the Map sent Flow to, by the id of its box, read again from the map each time it changes.
  const [scopeId, setScopeId] = createSignal<string | null>(null);
  const setScope = (id: string | null) => setScopeId(id);
  const setMode = (next: Mode) => {
    setModeSignal(next);
    saveMode(next);
    if (next === 'map') setScope(null);
  };
  // A box chosen on the Map opens Flow for this visit only: it does not become the mode left on.
  const showInFlow = (id: string) => {
    setScope(id);
    setModeSignal('flow');
  };

  const mapModel = createMemo(() => {
    const f = flow();
    if (!f) return null;
    return buildMap({
      flow: f,
      roles: (roles.value() ?? []).map((r) => ({ name: r.name, profileId: r.agent_profile_id ?? null })),
      profiles: profiles.value() ?? [],
      teams: teams.value() ?? [],
      defaultProfileId: project()?.default_profile_id ?? null,
      runners: runners.value() ?? null,
    });
  });
  const scope = createMemo<FlowScope | null>(() => {
    const id = scopeId();
    const model = mapModel();
    return id && model ? scopeOfNode(model, id) : null;
  });
  const compareModel = createMemo(() => {
    const f = flow();
    if (!f) return null;
    const a = autonomy.value();
    return buildCompare({
      flow: f,
      teams: teams.value() ?? [],
      profiles: profiles.value() ?? [],
      autonomy: a ? { level: a.level, teams: a.teams ?? [], provisionalTokensPerRun: a.budgets?.provisional_tokens_per_run ?? null } : null,
    });
  });

  const [filters, setFilters] = createSignal<ReadonlySet<FlowFilter>>(new Set());
  const toggleFilter = (filter: FlowFilter) =>
    setFilters((prior) => {
      const next = new Set(prior);
      if (!next.delete(filter)) next.add(filter);
      return next;
    });

  // What moved since the last picture: said once, politely, and marked so it arrives with a transition.
  const [announcement, setAnnouncement] = createSignal('');
  const [arrived, setArrived] = createSignal<ReadonlySet<string>>(new Set<string>());
  let before: Map<string, string> | null = null;
  createEffect(() => {
    const f = flow();
    if (!f) return;
    const after = placements(f);
    if (before) {
      const moves = movesBetween(before, after);
      if (moves.length > 0) {
        const ids = [...after].filter(([id, sentence]) => before!.get(id) !== sentence).map(([id]) => id);
        setAnnouncement(moves.length > 3 ? `${moves.slice(0, 3).join('. ')}. And ${moves.length - 3} more` : moves.join('. '));
        setArrived(new Set<string>(ids));
        setTimeout(() => setArrived(new Set<string>()), ARRIVAL_MS);
      }
    }
    before = after;
  });

  const computers = () => {
    const list = runners.value();
    if (!list) return null;
    const active = list.filter((r) => r.state === 'active');
    return { count: active.length, capacity: active.reduce((sum, r) => sum + r.total_capacity, 0) };
  };
  const capacity = () =>
    capacityText({
      open: flow()?.open ?? 0,
      maxOpen: project()?.auto_start_max_open ?? null,
      autoStart: !!project()?.auto_start_enabled,
      computers: computers(),
    });

  const [dragged, setDragged] = createSignal<MoveSource | null>(null);
  const [highlight, setHighlight] = createSignal<string | null>(null);

  const saveProfile = async (itemId: string, profileId: string | null) => {
    const item = items()?.find((i) => i.id === itemId);
    await api.items.update(itemId, { run_settings: withProfile(item?.run_settings, profileId) });
    await refetch();
  };

  const itemsById = createMemo(() => new Map((items() ?? []).map((i) => [i.id, i])));

  const ui: FlowUi = {
    now,
    projectId,
    byId: itemsById,
    lanes: () => flow()?.lanes ?? [],
    highlight,
    setHighlight,
    arrived,
    dragged,
    setDragged,
    profiles: () => profiles.value() ?? [],
    teams: () => teams.value() ?? [],
    reports: () =>
      (runners.value() ?? []).flatMap((r) => {
        const harnesses = (r.capability_snapshot as { harnesses?: unknown } | null | undefined)?.harnesses;
        return Array.isArray(harnesses) ? (harnesses as HarnessCapability[]) : [];
      }),
    open: (itemId, run) =>
      setSearchParams({ item: itemId, tab: 'execution', run: run?.state === 'waiting_decision' ? run.request_id : undefined }),
    requestMove: (source, lane) => {
      void (async () => {
        const target = moveTarget(source, lane);
        if (!target.ok) {
          toast.error(target.reason);
          return;
        }
        const also = lane.profiles.length > 1 ? ` Other profiles of this lane stay available from the task's Automation row.` : '';
        const ok = await ask({
          title: 'Send this task to another lane?',
          message: `Who does it changes to ${target.profile.name}, in the lane ${lane.name}, for "${source.title}". Nothing starts because of this.${also}`,
          confirmLabel: 'Send',
        });
        if (!ok) return;
        try {
          await saveProfile(source.id, target.profile.agent_profile_id);
          toast.success(`"${source.title}" goes to ${lane.name}`);
        } catch (err) {
          toast.error(err instanceof Error ? err.message : 'Could not save who does this task.');
        }
      })();
    },
    chooseProfile: (itemId, profile) => {
      void saveProfile(itemId, profile ? profile.agent_profile_id : null).catch((err) =>
        toast.error(err instanceof Error ? err.message : 'Could not save who does this task.'),
      );
    },
  };

  const failed = () => (!flow() ? (runs.error() ?? teams.error() ?? profiles.error()) : undefined);

  return (
    <FlowUiContext.Provider value={ui}>
      <section class="flex min-w-0 flex-col gap-4" data-testid="orchestration">
        <div class="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2" data-testid="orchestration-header">
          <h2 class="text-lead font-bold" style={{ color: 'var(--color-text-primary)' }}>{t('orchestration')}</h2>
          <div role="group" aria-label="Mode" data-testid="orchestration-mode" class="inline-flex items-center gap-1">
            <For each={['flow', 'map'] as const}>
              {(m) => (
                <button
                  type="button"
                  aria-pressed={mode() === m}
                  data-testid={`orchestration-mode-${m}`}
                  class="rounded-pill px-3 py-0.5 text-small font-bold focus:outline-none focus-visible:ring-2"
                  style={{
                    'background-color': mode() === m ? 'var(--color-accent-soft)' : 'transparent',
                    color: mode() === m ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
                    border: 0,
                    cursor: 'pointer',
                  }}
                  onClick={() => {
                    if (m === 'flow') setScope(null);
                    setMode(m);
                  }}
                >
                  {m === 'flow' ? 'Flow' : 'Map'}
                </button>
              )}
            </For>
          </div>
        </div>

        <Show
          when={hasRun() !== false}
          fallback={
            <div data-testid="orchestration-empty">
              <EmptyState
                title={NO_AGENT_YET}
                description="Once a task has run, this view shows which team has it and where it has got to."
                action={
                  <A href="/agents" class="text-sm font-semibold underline" style={{ color: 'var(--color-accent-ink)' }}>
                    Open Agents
                  </A>
                }
              />
            </div>
          }
        >
          <Show
            when={flow()}
            fallback={
              <Show
                when={failed()}
                fallback={
                  <div class="flex flex-col gap-3" aria-busy="true" aria-label="Loading the lanes">
                    <Skeleton height="2rem" width="60%" />
                    <Skeleton height="8rem" />
                  </div>
                }
              >
                {(error) => <LoadError what="the lanes" error={error()} onRetry={() => { reload(); void profiles.load(); }} />}
              </Show>
            }
          >
            {(f) => (
              <Show
                when={mode() === 'flow'}
                fallback={
                  <Show when={mapModel() && compareModel()}>
                    <div class="flex min-w-0 flex-col gap-4" data-testid="orchestration-map-mode">
                      <MapView model={mapModel()!} projectId={projectId()} onSelect={showInFlow} onApplied={reload} />
                      <Compare model={compareModel()!} />
                    </div>
                  </Show>
                }
              >
                <Show when={scope()}>
                  {(chosen) => (
                    <div class="flex min-w-0 flex-wrap items-center gap-2 text-small" data-testid="flow-scope" style={{ color: 'var(--color-text-secondary)' }}>
                      <span style={{ 'overflow-wrap': 'anywhere' }}>{`Showing what passes through ${chosen().label}.`}</span>
                      <button
                        type="button"
                        data-testid="flow-scope-clear"
                        class="font-semibold underline focus:outline-none focus-visible:ring-2"
                        style={{ color: 'var(--color-accent-ink)', background: 'none', border: 0, cursor: 'pointer', padding: 0 }}
                        onClick={() => setScope(null)}
                      >
                        Show everything
                      </button>
                    </div>
                  )}
                </Show>
                <Summary flow={f()} active={filters()} onToggle={toggleFilter} capacity={capacity()} />
                <FlowView flow={f()} filters={filters()} scope={scope()} dailyLimit={autonomy.value()?.budgets?.tokens_per_day ?? null} />
              </Show>
            )}
          </Show>
        </Show>
        <div role="status" aria-live="polite" data-testid="flow-live" class="sr-only">{announcement()}</div>
        <ConfirmHost />
      </section>
    </FlowUiContext.Provider>
  );
};

export default Orchestration;
