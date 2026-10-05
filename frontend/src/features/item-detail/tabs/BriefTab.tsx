import { type Component, createResource, createSignal, createEffect, For, Show, type JSX } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { api, ApiError } from '../../../shared/api';
import type {
  AcceptanceCriterion,
  BriefConstraint,
  BriefRisk,
  ItemBrief,
} from '../../../shared/api/briefs';
import { toast } from '../../../shared/ui/toast';
import { Button, FieldShell, Select } from '../../../shared/ui';

export interface BriefTabProps {
  itemId: string;
}

type CriterionKind = AcceptanceCriterion['kind'];
type ConstraintKind = BriefConstraint['kind'];

const CRITERION_KINDS: { value: CriterionKind; label: string }[] = [
  { value: 'command', label: 'Command' },
  { value: 'test', label: 'Test' },
  { value: 'metric', label: 'Metric' },
  { value: 'file', label: 'File exists' },
  { value: 'absent', label: 'File is absent' },
  { value: 'manual', label: 'Manual (a person checks)' },
];

const CONSTRAINT_KINDS: { value: ConstraintKind; label: string }[] = [
  { value: 'forbidden_path', label: 'Forbidden path' },
  { value: 'allowed_dependency', label: 'Allowed dependency' },
  { value: 'max_changed_files', label: 'Max changed files' },
  { value: 'note', label: 'Note' },
];

/** A criterion as edited: every kind's fields as text, converted on save. */
interface CriterionRow {
  kind: CriterionKind;
  id: string;
  title: string;
  run: string;
  expect_exit: string;
  cwd: string;
  name: string;
  runner: string;
  op: 'lte' | 'gte' | 'eq';
  threshold: string;
  unit: string;
  path: string;
  text: string;
}

/** A constraint as edited: one text value whose meaning follows its kind. */
interface ConstraintRow {
  kind: ConstraintKind;
  value: string;
}

const blankCriterion = (kind: CriterionKind, id: string): CriterionRow => ({
  kind,
  id,
  title: '',
  run: '',
  expect_exit: '0',
  cwd: '',
  name: '',
  runner: '',
  op: 'lte',
  threshold: '',
  unit: '',
  path: '',
  text: '',
});

const orNull = (s: string) => (s.trim() === '' ? null : s);

function toRow(c: AcceptanceCriterion): CriterionRow {
  const row = blankCriterion(c.kind, c.id);
  row.title = c.title;
  switch (c.kind) {
    case 'command':
      Object.assign(row, { run: c.run, expect_exit: String(c.expect_exit), cwd: c.cwd ?? '' });
      break;
    case 'test':
      Object.assign(row, { name: c.name, runner: c.runner ?? '' });
      break;
    case 'metric':
      Object.assign(row, { name: c.name, op: c.op, threshold: String(c.threshold), unit: c.unit ?? '' });
      break;
    case 'file':
    case 'absent':
      row.path = c.path;
      break;
    case 'manual':
      row.text = c.text;
      break;
  }
  return row;
}

function fromRow(r: CriterionRow): AcceptanceCriterion {
  const { id, title } = r;
  switch (r.kind) {
    case 'command':
      return { kind: 'command', id, title, run: r.run, expect_exit: Number(r.expect_exit), cwd: orNull(r.cwd) };
    case 'test':
      return { kind: 'test', id, title, name: r.name, runner: orNull(r.runner) };
    case 'metric':
      return { kind: 'metric', id, title, name: r.name, op: r.op, threshold: Number(r.threshold), unit: orNull(r.unit) };
    case 'file':
      return { kind: 'file', id, title, path: r.path };
    case 'absent':
      return { kind: 'absent', id, title, path: r.path };
    case 'manual':
      return { kind: 'manual', id, title, text: r.text };
  }
}

function constraintToRow(c: BriefConstraint): ConstraintRow {
  switch (c.kind) {
    case 'forbidden_path':
      return { kind: c.kind, value: c.glob };
    case 'allowed_dependency':
      return { kind: c.kind, value: c.name };
    case 'max_changed_files':
      return { kind: c.kind, value: String(c.n) };
    case 'note':
      return { kind: c.kind, value: c.text };
  }
}

function constraintFromRow(r: ConstraintRow): BriefConstraint {
  switch (r.kind) {
    case 'forbidden_path':
      return { kind: r.kind, glob: r.value };
    case 'allowed_dependency':
      return { kind: r.kind, name: r.value };
    case 'max_changed_files':
      return { kind: r.kind, n: Number(r.value) };
    case 'note':
      return { kind: r.kind, text: r.value };
  }
}

const CONSTRAINT_LABEL: Record<ConstraintKind, string> = {
  forbidden_path: 'Glob, e.g. src/legacy/**',
  allowed_dependency: 'Dependency name',
  max_changed_files: 'Number of files',
  note: 'Note',
};

