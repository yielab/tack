import { type Accessor, type Component, type JSX, For, Show, createSignal, createUniqueId } from 'solid-js';
import { Button } from '../ui';
import { agentProfilesApi } from '../execution';
import type { AgentProfileSummary } from '../execution/api';
import { api } from '../api';
import { request } from '../api/client';
import WhoDoesIt, { type TeamEntry } from './WhoDoesIt';

// The tree of proposed items, shared by the two panels that show one: the plan a run proposed
// (PlanPanel) and the creates of a change set (ChangeSetPanel). It draws rows and reports edits;
// what a row means for the request that creates it stays with the panel that owns it.

/** One proposed item as the tree draws it. */
export interface Row {
  title: string;
  description: string;
  acceptance: string[];
  /** Places in the list of the rows this one waits on. */
  depends_on: number[];
  estimate?: string;
  type: string | null;
  role: string | null;
  team: string | null;
  /** The place in the list of the row this one is nested under, or null for the top level. */
  parent: number | null;
  selected: boolean;
  /** Set when the row can no longer change (it was applied or turned down): what to say instead of a tick. */
  locked?: string;
}

/** What the project offers the entries' controls: its types and roles, its profiles and the teams they name. */
export interface Offer {
  types: Array<{ value: string; label: string }>;
  roles: Array<{ value: string; label: string }>;
  profiles: AgentProfileSummary[];
  teams: TeamEntry[];
  /** The names of the project's statuses, in the board's order. */
  statuses: string[];
  /** The project's own types were read, so a type missing from them is really not there. */
  ready?: boolean;
}

export const EMPTY_OFFER: Offer = { types: [], roles: [], profiles: [], teams: [], statuses: [] };

/** Reads what a project offers; a part that fails to load leaves its controls with only what each entry already names. */
export async function loadOffer(projectId: string): Promise<Offer> {
  const [project, roles, profiles, teams] = await Promise.all([
    api.projects.get(projectId).catch(() => null),
    api.roles.list(projectId).catch(() => []),
    agentProfilesApi.list().then((r) => r.data.data ?? []).catch(() => [] as AgentProfileSummary[]),
    request<{ teams: TeamEntry[] }>('/teams').then((r) => r.teams).catch(() => [] as TeamEntry[]),
  ]);
  return {
    types: (project?.item_types ?? []).map((t) => ({ value: t.key, label: t.name })),
    roles: roles.map((role) => ({ value: role.name, label: role.name })),
    profiles,
    teams,
    statuses: [...(project?.workflow?.statuses ?? [])].sort((a, b) => a.order - b.order).map((s) => s.name),
    ready: project !== null,
  };
}

export const inputStyle = {
  border: '1px solid var(--color-border-light)',
  color: 'var(--color-text-primary)',
  background: 'transparent',
} as const;

const chipStyle = {
  border: '1px solid var(--color-border-light)',
  color: 'var(--color-text-primary)',
  'background-color': 'var(--color-bg-subtle)',
} as const;

/**
 * The entry an entry is nested under, or null for the top level. Only an earlier entry counts, so a
 * parent that points forward or at itself cannot hide an entry or loop the tree.
 */
export function parentOf(rs: Row[], i: number): number | null {
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
  // A parent that was already decided does not hold its children back.
  return p === null || ((rs[p].selected || !!rs[p].locked) && ancestorsTicked(rs, p));
}

/** Unticks an entry and every entry nested under it. */
export function unticked(rs: Row[], i: number): Row[] {
  const off = new Set<number>([i]);
  // A parent is always earlier than its children, so one pass in order reaches every descendant.
  rs.forEach((_, j) => {
    const p = parentOf(rs, j);
    if (p !== null && off.has(p)) off.add(j);
  });
  return rs.map((r, j) => (off.has(j) && !r.locked ? { ...r, selected: false } : r));
}

export const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The header line of a tree: "3 items · 1 sub-item · 1 dependency". */
export function treeSummary(rs: Row[]): string {
  const nested = rs.filter((_, i) => parentOf(rs, i) !== null).length;
  const deps = rs.reduce((n, _, i) => n + waitsOn(rs, i).length, 0);
  const parts = [plural(rs.length, 'item', 'items')];
  if (nested > 0) parts.push(plural(nested, 'sub-item', 'sub-items'));
  if (deps > 0) parts.push(plural(deps, 'dependency', 'dependencies'));
  return parts.join(' · ');
}

/**
 * The project's type a row's word names: its key, or its name in any case (a team writes "Feature" where the catalog keys
 * it "feature"; the server takes either, as it makes the same key of the word). Undefined when the project has no such type.
 */
