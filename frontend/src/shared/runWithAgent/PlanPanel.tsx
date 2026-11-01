import { type Component, type JSX, For, Show, createResource, createSignal } from 'solid-js';
import { Button } from '../ui';
import { agentProfilesApi, artifactsApi } from '../execution';
import type { AgentProfileSummary } from '../execution/api';
import { api } from '../api';
import { request } from '../api/client';
import type { components } from '../api/schema.gen';
import { getItemTypeList } from '../vocab/vocab';
import WhoDoesIt, { type TeamEntry } from './WhoDoesIt';

type CreateBody = components['schemas']['CreateSubtasksFromPlanRequest'];
type CreateResponse = components['schemas']['CreateSubtasksFromPlanResponse'];
type CreatedSubtask = components['schemas']['CreatedSubtask'];

/** One entry of a plan (plan-v1): the keys this panel reads. */
export interface PlanEntry {
  title: string;
  description: string;
  acceptance: string[];
  depends_on: number[];
  estimate?: string;
  type?: string;
  role?: string;
  team?: string;
  parent?: number;
}

interface Row {
  title: string;
  description: string;
  acceptance: string[];
  depends_on: number[];
  estimate?: string;
  type: string | null;
  role: string | null;
  team: string | null;
  parent: number | null;
  selected: boolean;
}

/** What the project offers the entries' controls: its types and roles, its profiles and the teams they name. */
interface Offer {
  types: Array<{ value: string; label: string }>;
  roles: Array<{ value: string; label: string }>;
  profiles: AgentProfileSummary[];
  teams: TeamEntry[];
}

/** A run's plan (its request, attempt and artifact), or a file the person picked (its entries). `team` names the run's team, when it has one. */
export type PlanPanelProps = { itemId: string; team?: string } & (
  | { requestId: string; attemptNumber: number; artifactId: string }
  | { file: PlanEntry[] }
);

const inputStyle = {
  border: '1px solid var(--color-border-light)',
  color: 'var(--color-text-primary)',
  background: 'transparent',
} as const;

const chipStyle = {
  border: '1px solid var(--color-border-light)',
  color: 'var(--color-text-primary)',
  'background-color': 'var(--color-bg-subtle)',
} as const;

const toRow = (e: PlanEntry): Row => ({
  title: e.title,
  description: e.description ?? '',
  acceptance: e.acceptance ?? [],
  depends_on: e.depends_on ?? [],
  estimate: e.estimate,
  type: e.type ?? null,
  role: e.role ?? null,
  team: e.team ?? null,
  parent: e.parent ?? null,
  selected: true,
});

/**
 * The entry an entry is nested under, or null for the top level. Only an earlier entry counts, so a
 * parent that points forward or at itself cannot hide an entry or loop the tree.
 */
function parentOf(rs: Row[], i: number): number | null {
  const p = rs[i].parent;
  return p !== null && p < i ? p : null;
}

/** The entries directly under `parent` (null for the top level), in the order they are listed. */
const childrenOf = (rs: Row[], parent: number | null): number[] => rs.flatMap((_, j) => (parentOf(rs, j) === parent ? [j] : []));

/** The entries an entry waits on that are in the list; never itself. */
const waitsOn = (rs: Row[], i: number): number[] => rs[i].depends_on.filter((d) => d !== i && d < rs.length);

/** Whether every entry above an entry is ticked, so it can be sent under them. */
function ancestorsTicked(rs: Row[], i: number): boolean {
  const p = parentOf(rs, i);
  return p === null || (rs[p].selected && ancestorsTicked(rs, p));
}

