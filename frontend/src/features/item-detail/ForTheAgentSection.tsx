import { type Component, createResource, createSignal, For, Show, type JSX } from 'solid-js';
import { api } from '../../shared/api';
import type { AcceptanceCriterion, BriefConstraint, BriefRisk } from '../../shared/api/briefs';
import { Button, FieldShell, Fold, HelpHint, HelpNote, SectionHeading, Select } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import {
  CONSTRAINT_KINDS,
  CONSTRAINT_LABEL,
  CRITERION_KINDS,
  blankCriterion,
  controlClass,
  controlStyle,
  fromRow,
  type ConstraintKind,
  type CriterionKind,
} from './briefRows';
import { FolderMark } from './FolderMark';
import { IconRobot } from './sectionIcons';
import type { BriefEditor } from './useBrief';
import type { FolderMarks } from './useFolderMarks';

const RISK_LABEL: Record<BriefRisk, string> = { low: 'Low', medium: 'Medium', high: 'High' };

const COUNT = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** What a check does, in a few words, for the read-only table. */
function checkDetail(c: AcceptanceCriterion): string {
  switch (c.kind) {
    case 'command':
      return `${c.run} · exit ${c.expect_exit}`;
    case 'test':
      return c.name;
    case 'metric':
      return `${c.name} ${{ lte: 'at most', gte: 'at least', eq: 'exactly' }[c.op]} ${c.threshold}${c.unit ? ` ${c.unit}` : ''}`;
    case 'file':
      return `${c.path} exists`;
    case 'absent':
      return `${c.path} is absent`;
    case 'manual':
      return c.text;
  }
}

/** One limit as a short phrase. */
function constraintLine(c: BriefConstraint): string {
  switch (c.kind) {
    case 'forbidden_path':
      return `Keeps out of ${c.glob}`;
    case 'allowed_dependency':
      return `May use ${c.name}`;
    case 'max_changed_files':
      return `Changes at most ${COUNT(c.n, 'file', 'files')}`;
    case 'note':
      return c.text;
  }
}

const Subheading: Component<{ children: JSX.Element }> = (props) => (
  <h3 class="text-lead font-semibold" style={{ color: 'var(--color-text-primary)' }}>
    {props.children}
  </h3>
);