export function typeIn(types: Offer['types'], word: string | null): string | undefined {
  if (!word) return undefined;
  const key = word.trim().toLowerCase().replace(/[^a-z0-9_]+/g, '-').replace(/^-+|-+$/g, '');
  return types.find((t) => t.value === word || t.value === key || t.label.toLowerCase() === word.trim().toLowerCase())?.value;
}

/** An option list that still shows a value the project does not have, so the file's own word stays visible. */
export function withCurrent(options: Array<{ value: string; label: string }>, current: string | null) {
  if (!current || options.some((o) => o.value === current)) return options;
  return [{ value: current, label: `${current} (not in this project)` }, ...options];
}

/** A control under the words that name it, so no field of an entry is left to its placeholder or its position. */
const Field: Component<{ id: string; label: string; class?: string; children: JSX.Element }> = (props) => (
  <div class={`flex min-w-0 flex-col gap-0.5 ${props.class ?? ''}`}>
    <label for={props.id} data-field-label class="text-small font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
      {props.label}
    </label>
    {props.children}
  </div>
);

/** A text area that grows with its text, so a long line is read whole and not cut at the edge. */
const grow = { 'field-sizing': 'content' } as const;

/**
 * The proposed items as a tree: checkbox, title, description, acceptance, type, role, who does it and
 * what each waits on. It changes nothing itself; every edit goes out through `patch` and `setTicked`.
 */
