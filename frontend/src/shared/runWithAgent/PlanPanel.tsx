import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Button } from '../ui';
import { artifactsApi } from '../execution';
import { items } from '../api/items';

interface PlanSubtask {
  title: string;
  description: string;
  acceptance: string[];
  depends_on: number[];
  estimate: string;
}

interface Row extends PlanSubtask {
  selected: boolean;
}

export interface PlanPanelProps {
  requestId: string;
  attemptNumber: number;
  artifactId: string;
  itemId: string;
}

const inputStyle = {
  border: '1px solid var(--color-border-light)',
  color: 'var(--color-text-primary)',
  background: 'transparent',
} as const;

/** The Planner's proposed subtasks: tick, edit, create the chosen ones. */
const PlanPanel: Component<PlanPanelProps> = (props) => {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [created, setCreated] = createSignal<Array<{ id: string; title: string }> | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [loaded] = createResource(
    () => `${props.requestId}:${props.attemptNumber}:${props.artifactId}`,
    async () => {
      try {
        const plan = JSON.parse(
          await (await artifactsApi.download(props.requestId, props.attemptNumber, props.artifactId)).text(),
        ) as { subtasks?: PlanSubtask[] };
        setRows((plan.subtasks ?? []).map((s) => ({ ...s, selected: true })));
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Could not read the plan.');
      }
      return true;
    },
  );
  const patch = (i: number, change: Partial<Row>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...change } : r)));
  const chosen = () => rows().filter((r) => r.selected);
  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await items.createSubtasksFromPlan(props.itemId, {
        artifact_id: props.artifactId,
        subtasks: chosen().map((r) => ({
          title: r.title,
          description: r.description,
          acceptance: r.acceptance.filter((a) => a.trim() !== ''),
        })),
      });
      setCreated(res.created);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not create the subtasks.');
    } finally {
      setBusy(false);
    }
  };
  const after = (r: Row) => (r.depends_on.length ? ` · after ${r.depends_on.map((d) => d + 1).join(', ')}` : '');
  return (
    <Show when={loaded() && (rows().length > 0 || error())}>
      <section data-testid="plan-panel" class="space-y-2">
        <Show
          when={created()}
          fallback={
            <>
              <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
                The Planner proposes {rows().length} subtasks
              </h4>
              <For each={rows()}>
                {(r, i) => (
                  <div class="space-y-1 rounded-[12px] p-2" style={{ border: '1px solid var(--color-border-light)' }}>
                    <label class="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={r.selected}
                        aria-label={`Create ${r.title}`}
                        onChange={(e) => patch(i(), { selected: e.currentTarget.checked })}
                      />
                      <input
                        class="flex-1 rounded px-2 py-1"
                        style={inputStyle}
                        aria-label="Title"
                        value={r.title}
                        onInput={(e) => patch(i(), { title: e.currentTarget.value })}
                      />
                    </label>
                    <input
                      class="w-full rounded px-2 py-1 text-sm"
                      style={inputStyle}
                      aria-label="Description"
                      value={r.description}
                      onInput={(e) => patch(i(), { description: e.currentTarget.value })}
                    />
                    <textarea
                      class="w-full rounded px-2 py-1 text-sm"
                      style={inputStyle}
                      aria-label="Acceptance criteria, one per line"
                      rows={Math.max(2, r.acceptance.length)}
                      value={r.acceptance.join('\n')}
                      onInput={(e) => patch(i(), { acceptance: e.currentTarget.value.split('\n') })}
                    />
                    <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      {r.estimate}
                      {after(r)}
                    </p>
                  </div>
                )}
              </For>
              <Button size="sm" disabled={busy() || chosen().length === 0} onClick={() => void create()}>
                Create selected
              </Button>
            </>
          }
        >
          {(list) => (
            <>
              <h4 class="text-base" style={{ color: 'var(--color-text-primary)' }}>
                Created {list().length} subtasks
              </h4>
              <ul class="text-sm">
                <For each={list()}>{(c) => <li><a href={`?item=${c.id}`}>{c.title}</a></li>}</For>
              </ul>
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
