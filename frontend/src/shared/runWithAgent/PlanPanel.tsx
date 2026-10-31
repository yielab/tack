import { type Component, For, Show, createResource, createSignal } from 'solid-js';
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

/** Whether every entry above an entry is ticked, so it can be sent under them. */
function ancestorsTicked(rs: Row[], i: number): boolean {
  const p = rs[i].parent;
  return p === null || (rs[p].selected && ancestorsTicked(rs, p));
}

/** Unticks an entry and every entry nested under it. */
function unticked(rs: Row[], i: number): Row[] {
  const off = new Set<number>([i]);
  let grew = true;
  while (grew) {
    grew = false;
    rs.forEach((r, j) => {
      if (!off.has(j) && r.parent !== null && off.has(r.parent)) {
        off.add(j);
        grew = true;
      }
    });
  }
  return rs.map((r, j) => (off.has(j) ? { ...r, selected: false } : r));
}

const depthOf = (rs: Row[], i: number): number => (rs[i].parent === null ? 0 : 1 + depthOf(rs, rs[i].parent!));

/** An option list that still shows a value the project does not have, so the file's own word stays visible. */
function withCurrent(options: Array<{ value: string; label: string }>, current: string | null) {
  if (!current || options.some((o) => o.value === current)) return options;
  return [{ value: current, label: `${current} (not in this project)` }, ...options];
}

const EMPTY_OFFER: Offer = { types: getItemTypeList(undefined), roles: [], profiles: [], teams: [] };

/**
 * The plan a Planner or a team proposed, or a file the person picked: tick, edit, set each entry's
 * type, role and who does it, and create the chosen ones. Nothing is created until "Create selected".
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
        const detail = await request<{ project_id: string }>(`/items/${itemId}`);
        const [project, roles, profiles, teams] = await Promise.all([
          api.projects.get(detail.project_id).catch(() => null),
          api.roles.list(detail.project_id).catch(() => []),
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
  const toggleWait = (i: number, other: number, checked: boolean) =>
    patch(i, {
      depends_on: checked ? [...rows()[i].depends_on, other] : rows()[i].depends_on.filter((d) => d !== other),
    });
  const chosen = () => rows().filter((r) => r.selected).length;

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
      const subtasks = rows().flatMap((r) =>
        r.selected
          ? [
              {
                title: r.title,
                description: r.description,
                acceptance: r.acceptance.filter((a) => a.trim() !== ''),
                depends_on: r.depends_on.filter((d) => sentAt.has(d)).map((d) => sentAt.get(d)!),
                type: r.type,
                role: r.role,
                team: r.team,
                parent: r.parent !== null && sentAt.has(r.parent) ? sentAt.get(r.parent)! : null,
              },
            ]
          : [],
      );
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

  return (
    <Show when={(fromFile() || loaded()) && (rows().length > 0 || error())}>
      <section data-testid="plan-panel" class="space-y-2">
        <Show
          when={created()}
          fallback={
            <>
              <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
                {title()}
              </h4>
              {/* Keyed by position: a patch replaces the row's object, and the row's DOM (and focus) stays. */}
              <For each={rows().map((_, n) => n)}>
                {(i) => {
                  const r = () => rows()[i];
                  return (
                  <div
                    data-testid="plan-row"
                    class="space-y-1 rounded-[12px] p-2"
                    style={{ border: '1px solid var(--color-border-light)', 'margin-left': `${depthOf(rows(), i) * 1.5}rem` }}
                  >
                    <label class="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={r().selected}
                        disabled={!ancestorsTicked(rows(), i)}
                        aria-label={`Create ${r().title}`}
                        onChange={(e) => setTicked(i, e.currentTarget.checked)}
                      />
                      <input
                        class="flex-1 rounded px-2 py-1"
                        style={inputStyle}
                        aria-label="Title"
                        value={r().title}
                        onInput={(e) => patch(i, { title: e.currentTarget.value })}
                      />
                    </label>
                    <input
                      class="w-full rounded px-2 py-1 text-sm"
                      style={inputStyle}
                      aria-label="Description"
                      value={r().description}
                      onInput={(e) => patch(i, { description: e.currentTarget.value })}
                    />
                    <textarea
                      class="w-full rounded px-2 py-1 text-sm"
                      style={inputStyle}
                      aria-label="Acceptance criteria, one per line"
                      rows={Math.max(2, r().acceptance.length)}
                      value={r().acceptance.join('\n')}
                      onInput={(e) => patch(i, { acceptance: e.currentTarget.value.split('\n') })}
                    />
                    <div class="flex flex-wrap items-center gap-2 text-sm">
                      <select
                        aria-label="Type"
                        class="rounded px-2 py-1 text-sm"
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
                        class="rounded px-2 py-1 text-sm"
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
                      <WhoDoesIt
                        label="Who does it"
                        value={r().team}
                        profiles={ctx().profiles}
                        teams={ctx().teams}
                        onChange={(team) => patch(i, { team })}
                      />
                    </div>
                    <fieldset class="flex flex-wrap items-center gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                      <legend class="mr-1">Waits on</legend>
                      <For each={rows().map((_, n) => n)}>
                        {(j) => (
                          <Show when={j !== i}>
                            <label class="flex items-center gap-1">
                              <input
                                type="checkbox"
                                aria-label={`Waits on ${j + 1}`}
                                title={rows()[j].title}
                                checked={r().depends_on.includes(j)}
                                onChange={(e) => toggleWait(i, j, e.currentTarget.checked)}
                              />
                              {j + 1}
                            </label>
                          </Show>
                        )}
                      </For>
                    </fieldset>
                    <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      {r().estimate}
                    </p>
                  </div>
                  );
                }}
              </For>
              <Button size="sm" disabled={busy() || chosen() === 0} onClick={() => void create()}>
                Create selected
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