const PlanTree: Component<{
  rows: Accessor<Row[]>;
  offer: Accessor<Offer>;
  patch: (i: number, change: Partial<Row>) => void;
  setTicked: (i: number, on: boolean) => void;
}> = (props) => {
  const rows = props.rows;
  const ctx = props.offer;
  const patch = props.patch;
  const toggleWait = (i: number, other: number, checked: boolean) =>
    patch(i, {
      depends_on: checked ? [...rows()[i].depends_on, other] : rows()[i].depends_on.filter((d) => d !== other),
    });
  const titleOf = (j: number) => rows()[j].title.trim() || 'Untitled item';

  /** One entry's card; the entries nested under it are listed beneath it by `branch`. */
  const entry = (i: number) => {
    const r = () => rows()[i];
    const frozen = () => !!r().locked;
    const uid = createUniqueId();
    const fid = (name: string) => `plan-${uid}-${name}`;
    const [editing, setEditing] = createSignal(false);
    const missingDoneWhen = () => r().acceptance.every((a) => a.trim() === '');
    /** The row in one line: what it is, who does it, how many Done when. */
    const meta = () => {
      const done = r().acceptance.filter((a) => a.trim() !== '').length;
      const typeLabel = ctx().types.find((t) => t.value === typeValue())?.label ?? r().type;
      return [
        parentOf(rows(), i) !== null ? 'Sub-item' : null,
        typeLabel,
        r().role,
        r().team,
        `${done} Done when`,
        waitsOn(rows(), i).length > 0 ? `waits on ${waitsOn(rows(), i).map(titleOf).join(', ')}` : null,
      ].filter(Boolean).join(' · ');
    };
    const typeValue = () => typeIn(ctx().types, r().type) ?? r().type;
    const typeMissing = () => !!r().type && ctx().ready === true && !typeIn(ctx().types, r().type);
    const others = () => rows().map((_, j) => j).filter((j) => j !== i && !waitsOn(rows(), i).includes(j));
    return (
      <div data-testid="plan-row" class="min-w-0 py-3" style={{ 'border-bottom': '1px solid var(--color-border-light)' }}>
        <div class="flex min-w-0 items-start gap-3">
          <input
            type="checkbox"
            class="mt-1 h-4 w-4"
            checked={r().selected}
            disabled={frozen() || !ancestorsTicked(rows(), i)}
            aria-label={`Create ${r().title}`}
            onChange={(e) => props.setTicked(i, e.currentTarget.checked)}
          />
          <div class="min-w-0 flex-1">
            <p class="text-body font-semibold" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>{titleOf(i)}</p>
            <Show
              when={missingDoneWhen()}
              fallback={<p data-testid="plan-row-meta" class="text-small" style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}>{meta()}</p>}
            >
              <p data-testid="plan-row-meta" class="text-small" style={{ color: 'var(--color-danger-700)' }}>No Done when yet</p>
            </Show>
          </div>
          <Show when={r().locked}>
            {(why) => (
              <span data-testid="plan-row-state" class="shrink-0 text-small" style={{ color: 'var(--color-text-secondary)' }}>
                {why()}
              </span>
            )}
          </Show>
          <Button type="button" size="sm" variant="ghost" aria-expanded={editing()} aria-controls={fid('editor')} onClick={() => setEditing(!editing())}>
            Edit
          </Button>
        </div>
        <div id={fid('editor')} hidden={!editing()} class="mt-3 min-w-0 space-y-3 pl-8">
          <Field id={fid('title')} label="Title">
            <input
              id={fid('title')}
              class="min-w-0 rounded-md px-2 py-1"
              style={inputStyle}
              aria-label="Title"
              disabled={frozen()}
              value={r().title}
              onInput={(e) => patch(i, { title: e.currentTarget.value })}
            />
          </Field>
        <Field id={fid('description')} label="Description">
          <textarea
            id={fid('description')}
            class="w-full min-w-0 rounded-md px-2 py-1 text-small"
            style={{ ...inputStyle, ...grow }}
            aria-label="Description"
            disabled={frozen()}
            rows={2}
            value={r().description}
            onInput={(e) => patch(i, { description: e.currentTarget.value })}
          />
        </Field>
        <Field id={fid('done-when')} label="Done when (one line each)">
          <textarea
            id={fid('done-when')}
            class="w-full min-w-0 rounded-md px-2 py-1 text-small"
            style={{ ...inputStyle, ...grow }}
            aria-label="Done when (one line each)"
            disabled={frozen()}
            rows={Math.max(2, r().acceptance.length)}
            value={r().acceptance.join('\n')}
            onInput={(e) => patch(i, { acceptance: e.currentTarget.value.split('\n') })}
          />
        </Field>
        <div class="flex min-w-0 flex-wrap items-end gap-2 text-small">
          <Field id={fid('type')} label="Type">
            <select
              id={fid('type')}
              aria-label="Type"
              class="max-w-full rounded-md px-2 py-1 text-small"
              style={inputStyle}
              disabled={frozen()}
              onChange={(e) => patch(i, { type: e.currentTarget.value || null })}
            >
              <option value="" selected={!r().type}>
                Default
              </option>
              <For each={withCurrent(ctx().types, typeValue())}>
                {(t) => (
                  <option value={t.value} selected={typeValue() === t.value}>
                    {t.label}
                  </option>
                )}
              </For>
            </select>
          </Field>
          <Field id={fid('role')} label="Role">
            <select
              id={fid('role')}
              aria-label="Role"
              class="max-w-full rounded-md px-2 py-1 text-small"
              style={inputStyle}
              disabled={frozen()}
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
          </Field>
          <Field id={fid('who')} label="Who does it" class="max-w-full">
            {/* The profile reads as a chip: the picker keeps its own border inside the pill. */}
            <span class="inline-flex max-w-full items-center rounded-md px-1" style={chipStyle}>
              <WhoDoesIt
                id={fid('who')}
                label="Who does it"
                value={r().team}
                profiles={ctx().profiles}
                teams={ctx().teams}
                disabled={frozen()}
                onChange={(team) => patch(i, { team })}
              />
            </span>
          </Field>
          <Show when={r().estimate}>
            {(estimate) => (
              <span class="pb-1.5" style={{ color: 'var(--color-text-secondary)' }}>{estimate()}</span>
            )}
          </Show>
        </div>
        <Show when={typeMissing()}>
          <p data-testid="plan-type-note" class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            {`This project has no type "${r().type}": the item will be created with its parent's type, or the default work type when the parent holds work.`}
          </p>
        </Show>
        <fieldset class="min-w-0 space-y-1">
          <legend class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            Waits on
          </legend>
          <ul class="flex min-w-0 flex-wrap items-center gap-1 text-small">
            <For each={waitsOn(rows(), i)}>
              {(j) => (
                <li
                  class="flex max-w-full items-center gap-1 rounded-sm py-0.5 pl-2.5 pr-1"
                  style={chipStyle}
                >
                  <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>
                    {titleOf(j)}
                  </span>
                  <button
                    type="button"
                    aria-label={`Stop waiting on ${titleOf(j)}`}
                    disabled={frozen()}
                    class="grid h-4 w-4 shrink-0 place-items-center rounded-sm leading-none focus-visible:ring-2"
                    style={{ color: 'var(--color-text-secondary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                    onClick={() => toggleWait(i, j, false)}
                  >
                    <span aria-hidden="true">×</span>
                  </button>
                </li>
              )}
            </For>
            <Show when={others().length > 0 && !frozen()}>
              <li>
                <select
                  aria-label={`Add a wait for ${r().title.trim() || 'this item'}`}
                  class="max-w-full rounded-md px-2 py-0.5 text-small"
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
      class={parent === null ? '' : 'ml-6'}
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

  return <>{branch(null)}</>;
};

export default PlanTree;