/** Unticks an entry and every entry nested under it. */
function unticked(rs: Row[], i: number): Row[] {
  const off = new Set<number>([i]);
  // A parent is always earlier than its children, so one pass in order reaches every descendant.
  rs.forEach((_, j) => {
    const p = parentOf(rs, j);
    if (p !== null && off.has(p)) off.add(j);
  });
  return rs.map((r, j) => (off.has(j) ? { ...r, selected: false } : r));
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** An option list that still shows a value the project does not have, so the file's own word stays visible. */
function withCurrent(options: Array<{ value: string; label: string }>, current: string | null) {
  if (!current || options.some((o) => o.value === current)) return options;
  return [{ value: current, label: `${current} (not in this project)` }, ...options];
}

const EMPTY_OFFER: Offer = { types: getItemTypeList(undefined), roles: [], profiles: [], teams: [] };

/**
 * The plan a Planner or a team proposed, or a file the person picked: tick, edit, set each entry's
 * type, role and who does it, and create the chosen ones. Nothing is created until the create button is pressed.
 */
const PlanPanel: Component<PlanPanelProps> = (props) => {
  const fromFile = () => 'file' in props;
  const [rows, setRows] = createSignal<Row[]>(fromFile() ? (props as { file: PlanEntry[] }).file.map(toRow) : []);
  const [created, setCreated] = createSignal<CreatedSubtask[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [loaded] = createResource(
    () => (fromFile() ? null : `${(props as { requestId: string }).requestId}:${(props as { attemptNumber: number }).attemptNumber}:${(props as { artifactId: string }).artifactId}`),
    async () => {
      const run = props as { requestId: string; attemptNumber: number; artifactId: string };
      try {
        const plan = JSON.parse(await (await artifactsApi.download(run.requestId, run.attemptNumber, run.artifactId)).text()) as {
          subtasks?: PlanEntry[];
        };
        setRows((plan.subtasks ?? []).map(toRow));
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Could not read the plan.');
      }
      return true;
    },
  );
  // A failed load leaves the controls with only what each entry already names.
  const [offer] = createResource(
    () => props.itemId,
    async (itemId): Promise<Offer> => {
      try {
        // `GET /items/{id}` answers `{ item, roles, dependencies }`: the project is the item's.
        const projectId = (await request<{ item: { project_id: string } }>(`/items/${itemId}`)).item.project_id;
        const [project, roles, profiles, teams] = await Promise.all([
          api.projects.get(projectId).catch(() => null),
          api.roles.list(projectId).catch(() => []),
          agentProfilesApi.list().then((r) => r.data.data ?? []).catch(() => [] as AgentProfileSummary[]),
          request<{ teams: TeamEntry[] }>('/teams').then((r) => r.teams).catch(() => [] as TeamEntry[]),
        ]);
        return {
          types: getItemTypeList(project?.vocabulary ?? undefined).map((t) => ({ value: t.value, label: t.label })),
          roles: roles.map((role) => ({ value: role.name, label: role.name })),
          profiles,
          teams,
        };
      } catch {
        return EMPTY_OFFER;
      }
    },
  );
  const ctx = (): Offer => offer() ?? EMPTY_OFFER;
  const patch = (i: number, change: Partial<Row>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...change } : r)));
  const setTicked = (i: number, on: boolean) =>
    setRows((rs) => (on ? rs.map((r, j) => (j === i ? { ...r, selected: true } : r)) : unticked(rs, i)));
  const selectAll = (on: boolean) => setRows((rs) => rs.map((r) => ({ ...r, selected: on })));
  const toggleWait = (i: number, other: number, checked: boolean) =>
    patch(i, {
      depends_on: checked ? [...rows()[i].depends_on, other] : rows()[i].depends_on.filter((d) => d !== other),
    });
  const chosen = () => rows().filter((r) => r.selected).length;
  const titleOf = (j: number) => rows()[j].title.trim() || 'Untitled item';
  const nestedCount = () => rows().filter((_, i) => parentOf(rows(), i) !== null).length;
  const dependencyCount = () => rows().reduce((n, _, i) => n + waitsOn(rows(), i).length, 0);
  const summary = () => {
    const parts = [plural(rows().length, 'item', 'items')];
    if (nestedCount() > 0) parts.push(plural(nestedCount(), 'sub-item', 'sub-items'));
    if (dependencyCount() > 0) parts.push(plural(dependencyCount(), 'dependency', 'dependencies'));
    return parts.join(' · ');
  };

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      // The request names entries by their place in the list it sends: unticked entries are left out,
      // and so is any reference to one.
      const sentAt = new Map<number, number>();
      rows().forEach((r, i) => {
        if (r.selected) sentAt.set(i, sentAt.size);
      });
      const subtasks = rows().flatMap((r, i) => {
        const p = parentOf(rows(), i);
        return r.selected
          ? [
              {
                title: r.title,
                description: r.description,
                acceptance: r.acceptance.filter((a) => a.trim() !== ''),
                depends_on: r.depends_on.filter((d) => sentAt.has(d)).map((d) => sentAt.get(d)!),
                type: r.type,
                role: r.role,
                team: r.team,
                parent: p !== null && sentAt.has(p) ? sentAt.get(p)! : null,
              },
            ]
          : [];
      });
      const body: CreateBody = fromFile() ? { subtasks } : { artifact_id: (props as { artifactId: string }).artifactId, subtasks };
      const res: CreateResponse = await api.items.createSubtasksFromPlan(props.itemId, body);
      setCreated(res.created);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not create the items.');
    } finally {
      setBusy(false);
    }
  };

  const notes = (c: CreatedSubtask): string[] =>
    [
      c.unknown_role ? `${c.title}: the project has no role "${c.unknown_role}", so it has none.` : null,
      c.unknown_type ? `${c.title}: the project has no type "${c.unknown_type}", so it took the parent's type.` : null,
      c.unresolved_team
        ? `${c.title}: no single profile names the team "${c.unresolved_team}", so nobody is set to do it.`
        : null,
    ].filter((n): n is string => n !== null);

  const title = () => {
    const n = rows().length;
    const who = fromFile() ? 'This file proposes' : props.team ? `The team ${props.team} proposes` : 'The Planner proposes';
    return `${who} ${n} ${n === 1 ? 'item' : 'items'}`;
  };

  /** One entry's card; the entries nested under it are listed beneath it by `branch`. */
  const entry = (i: number) => {
    const r = () => rows()[i];
    const others = () => rows().map((_, j) => j).filter((j) => j !== i && !waitsOn(rows(), i).includes(j));
    return (
      <div data-testid="plan-row" class="min-w-0 space-y-2 rounded-item p-2" style={{ border: '1px solid var(--color-border-light)' }}>
        <label class="flex min-w-0 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={r().selected}
            disabled={!ancestorsTicked(rows(), i)}
            aria-label={`Create ${r().title}`}
            onChange={(e) => setTicked(i, e.currentTarget.checked)}
          />
          <input
            class="min-w-0 flex-1 rounded px-2 py-1"
            style={inputStyle}
            aria-label="Title"
            value={r().title}
            onInput={(e) => patch(i, { title: e.currentTarget.value })}
          />
        </label>
        <input
          class="w-full min-w-0 rounded px-2 py-1 text-sm"
          style={inputStyle}
          aria-label="Description"
          value={r().description}
          onInput={(e) => patch(i, { description: e.currentTarget.value })}
        />
        <textarea
          class="w-full min-w-0 rounded px-2 py-1 text-sm"
          style={inputStyle}
          aria-label="Acceptance criteria, one per line"
          rows={Math.max(2, r().acceptance.length)}
          value={r().acceptance.join('\n')}
          onInput={(e) => patch(i, { acceptance: e.currentTarget.value.split('\n') })}
        />
        <div class="flex min-w-0 flex-wrap items-center gap-2 text-xs">
          <select
            aria-label="Type"
            class="max-w-full rounded px-2 py-1 text-xs"
            style={inputStyle}
            onChange={(e) => patch(i, { type: e.currentTarget.value || null })}
          >
            <option value="" selected={!r().type}>
              Default
            </option>
            <For each={withCurrent(ctx().types, r().type)}>
              {(t) => (
                <option value={t.value} selected={r().type === t.value}>
                  {t.label}
                </option>
              )}
            </For>
          </select>
          <select
            aria-label="Role"
            class="max-w-full rounded px-2 py-1 text-xs"
            style={inputStyle}
            onChange={(e) => patch(i, { role: e.currentTarget.value || null })}
          >
            <option value="" selected={!r().role}>
              No role
            </option>
            <For each={withCurrent(ctx().roles, r().role)}>
              {(role) => (
                <option value={role.value} selected={r().role === role.value}>
                  {role.label}
                </option>
              )}
            </For>
          </select>
          {/* The profile reads as a chip: the picker keeps its own border inside the pill. */}
          <span class="inline-flex max-w-full items-center rounded-full px-1" style={chipStyle}>
            <WhoDoesIt
              label="Who does it"
              value={r().team}
              profiles={ctx().profiles}
              teams={ctx().teams}
              onChange={(team) => patch(i, { team })}
            />
          </span>
          <Show when={r().estimate}>
            {(estimate) => (
              <span style={{ color: 'var(--color-text-tertiary)' }}>{estimate()}</span>
            )}
          </Show>
        </div>
        <fieldset class="min-w-0 space-y-1">
          <legend class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Waits on
          </legend>
          <ul class="flex min-w-0 flex-wrap items-center gap-1 text-xs">
            <For each={waitsOn(rows(), i)}>
              {(j) => (
                <li
                  class="flex max-w-full items-center gap-1 rounded-full py-0.5 pl-2.5 pr-1"
                  style={chipStyle}
                >
                  <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>
                    {titleOf(j)}
                  </span>
                  <button
                    type="button"
                    aria-label={`Stop waiting on ${titleOf(j)}`}
                    class="grid h-4 w-4 shrink-0 place-items-center rounded-full leading-none focus-visible:ring-2"
                    style={{ color: 'var(--color-text-secondary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                    onClick={() => toggleWait(i, j, false)}
                  >
                    <span aria-hidden="true">×</span>
                  </button>
                </li>
              )}
            </For>
            <Show when={others().length > 0}>
              <li>
                <select
                  aria-label={`Add a wait for ${r().title.trim() || 'this item'}`}
                  class="max-w-full rounded px-2 py-0.5 text-xs"
                  style={inputStyle}
                  onChange={(e) => {
                    // The menu is a way to add one more wait, so it goes back to its first line after each pick.
                    const v = e.currentTarget.value;
                    e.currentTarget.value = '';
                    if (v !== '') toggleWait(i, Number(v), true);
                  }}
                >
                  <option value="">Add an item…</option>
                  <For each={others()}>{(j) => <option value={j}>{titleOf(j)}</option>}</For>
                </select>
              </li>
            </Show>
          </ul>
        </fieldset>
      </div>
    );
  };

  /**
   * The entries nested under `parent` (null for the top level), each followed by its own. The guide
   * line is the left border of the nested list, so the depth is the list's structure, not a margin.
   */
  const branch = (parent: number | null): JSX.Element => (
    <ul
      aria-label={parent === null ? 'Proposed items' : `Sub-items of ${titleOf(parent)}`}
      class={parent === null ? 'space-y-2' : 'ml-3 mt-2 space-y-2 pl-3'}
      style={parent === null ? undefined : { 'border-left': '1px solid var(--color-border-medium)' }}
    >
      {/* Keyed by position: a patch replaces the row's object, and the row's DOM (and focus) stays. */}
      <For each={childrenOf(rows(), parent)}>
        {(i) => (
          <li class="min-w-0">
            {entry(i)}
            <Show when={childrenOf(rows(), i).length > 0}>{branch(i)}</Show>
          </li>
        )}
      </For>
    </ul>
  );

  return (
    <Show when={(fromFile() || loaded()) && (rows().length > 0 || error())}>
      <section data-testid="plan-panel" class="min-w-0 space-y-2">
        <Show
          when={created()}
          fallback={
            <>
              <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
                {title()}
              </h4>
              <div class="flex flex-wrap items-center justify-between gap-2">
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                  {summary()}
                </p>
                <div class="flex items-center gap-1">
                  <Button size="sm" variant="ghost" disabled={chosen() === rows().length} onClick={() => selectAll(true)}>
                    Select all
                  </Button>
                  <Button size="sm" variant="ghost" disabled={chosen() === 0} onClick={() => selectAll(false)}>
                    Select none
                  </Button>
                </div>
              </div>
              {branch(null)}
              <Button size="sm" disabled={busy() || chosen() === 0} onClick={() => void create()}>
                {`Create ${plural(chosen(), 'item', 'items')}`}
              </Button>
            </>
          }
        >
          {(list) => (
            <>
              <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
                Created {list().length} {list().length === 1 ? 'item' : 'items'}
              </h4>
              <ul class="text-sm">
                <For each={list()}>{(c) => <li><a href={`?item=${c.id}`}>{c.title}</a></li>}</For>
              </ul>
              <For each={list().flatMap(notes)}>
                {(note) => <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>{note}</p>}
              </For>
            </>
          )}
        </Show>
        <Show when={error()}>
          <p class="text-sm" role="alert" style={{ color: 'var(--color-danger-600)' }}>{error()}</p>
        </Show>
      </section>
    </Show>
  );
};

export default PlanPanel;