const controlClass =
  'w-full min-h-9 rounded-full border px-3.5 py-1.5 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1';
const controlStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
  '--tw-ring-color': 'var(--color-focus-ring)',
} as const;
const panelStyle = { 'background-color': 'var(--color-bg-panel)' } as const;

/** Split a server message into per-section errors (`acceptance: ...`) and the rest. */
function sortErrors(message: string): { acceptance?: string; constraints?: string; other?: string } {
  const out: { acceptance?: string; constraints?: string; other?: string } = {};
  for (const line of message.split('\n').map((l) => l.trim()).filter(Boolean)) {
    const m = /^(acceptance|constraints)\b[^:]*:\s*(.*)$/s.exec(line);
    if (m) out[m[1] as 'acceptance' | 'constraints'] = m[2];
    else out.other = out.other ? `${out.other} ${line}` : line;
  }
  return out;
}

/** What "done" means for an item: criteria, constraints, definition of done, risk. */
const BriefTab: Component<BriefTabProps> = (props) => {
  const [brief] = createResource(
    () => props.itemId,
    async (id): Promise<ItemBrief | null> => {
      try {
        return await api.briefs.get(id);
      } catch (err) {
        if (err instanceof ApiError && err.status === 404) return null;
        throw err;
      }
    },
  );

  const [criteria, setCriteria] = createStore<CriterionRow[]>([]);
  const [constraints, setConstraints] = createStore<ConstraintRow[]>([]);
  const [done, setDone] = createSignal('');
  const [risk, setRisk] = createSignal<BriefRisk | ''>('');
  const [errors, setErrors] = createSignal<{ acceptance?: string; constraints?: string; other?: string }>({});
  const [saving, setSaving] = createSignal(false);
  let nextId = 1;

  createEffect(() => {
    const b = brief();
    if (!b) return;
    setCriteria(b.acceptance.map(toRow));
    setConstraints(b.constraints.map(constraintToRow));
    setDone(b.definition_of_done ?? '');
    setRisk(b.risk ?? '');
  });

  const freshId = () => {
    let id: string;
    do id = `c${nextId++}`;
    while (criteria.some((c) => c.id === id));
    return id;
  };

  const patchCriterion = (i: number, patch: Partial<CriterionRow>) =>
    setCriteria(produce((rows) => Object.assign(rows[i], patch)));

  const save = async () => {
    setSaving(true);
    setErrors({});
    try {
      const saved = await api.briefs.put(props.itemId, {
        acceptance: criteria.map(fromRow),
        constraints: constraints.map(constraintFromRow),
        definition_of_done: orNull(done()),
        risk: risk() || null,
      });
      setCriteria(saved.acceptance.map(toRow));
      setConstraints(saved.constraints.map(constraintToRow));
      toast.success('Brief saved');
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) setErrors(sortErrors(err.message));
      else toast.error(err instanceof Error ? err.message : 'Failed to save brief');
    } finally {
      setSaving(false);
    }
  };

  const text = (
    label: string,
    value: string,
    onInput: (v: string) => void,
    extra: JSX.InputHTMLAttributes<HTMLInputElement> = {},
  ) => (
    <FieldShell label={label}>
      <input
        {...extra}
        aria-label={label}
        value={value}
        onInput={(e) => onInput(e.currentTarget.value)}
        class={controlClass}
        style={controlStyle}
      />
    </FieldShell>
  );

  const kindFields = (c: CriterionRow, i: number) => {
    const set = (k: keyof CriterionRow) => (v: string) => patchCriterion(i, { [k]: v });
    switch (c.kind) {
      case 'command':
        return (
          <>
            {text('Command', c.run, set('run'))}
            {text('Expected exit code', c.expect_exit, set('expect_exit'), { type: 'number', min: 0, max: 255 })}
            {text('Working directory (optional)', c.cwd, set('cwd'))}
          </>
        );
      case 'test':
        return (
          <>
            {text('Test name', c.name, set('name'))}
            {text('Runner (optional)', c.runner, set('runner'))}
          </>
        );
      case 'metric':
        return (
          <>
            {text('Metric name', c.name, set('name'))}
            <FieldShell label="Comparison">
              <Select
                aria-label="Comparison"
                value={c.op}
                onChange={(e) => patchCriterion(i, { op: e.currentTarget.value as CriterionRow['op'] })}
              >
                <option value="lte">at most</option>
                <option value="gte">at least</option>
                <option value="eq">exactly</option>
              </Select>
            </FieldShell>
            {text('Threshold', c.threshold, set('threshold'), { type: 'number' })}
            {text('Unit (optional)', c.unit, set('unit'))}
          </>
        );
      case 'file':
      case 'absent':
        return text(c.kind === 'file' ? 'Path that must exist' : 'Path that must not exist', c.path, set('path'));
      case 'manual':
        return (
          <FieldShell label="What a person must check">
            <textarea
              aria-label="What a person must check"
              rows={2}
              value={c.text}
              onInput={(e) => set('text')(e.currentTarget.value)}
              class={controlClass.replace('rounded-full', 'rounded-[20px]') + ' resize-none'}
              style={controlStyle}
            />
          </FieldShell>
        );
    }
  };

  return (
    <div class="space-y-4">
      <section class="space-y-3 rounded-[28px] p-5" style={panelStyle}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Acceptance criteria
        </h3>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Checks that must pass for this item to count as done. Prefer a kind a machine can run; use Manual only when
          nothing else fits.
        </p>
        <Show when={errors().acceptance}>
          <p role="alert" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {errors().acceptance}
          </p>
        </Show>
        <For each={criteria}>
          {(c, i) => (
            <div
              class="space-y-2 rounded-[20px] border p-3"
              style={{
                'border-color': c.kind === 'manual' ? 'var(--color-warning-500)' : 'var(--color-border-light)',
                'background-color': c.kind === 'manual' ? 'var(--color-warning-50)' : 'var(--color-bg-app)',
              }}
              data-testid="criterion"
            >
              <div class="flex items-center gap-2">
                <Select
                  class="min-w-[15rem]"
                  aria-label="Kind"
                  value={c.kind}
                  onChange={(e) => {
                    const next = blankCriterion(e.currentTarget.value as CriterionKind, c.id);
                    next.title = c.title;
                    setCriteria(i(), next);
                  }}
                >
                  <For each={CRITERION_KINDS}>{(k) => <option value={k.value}>{k.label}</option>}</For>
                </Select>
                <Button
                  class="ml-auto"
                  variant="ghost"
                  size="sm"
                  type="button"
                  aria-label="Remove criterion"
                  onClick={() => setCriteria((rows) => rows.filter((_, j) => j !== i()))}
                >
                  Remove
                </Button>
              </div>
              <Show when={c.kind === 'manual'}>
                <span
                  class="inline-block rounded-full px-2.5 py-1 text-xs font-medium"
                  style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-text-primary)' }}
                >
                  Costs a person's time
                </span>
              </Show>
              {text('Title', c.title, (v) => patchCriterion(i(), { title: v }))}
              {kindFields(c, i())}
            </div>
          )}
        </For>
        <Button
          variant="secondary"
          size="sm"
          type="button"
          onClick={() => setCriteria((rows) => [...rows, blankCriterion('command', freshId())])}
        >
          Add criterion
        </Button>
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={panelStyle}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Constraints
        </h3>
        <Show when={errors().constraints}>
          <p role="alert" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {errors().constraints}
          </p>
        </Show>
        <For each={constraints}>
          {(c, i) => (
            <div class="flex items-end gap-2" data-testid="constraint">
              <Select
                aria-label="Constraint kind"
                value={c.kind}
                onChange={(e) => setConstraints(i(), { kind: e.currentTarget.value as ConstraintKind, value: '' })}
              >
                <For each={CONSTRAINT_KINDS}>{(k) => <option value={k.value}>{k.label}</option>}</For>
              </Select>
              <input
                aria-label={CONSTRAINT_LABEL[c.kind]}
                placeholder={CONSTRAINT_LABEL[c.kind]}
                type={c.kind === 'max_changed_files' ? 'number' : 'text'}
                value={c.value}
                onInput={(e) => setConstraints(i(), 'value', e.currentTarget.value)}
                class={controlClass}
                style={controlStyle}
              />
              <Button
                variant="ghost"
                size="sm"
                type="button"
                aria-label="Remove constraint"
                onClick={() => setConstraints((rows) => rows.filter((_, j) => j !== i()))}
              >
                Remove
              </Button>
            </div>
          )}
        </For>
        <Button
          variant="secondary"
          size="sm"
          type="button"
          onClick={() => setConstraints((rows) => [...rows, { kind: 'forbidden_path', value: '' }])}
        >
          Add constraint
        </Button>
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={panelStyle}>
        <FieldShell label="Definition of done" hint="In your own words: when can someone stop and call this finished?">
          <textarea
            aria-label="Definition of done"
            rows={3}
            value={done()}
            onInput={(e) => setDone(e.currentTarget.value)}
            class={controlClass.replace('rounded-full', 'rounded-[20px]') + ' resize-none'}
            style={controlStyle}
          />
        </FieldShell>
        <FieldShell label="Risk">
          <Select aria-label="Risk" value={risk()} onChange={(e) => setRisk(e.currentTarget.value as BriefRisk | '')}>
            <option value="">Not set</option>
            <option value="low">Low</option>
            <option value="medium">Medium</option>
            <option value="high">High</option>
          </Select>
        </FieldShell>
      </section>

      <Show when={errors().other}>
        <p role="alert" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          {errors().other}
        </p>
      </Show>
      <Button type="button" loading={saving()} onClick={() => void save()}>
        Save brief
      </Button>
    </div>
  );
};

export default BriefTab;
