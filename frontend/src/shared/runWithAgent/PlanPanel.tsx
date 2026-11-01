import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Button } from '../ui';
import { artifactsApi } from '../execution';
import { api } from '../api';
import { request } from '../api/client';
import type { components } from '../api/schema.gen';
import PlanTree, { EMPTY_OFFER, loadOffer, parentOf, plural, treeSummary, unticked, type Offer, type Row } from './PlanTree';

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

/** A run's plan (its request, attempt and artifact), or a file the person picked (its entries). `team` names the run's team, when it has one. */
export type PlanPanelProps = { itemId: string; team?: string } & (
  | { requestId: string; attemptNumber: number; artifactId: string }
  | { file: PlanEntry[] }
);

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
        return await loadOffer(projectId);
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
  const chosen = () => rows().filter((r) => r.selected).length;
  const summary = () => treeSummary(rows());

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
              <PlanTree rows={rows} offer={ctx} patch={patch} setTicked={setTicked} />
              <Button size="sm" data-testid="plan-create" disabled={busy() || chosen() === 0} onClick={() => void create()}>
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
