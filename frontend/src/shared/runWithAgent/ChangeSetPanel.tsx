import { type Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { Button } from '../ui';
import { ApiError, api, request } from '../api';
import { type ChangeSet, type Operation, refusalReasons } from '../api/changeSets';
import PlanTree, { EMPTY_OFFER, inputStyle, loadOffer, plural, treeSummary, unticked } from './PlanTree';
import OperationEditor from './OperationEditor';
import {
  KIND_LABEL,
  createPositions,
  createRows,
  existingItems,
  isCreate,
  originText,
  proposerName,
  summarize,
  withRowChange,
} from './changeSetOps';
import { relativeTimeFromIso } from './shared';
import { shortItemId } from '../shortItemId';

const STATUS_TEXT: Record<ChangeSet['status'], string> = {
  waiting: 'Waiting for you',
  partly: 'Partly applied',
  applied: 'Applied',
  rejected: 'Turned down',
};

const OP_STATE_TEXT = { waiting: '', applied: 'Applied', rejected: 'Turned down' } as const;

/** What a refusal or a failure said, as the lines the panel lists. */
interface Problem {
  headline: string;
  reasons: string[];
}

const secondary = { color: 'var(--color-text-secondary)' } as const;

/**
 * One change set, whoever proposed it: the creates as the tree of items, every other operation as a
 * list, each editable and tickable, with the reason, the origin and the proposer above. Apply the
 * ticked operations or all of them, or turn the waiting ones down with a reason. A change set the
 * server refuses is refused whole, and the panel lists every reason it gave.
 */
const ChangeSetPanel: Component<{ changeSet: ChangeSet; onChange?: (next: ChangeSet) => void }> = (props) => {
  const [set, setSet] = createSignal<ChangeSet>(props.changeSet);
  /** Operations the person has edited and not yet saved, by position. */
  const [drafts, setDrafts] = createSignal<Record<number, Operation>>({});
  const waitingAt = (s: ChangeSet): Set<number> => new Set<number>(s.ops.flatMap((o, i) => ((o.state ?? 'waiting') === 'waiting' ? [i] : [])));
  const [ticked, setTicked] = createSignal<Set<number>>(waitingAt(props.changeSet));
  const [busy, setBusy] = createSignal(false);
  const [problem, setProblem] = createSignal<Problem | null>(null);
  const [rejecting, setRejecting] = createSignal(false);
  const [rejectReason, setRejectReason] = createSignal('');

  const ops = createMemo<Operation[]>(() => set().ops.map((o, i) => drafts()[i] ?? o.operation));
  const states = createMemo(() => set().ops.map((o) => o.state ?? 'waiting'));
  const waiting = createMemo(() => [...waitingAt(set())]);
  const dirty = () => Object.keys(drafts()).map(Number).filter((i) => states()[i] === 'waiting');
  const chosen = () => waiting().filter((i) => ticked().has(i));

  const [offer] = createResource(
    () => set().project,
    async (projectId) => {
      try {
        return await loadOffer(projectId);
      } catch {
        return EMPTY_OFFER;
      }
    },
  );
  const ctx = () => offer() ?? EMPTY_OFFER;

  // The titles of the existing items the operations name, read once for the set.
  const [titles] = createResource(
    () => existingItems(set().ops.map((o) => o.operation)),
    async (ids) => {
      const read = await Promise.all(
        ids.map((id) =>
          request<{ item: { title: string } }>(`/items/${encodeURIComponent(id)}`)
            .then((d): [string, string] => [id, d.item.title])
            .catch((): [string, string] => [id, '']),
        ),
      );
      return Object.fromEntries(read.filter(([, title]) => title !== ''));
    },
  );
  const nameOf = (ref: number | string): string => {
    if (typeof ref === 'number') {
      const target = ops()[ref];
      return target && isCreate(target) ? target.title.trim() || 'an untitled item' : `new item ${ref + 1}`;
    }
    return titles()?.[ref] ?? `item ${shortItemId(ref)}`;
  };
  const itemName = (op: Operation): string => {
    if (op.op === 'create') return op.title.trim() || 'untitled item';
    if (op.op === 'depend') return nameOf(op.blocked);
    return nameOf(op.item);
  };

  const positions = createMemo(() => createPositions(ops()));
  const rows = createMemo(() => createRows(ops(), states(), ticked(), ctx().profiles));
  const others = createMemo(() => ops().flatMap((op, i) => (isCreate(op) ? [] : [i])));

  const draft = (pos: number, next: Operation) => setDrafts((d) => ({ ...d, [pos]: next }));
  const patchRow = (local: number, change: Parameters<typeof withRowChange>[1]) => {
    const pos = positions()[local];
    draft(pos, withRowChange(ops()[pos] as Extract<Operation, { op: 'create' }>, change, positions()));
  };
  const tick = (pos: number, on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      if (on) next.add(pos);
      else next.delete(pos);
      return next;
    });
  const tickRow = (local: number, on: boolean) => {
    const after = on ? rows().map((r, j) => (j === local ? { ...r, selected: true } : r)) : unticked(rows(), local);
    setTicked((t) => {
      const next = new Set(t);
      after.forEach((r, j) => {
        if (states()[positions()[j]] !== 'waiting') return;
        if (r.selected) next.add(positions()[j]);
        else next.delete(positions()[j]);
      });
      return next;
    });
  };
  const tickAll = (on: boolean) => setTicked(on ? new Set<number>(waiting()) : new Set<number>());

  const settle = (next: ChangeSet) => {
    setSet(next);
    setDrafts({});
    setTicked(waitingAt(next));
    setRejecting(false);
    setRejectReason('');
    props.onChange?.(next);
  };

  /** Stores the edits that wait, in the order of the operations, so what is applied is what is on screen. */
  const saveEdits = async (): Promise<ChangeSet | null> => {
    let latest: ChangeSet | null = null;
    for (const pos of dirty().sort((a, b) => a - b)) {
      latest = await api.changeSets.edit(set().id, pos, drafts()[pos]);
    }
    return latest;
  };

  const fail = (e: unknown, what: string) => {
    if (e instanceof ApiError && e.status === 400) {
      const reasons = refusalReasons(e.message);
      setProblem({
        headline: reasons.length > 1 ? `Nothing was applied, for ${reasons.length} reasons:` : 'Nothing was applied:',
        reasons,
      });
    } else {
      setProblem({ headline: '', reasons: [e instanceof Error ? e.message : what] });
    }
  };

  const run = async (action: () => Promise<ChangeSet>, what: string) => {
    setBusy(true);
    setProblem(null);
    try {
      settle(await action());
    } catch (e) {
      fail(e, what);
    } finally {
      setBusy(false);
    }
  };

  const applyChosen = () =>
    run(async () => {
      const picked = chosen();
      await saveEdits();
      return api.changeSets.apply(set().id, picked);
    }, 'Could not apply the changes.');
  const applyAll = () =>
    run(async () => {
      await saveEdits();
      return api.changeSets.apply(set().id);
    }, 'Could not apply the changes.');
  const reject = () => run(() => api.changeSets.reject(set().id, rejectReason().trim()), 'Could not turn the changes down.');
  const saveOnly = async () => {
    setBusy(true);
    setProblem(null);
    try {
      const latest = await saveEdits();
      if (latest) {
        setSet(latest);
        setDrafts({});
        props.onChange?.(latest);
      }
    } catch (e) {
      fail(e, 'Could not save the edits.');
    } finally {
      setBusy(false);
    }
  };

  const title = () => {
    const n = ops().length;
    return `${proposerName(set())} proposes ${plural(n, 'change', 'changes')}`;
  };
  const noteFor = (pos: number): string[] => {
    const result = set().ops[pos]?.result;
    const name = (result?.title ?? itemName(ops()[pos])) || 'The item';
    return [
      result?.unknown_role ? `${name}: the project has no role "${result.unknown_role}", so it has none.` : null,
      result?.unknown_type ? `${name}: the project has no type "${result.unknown_type}", so it took the fallback type.` : null,
      result?.unresolved_team ? `${name}: no single profile names the team "${result.unresolved_team}", so nobody is set to do it.` : null,
    ].filter((n): n is string => n !== null);
  };
  const doneNotes = () => set().ops.flatMap((o, i) => (o.state === 'applied' ? noteFor(i) : []));
  const nothingWaits = () => waiting().length === 0;

  return (
    <section
      data-testid="change-set-panel"
      data-status={set().status}
      aria-label={title()}
      class="min-w-0 space-y-3 rounded-item p-3"
      style={{ border: '1px solid var(--color-border-light)' }}
    >
      <header class="min-w-0 space-y-1">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
            {title()}
          </h4>
          <span data-testid="change-set-status" class="rounded-full px-2.5 py-0.5 text-xs font-semibold" style={{ border: '1px solid var(--color-border-medium)', color: 'var(--color-text-primary)' }}>
            {STATUS_TEXT[set().status]}
          </span>
        </div>
        <p data-testid="change-set-reason" class="text-sm" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
          {set().reason}
        </p>
        <p data-testid="change-set-origin" class="text-xs" style={secondary}>
          {originText(set())} · proposed by {proposerName(set())} · {relativeTimeFromIso(set().created_at)}
        </p>
        <Show when={set().decided_at}>
          {(at) => (
            <p class="text-xs" style={secondary}>
              Decided{set().decided_by ? ` by ${set().decided_by}` : ''} {relativeTimeFromIso(at())}
              {set().rejection ? `: ${set().rejection}` : ''}
            </p>
          )}
        </Show>
      </header>

      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="text-sm" style={secondary}>
          {[
            positions().length > 0 ? treeSummary(rows()) : null,
            others().length > 0 ? plural(others().length, 'other change', 'other changes') : null,
          ]
            .filter(Boolean)
            .join(' · ')}
        </p>
        <div class="flex items-center gap-1">
          <Button size="sm" variant="ghost" disabled={nothingWaits() || chosen().length === waiting().length} onClick={() => tickAll(true)}>
            Select all
          </Button>
          <Button size="sm" variant="ghost" disabled={nothingWaits() || chosen().length === 0} onClick={() => tickAll(false)}>
            Select none
          </Button>
        </div>
      </div>

      <Show when={positions().length > 0}>
        <PlanTree rows={rows} offer={ctx} patch={patchRow} setTicked={tickRow} />
      </Show>

      <Show when={others().length > 0}>
        <ul aria-label="Other changes" class="space-y-2">
          <For each={others()}>
            {(pos) => {
              const op = () => ops()[pos];
              const locked = () => states()[pos] !== 'waiting';
              return (
                <li data-testid="change-op" data-kind={op().op} class="min-w-0 space-y-2 rounded-item p-2" style={{ border: '1px solid var(--color-border-light)' }}>
                  <label class="flex min-w-0 items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={ticked().has(pos)}
                      disabled={locked()}
                      aria-label={summarize(op(), nameOf)}
                      onChange={(e) => tick(pos, e.currentTarget.checked)}
                    />
                    <span class="rounded-full px-2 py-0.5 text-xs" style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}>
                      {KIND_LABEL[op().op]}
                    </span>
                    <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
                      {summarize(op(), nameOf)}
                    </span>
                    <Show when={locked()}>
                      <span data-testid="change-op-state" class="ml-auto shrink-0 text-xs" style={secondary}>
                        {OP_STATE_TEXT[states()[pos]]}
                      </span>
                    </Show>
                  </label>
                  <OperationEditor op={op()} name={itemName(op())} offer={ctx()} disabled={locked()} onEdit={(next) => draft(pos, next)} />
                </li>
              );
            }}
          </For>
        </ul>
      </Show>

      <For each={doneNotes()}>{(note) => <p class="text-sm" style={secondary}>{note}</p>}</For>

      <Show when={problem()}>
        {(p) => (
          <div data-testid="change-set-refusal" role="alert" class="space-y-1 text-sm" style={{ color: 'var(--color-danger-600)' }}>
            <Show when={p().headline}>
              <p>{p().headline}</p>
            </Show>
            <ul class={p().headline ? 'list-disc space-y-0.5 pl-5' : 'space-y-0.5'}>
              <For each={p().reasons}>{(reason) => <li>{reason}</li>}</For>
            </ul>
          </div>
        )}
      </Show>

      <Show when={rejecting()}>
        <div class="flex min-w-0 flex-wrap items-center gap-2">
          <input
            class="min-w-0 flex-1 rounded px-2 py-1 text-sm"
            style={inputStyle}
            aria-label="Why you turn these changes down"
            placeholder="Why? The proposer reads this."
            value={rejectReason()}
            onInput={(e) => setRejectReason(e.currentTarget.value)}
          />
          <Button size="sm" variant="danger" data-testid="change-set-reject-confirm" disabled={busy() || rejectReason().trim() === ''} onClick={() => void reject()}>
            Reject {plural(waiting().length, 'change', 'changes')}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setRejecting(false)}>
            Cancel
          </Button>
        </div>
      </Show>

      <div class="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          data-testid="change-set-apply"
          disabled={busy() || chosen().length === 0}
          onClick={() => void applyChosen()}
        >
          {`Apply ${plural(chosen().length, 'change', 'changes')}`}
        </Button>
        <Button size="sm" variant="secondary" data-testid="change-set-apply-all" disabled={busy() || nothingWaits()} onClick={() => void applyAll()}>
          Apply all
        </Button>
        <Show when={dirty().length > 0}>
          <Button size="sm" variant="ghost" data-testid="change-set-save" disabled={busy()} onClick={() => void saveOnly()}>
            Save edits
          </Button>
        </Show>
        <Button size="sm" variant="ghost" data-testid="change-set-reject" disabled={busy() || nothingWaits() || rejecting()} onClick={() => setRejecting(true)}>
          Reject…
        </Button>
        <Show when={nothingWaits()}>
          <span class="text-xs" style={secondary}>
            Nothing here waits for you.
          </span>
        </Show>
      </div>
    </section>
  );
};

export default ChangeSetPanel;
