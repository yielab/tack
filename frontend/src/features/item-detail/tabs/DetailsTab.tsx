import { type Component, createResource, createSignal, createEffect, For, Show, type JSX } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import RichTextEditor from '../../../shared/ui/RichTextEditor';
import { api, ApiError } from '../../../shared/api';
import type {
  AcceptanceCriterion,
  BriefConstraint,
  BriefRisk,
  ItemBrief,
} from '../../../shared/api/briefs';
import { Button, Field, FieldShell, Select } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import type { Item } from '../../../shared/types';

export interface DetailsTabProps {
  item: Item;
  /** Persist a description change (debounced by the caller is optional). */
  onDescriptionChange: (html: string) => void;
}

type CriterionKind = AcceptanceCriterion['kind'];
type ConstraintKind = BriefConstraint['kind'];

const CRITERION_KINDS: { value: CriterionKind; label: string }[] = [
  { value: 'command', label: 'Command' },
  { value: 'test', label: 'Test' },
  { value: 'metric', label: 'Metric' },
  { value: 'file', label: 'File exists' },
  { value: 'absent', label: 'File is absent' },
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


const COUNT_FORMAT = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Item description, edited with the existing rich-text editor, plus the
 * item's manual GitHub issue link (link, unlink; shows the current link). */
const DetailsTab: Component<DetailsTabProps> = (props) => {
  const [link, { refetch: refetchLink }] = createResource(
    () => props.item.id,
    (id) => api.items.getGithubLink(id),
  );
  const [draft, setDraft] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const linkIssue = async () => {
    // One field, split on the last `#`: "owner/repo#42".
    const raw = draft().trim();
    const hashIndex = raw.lastIndexOf('#');
    const repo = hashIndex > 0 ? raw.slice(0, hashIndex) : '';
    const issueNumber = hashIndex > 0 ? Number(raw.slice(hashIndex + 1)) : NaN;
    if (!repo || !Number.isInteger(issueNumber) || issueNumber < 1) {
      toast.error('Enter the link as owner/repo#issue-number');
      return;
    }
    setBusy(true);
    try {
      await api.items.setGithubLink(props.item.id, repo, issueNumber);
      setDraft('');
      await refetchLink();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to link the GitHub issue');
    } finally {
      setBusy(false);
    }
  };

  const [brief] = createResource(
    () => props.item.id,
    async (id): Promise<ItemBrief | null> => {
      try {
        return await api.briefs.get(id);
      } catch (err) {
        if (err instanceof ApiError && err.status === 404) return null;
        throw err;
      }
    },
  );

  // The checklist (manual criteria) and the agent's criteria are edited apart
  // and saved together: the brief is one document on the server.
  const [checks, setChecks] = createStore<CriterionRow[]>([]);
  const [criteria, setCriteria] = createStore<CriterionRow[]>([]);
  const [constraints, setConstraints] = createStore<ConstraintRow[]>([]);
  const [done, setDone] = createSignal<string | null>(null);
  const [risk, setRisk] = createSignal<BriefRisk | ''>('');
  const [errors, setErrors] = createSignal<{ acceptance?: string; constraints?: string; other?: string }>({});
  const [saving, setSaving] = createSignal(false);
  let nextId = 1;

  const load = (b: ItemBrief) => {
    const rows = b.acceptance.map(toRow);
    setChecks(rows.filter((r) => r.kind === 'manual'));
    setCriteria(rows.filter((r) => r.kind !== 'manual'));
    setConstraints(b.constraints.map(constraintToRow));
    setDone(b.definition_of_done ?? null);
    setRisk(b.risk ?? '');
  };

  createEffect(() => {
    const b = brief();
    if (b?.acceptance) load(b);
  });

  const freshId = () => {
    let id: string;
    do id = `m${nextId++}`;
    while (checks.some((c) => c.id === id) || criteria.some((c) => c.id === id));
    return id;
  };

  const patchCriterion = (i: number, patch: Partial<CriterionRow>) =>
    setCriteria(produce((rows) => Object.assign(rows[i], patch)));

  const move = (i: number, by: number) =>
    setChecks(
      produce((rows) => {
        const j = i + by;
        if (j < 0 || j >= rows.length) return;
        [rows[i], rows[j]] = [rows[j], rows[i]];
      }),
    );

  const save = async () => {
    setSaving(true);
    setErrors({});
    try {
      const manual = checks
        .filter((c) => c.text.trim() !== '')
        .map((c) => fromRow({ ...c, title: c.text.slice(0, 60) }));
      const saved = await api.briefs.put(props.item.id, {
        acceptance: [...manual, ...criteria.map(fromRow)],
        constraints: constraints.map(constraintFromRow),
        definition_of_done: done(),
        risk: risk() || null,
      });
      load(saved);
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) setErrors(sortErrors(err.message));
      else toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  const agentSummary = () => {
    const parts: string[] = [];
    if (criteria.length) parts.push(COUNT_FORMAT(criteria.length, 'check', 'checks'));
    if (constraints.length) parts.push(COUNT_FORMAT(constraints.length, 'limit', 'limits'));
    if (risk()) parts.push('risk noted');
    return parts.join(' · ');
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
        return null;
    }
  };

  const unlinkIssue = async () => {
    setBusy(true);
    try {
      await api.items.removeGithubLink(props.item.id);
      await refetchLink();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to unlink the GitHub issue');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="space-y-4">
      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
        Description
      </h3>
      <RichTextEditor
        value={props.item.description ?? ''}
        onChange={props.onDescriptionChange}
        placeholder="Add details or notes…"
      />
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
        <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
          Acceptance criteria
        </h3>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          One line per thing a person checks before calling this done.
        </p>
        <Show when={errors().acceptance}>
          <p role="alert" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {errors().acceptance}
          </p>
        </Show>
        <For each={checks}>
          {(c, i) => (
            <div class="flex items-center gap-2" data-testid="check">
              <input
                aria-label="Acceptance criterion"
                value={c.text}
                onInput={(e) => setChecks(i(), 'text', e.currentTarget.value)}
                onFocusOut={() => void save()}
                class={controlClass}
                style={controlStyle}
              />
              <Button variant="ghost" size="sm" type="button" aria-label="Move up" onClick={() => { move(i(), -1); void save(); }}>
                Up
              </Button>
              <Button variant="ghost" size="sm" type="button" aria-label="Move down" onClick={() => { move(i(), 1); void save(); }}>
                Down
              </Button>
              <Button
                variant="ghost"
                size="sm"
                type="button"
                aria-label="Remove check"
                onClick={() => {
                  setChecks((rows) => rows.filter((_, j) => j !== i()));
                  void save();
                }}
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
          onClick={() => setChecks((rows) => [...rows, blankCriterion('manual', freshId())])}
        >
          Add check
        </Button>
      </section>

      <details class="rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
        <summary class="cursor-pointer text-lg" style={{ color: 'var(--color-text-primary)' }}>
          For the agent
          <Show when={agentSummary()}>
            <span class="ml-2 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              {agentSummary()}
            </span>
          </Show>
        </summary>
        <div class="mt-3 space-y-4">
          <div class="space-y-3">
            <h4 class="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
              Checks a machine runs
            </h4>
            <For each={criteria}>
              {(c, i) => (
                <div
                  class="space-y-2 rounded-[20px] border p-3"
                  style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-app)' }}
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
          </div>

          <div class="space-y-3">
            <h4 class="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
              Constraints
            </h4>
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
          </div>

          <FieldShell label="Risk">
            <Select aria-label="Risk" value={risk()} onChange={(e) => setRisk(e.currentTarget.value as BriefRisk | '')}>
              <option value="">Not set</option>
              <option value="low">Low</option>
              <option value="medium">Medium</option>
              <option value="high">High</option>
            </Select>
          </FieldShell>

          <Show when={errors().other}>
            <p role="alert" class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
              {errors().other}
            </p>
          </Show>
          <Button type="button" loading={saving()} onClick={() => void save()}>
            Save for the agent
          </Button>
        </div>
      </details>

      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
        Link GitHub issue
      </h3>
      <Show
        when={link()}
        fallback={
          <div class="flex items-end gap-2">
            <Field
              class="flex-1"
              label="Issue"
              placeholder="owner/repo#42"
              value={draft()}
              disabled={busy()}
              onInput={(e) => setDraft(e.currentTarget.value)}
            />
            <Button size="sm" onClick={() => void linkIssue()} loading={busy()}>
              Link
            </Button>
          </div>
        }
      >
        <div class="flex items-center justify-between gap-2 rounded-[20px] py-2 pl-4 pr-2" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
          <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)', 'font-family': 'var(--font-mono)' }}>
            {link()!.repo}#{link()!.issue_number}
          </span>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void unlinkIssue()}
            loading={busy()}
          >
            Unlink
          </Button>
        </div>
      </Show>
      </section>
    </div>
  );
};

export default DetailsTab;