/** The panel's content: what is written (checks, limits, risk, the run's token limit) and, on Edit, the form that writes it. */
const AgentBody: Component<{ itemId: string; brief: BriefEditor; editing: boolean }> = (props) => {
  const b = () => props.brief;
  const [context] = createResource(
    () => props.itemId,
    (id) => api.briefs.agentContext(id).catch(() => undefined),
  );
  const [previewing, setPreviewing] = createSignal(false);

  const text = (
    label: string,
    value: string,
    onInput: (v: string) => void,
    extra: JSX.InputHTMLAttributes<HTMLInputElement> = {},
  ) => (
    <FieldShell label={label}>
      <input {...extra} aria-label={label} value={value} onInput={(e) => onInput(e.currentTarget.value)} class={controlClass} style={controlStyle} />
    </FieldShell>
  );

  const kindFields = (i: number) => {
    const c = b().criteria[i];
    const set = (k: string) => (v: string) => b().patchCriterion(i, { [k]: v });
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
                onChange={(e) => b().patchCriterion(i, { op: e.currentTarget.value as 'lte' | 'gte' | 'eq' })}
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

  const preview = (
    <div class="space-y-2">
      <button
        type="button"
        aria-expanded={previewing()}
        onClick={() => setPreviewing(!previewing())}
        class="text-small font-semibold hover:underline focus:outline-none focus-visible:ring-2"
        style={{ color: 'var(--color-accent-ink)', '--tw-ring-color': 'var(--color-focus-ring)' }}
      >
        Preview everything the agent will read →
      </button>
      <Show when={previewing()}>
        <pre
          data-testid="agent-preview"
          class="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border px-4 py-3 text-small"
          style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)', color: 'var(--color-text-primary)' }}
        >
          {context()?.text ?? 'Not available.'}
        </pre>
      </Show>
    </div>
  );

  return (
    <Show
      when={props.editing}
      fallback={
        <div class="space-y-5">
          <div class="space-y-2.5">
            <Subheading>
              Checks a machine runs <span class="font-normal" style={{ color: 'var(--color-text-secondary)' }}>· {b().criteria.length}</span>
            </Subheading>
            <Show
              when={b().criteria.length > 0}
              fallback={<p class="text-body" style={{ color: 'var(--color-text-secondary)' }}>None yet. Edit to add one.</p>}
            >
              <ul class="rounded-md border" style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}>
                <For each={b().criteria.map(fromRow)}>
                  {(c) => (
                    <li
                      class="flex flex-wrap items-baseline gap-x-4 gap-y-1 border-b px-4 py-3 last:border-b-0"
                      style={{ 'border-color': 'var(--color-border-subtle)' }}
                      data-testid="criterion-row"
                    >
                      <span class="min-w-0 flex-1 text-lead">{c.title || checkDetail(c)}</span>
                      <code class="font-mono text-small" style={{ color: 'var(--color-text-secondary)' }}>{checkDetail(c)}</code>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
          <div class="space-y-2">
            <Subheading>Constraints</Subheading>
            <Show
              when={b().constraints.length > 0}
              fallback={<p class="text-body" style={{ color: 'var(--color-text-secondary)' }}>None.</p>}
            >
              <ul class="space-y-1 text-lead">
                <For each={b().constraints.map(constraintFromRowLine)}>{(line) => <li>{line}</li>}</For>
              </ul>
            </Show>
          </div>
          <dl class="flex flex-wrap gap-x-10 gap-y-3">
            <div>
              <dt class="text-small" style={{ color: 'var(--color-text-secondary)' }}>Risk</dt>
              <dd class="text-lead font-medium" data-testid="agent-risk">{b().risk() ? RISK_LABEL[b().risk() as BriefRisk] : 'Not set'}</dd>
            </div>
            <Show when={context()?.limit}>
              {(limit) => (
                <div class="min-w-0">
                  <dt class="text-small" style={{ color: 'var(--color-text-secondary)' }}>Token limit</dt>
                  <dd class="text-lead font-medium" data-testid="agent-token-limit">{limit().sentence}</dd>
                </div>
              )}
            </Show>
          </dl>
          {preview}
        </div>
      }
    >
      <div class="space-y-5">
        <HelpNote help={HELP.forTheAgent} />
        <div class="space-y-3">
          <Subheading>Checks a machine runs</Subheading>
          <For each={b().criteria}>
            {(c, i) => (
              <div
                class="space-y-2 rounded-md border p-3"
                style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-base)' }}
                data-testid="criterion"
              >
                <div class="flex items-center gap-2">
                  <Select
                    class="min-w-60"
                    aria-label="Kind"
                    value={c.kind}
                    onChange={(e) => {
                      const next = blankCriterion(e.currentTarget.value as CriterionKind, c.id);
                      next.title = c.title;
                      b().setCriteria(i(), next);
                    }}
                  >
                    <For each={CRITERION_KINDS}>{(k) => <option value={k.value}>{k.label}</option>}</For>
                  </Select>
                  <Button
                    class="ml-auto"
                    variant="destructive"
                    size="sm"
                    type="button"
                    aria-label="Remove criterion"
                    onClick={() => b().setCriteria((rows) => rows.filter((_, j) => j !== i()))}
                  >
                    Remove
                  </Button>
                </div>
                {text('Title', c.title, (v) => b().patchCriterion(i(), { title: v }))}
                {kindFields(i())}
              </div>
            )}
          </For>
          <Button
            variant="secondary"
            size="sm"
            type="button"
            onClick={() => b().setCriteria((rows) => [...rows, blankCriterion('command', b().freshId())])}
          >
            Add criterion
          </Button>
        </div>

        <div class="space-y-3">
          <Subheading>Constraints</Subheading>
          <Show when={b().errors().constraints}>
            <p role="alert" class="text-small" style={{ color: 'var(--color-danger-700)' }}>
              {b().errors().constraints}
            </p>
          </Show>
          <For each={b().constraints}>
            {(c, i) => (
              <div class="flex items-end gap-2" data-testid="constraint">
                <Select
                  aria-label="Constraint kind"
                  value={c.kind}
                  onChange={(e) => b().setConstraints(i(), { kind: e.currentTarget.value as ConstraintKind, value: '' })}
                >
                  <For each={CONSTRAINT_KINDS}>{(k) => <option value={k.value}>{k.label}</option>}</For>
                </Select>
                <input
                  aria-label={CONSTRAINT_LABEL[c.kind]}
                  placeholder={CONSTRAINT_LABEL[c.kind]}
                  type={c.kind === 'max_changed_files' ? 'number' : 'text'}
                  value={c.value}
                  onInput={(e) => b().setConstraints(i(), 'value', e.currentTarget.value)}
                  class={controlClass}
                  style={controlStyle}
                />
                <Button
                  variant="destructive"
                  size="sm"
                  type="button"
                  aria-label="Remove constraint"
                  onClick={() => b().setConstraints((rows) => rows.filter((_, j) => j !== i()))}
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
            onClick={() => b().setConstraints((rows) => [...rows, { kind: 'forbidden_path', value: '' }])}
          >
            Add constraint
          </Button>
        </div>

        <FieldShell label="Risk">
          <Select aria-label="Risk" value={b().risk()} onChange={(e) => b().setRisk(e.currentTarget.value as BriefRisk | '')}>
            <option value="">Not set</option>
            <option value="low">Low</option>
            <option value="medium">Medium</option>
            <option value="high">High</option>
          </Select>
        </FieldShell>

        <Show when={b().errors().other}>
          <p role="alert" class="text-small" style={{ color: 'var(--color-danger-700)' }}>
            {b().errors().other}
          </p>
        </Show>
        <Button type="button" loading={b().saving()} onClick={() => void b().save()}>
          Save for the agent
        </Button>
        {preview}
      </div>
    </Show>
  );
};

/** A constraint row as the short phrase the table shows. */
const constraintFromRowLine = (r: { kind: ConstraintKind; value: string }): string => {
  switch (r.kind) {
    case 'forbidden_path':
      return constraintLine({ kind: r.kind, glob: r.value });
    case 'allowed_dependency':
      return constraintLine({ kind: r.kind, name: r.value });
    case 'max_changed_files':
      return constraintLine({ kind: r.kind, n: Number(r.value) });
    case 'note':
      return constraintLine({ kind: r.kind, text: r.value });
  }
};

export interface ForTheAgentSectionProps {
  itemId: string;
  brief: BriefEditor;
  marks: FolderMarks;
  /** `page`: its own soft-teal panel, open. `drawer`: one summary line that opens it. */
  layout: 'page' | 'drawer';
}

/** What the agent reads apart from the description: the checks a machine runs, the limits, the risk and the run's token limit. The person can skip it. */
const ForTheAgentSection: Component<ForTheAgentSectionProps> = (props) => {
  const b = () => props.brief;
  const [editing, setEditing] = createSignal(false);
  const summary = () => {
    const parts: string[] = [];
    if (b().criteria.length) parts.push(COUNT(b().criteria.length, 'check', 'checks'));
    if (b().constraints.length) parts.push(COUNT(b().constraints.length, 'limit', 'limits'));
    if (b().risk()) parts.push(`risk ${b().risk()}`);
    return parts.join(' · ');
  };
  const panel = { 'background-color': 'var(--color-accent-soft)', 'border-color': 'color-mix(in srgb, var(--color-accent-ink) 25%, transparent)' };

  return (
    <Show
      when={props.layout === 'page'}
      fallback={
        <Fold
          kind="for-the-agent"
          title={<span class="text-section">For the agent</span>}
          summary={summary() || 'Nothing written yet'}
          lazy
          class="rounded-lg border p-4"
          style={panel}
        >
          <div class="space-y-4 pt-2">
            <div class="flex items-center gap-2">
              <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>The agent reads this with the description. People can skip it.</span>
              <Button class="ml-auto" variant="secondary" size="sm" type="button" onClick={() => setEditing(!editing())}>
                {editing() ? 'Done' : 'Edit'}
              </Button>
            </div>
            <FolderMark marks={props.marks} fields={['brief']} class="block" />
            <AgentBody itemId={props.itemId} brief={props.brief} editing={editing()} />
          </div>
        </Fold>
      }
    >
      <section aria-labelledby="item-for-the-agent" class="space-y-4 rounded-lg border p-6" style={panel}>
        <SectionHeading
          id="item-for-the-agent"
          icon={<IconRobot />}
          tone="agent"
          actions={
            <>
              <HelpHint label="For the agent" help={HELP.forTheAgent} />
              <Button variant="secondary" size="sm" type="button" onClick={() => setEditing(!editing())}>
                {editing() ? 'Done' : 'Edit'}
              </Button>
            </>
          }
        >
          For the agent
        </SectionHeading>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          The agent reads this with the description. People can skip it. <FolderMark marks={props.marks} fields={['brief']} />
        </p>
        <AgentBody itemId={props.itemId} brief={props.brief} editing={editing()} />
      </section>
    </Show>
  );
};

export default ForTheAgentSection;
