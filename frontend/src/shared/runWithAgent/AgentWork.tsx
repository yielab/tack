import { type Component, For, type JSX, Show, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Badge } from '../ui';
import { toast } from '../ui/toast';
import type { RunRecordSet, RunStep, RunStepPrompt } from '../execution';
import { inProject } from './attemptFormat';
import EventTimeline from './EventTimeline';
import { groupByTool, type RunLog, type ToolGroup } from './runLog';

/** The report is clipped to this many lines until "Read full report". */
const CLAMP_LINES = 5;
/** A report shorter than this, in few lines, is shown whole, with nothing to expand. */
const CLAMP_CHARS = 360;
/** A tool's first targets show as chips; the rest wait behind "+N more". */
const TARGETS_SHOWN = 10;
const CHIP_CHARS = 64;

/** Where the page's side column takes what belongs beside the run. With a `target` the
 *  children are placed there; without one they stay where they are written (the drawer's one
 *  column); on the page, before the column exists, they wait. */
export const SideSlot: Component<{ page: boolean; target?: HTMLElement; children: JSX.Element }> = (props) => (
  <Show when={props.page} fallback={props.children}>
    <Show when={props.target}>{(mount) => <Portal mount={mount()}>{props.children}</Portal>}</Show>
  </Show>
);

/** What the agent said at the end, as written, clipped to a few lines. */
export const AgentConcluded: Component<{ result: string | null; workspace: string | null }> = (props) => {
  const [full, setFull] = createSignal(false);
  const text = () => inProject(props.result ?? '', props.workspace);
  const long = () => text().length > CLAMP_CHARS || text().split('\n').length > CLAMP_LINES;
  const clamped = () => long() && !full();
  return (
    <Show when={props.result}>
      <section data-testid="agent-concluded">
        <h4 class="mb-1.5 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent concluded</h4>
        <p
          data-testid="agent-result"
          class="whitespace-pre-wrap rounded-item px-4 py-3 text-sm"
          classList={{ 'max-h-96 overflow-auto': !clamped() }}
          style={{
            'background-color': 'var(--color-bg-panel)',
            color: 'var(--color-text-primary)',
            ...(clamped() ? { display: '-webkit-box', '-webkit-line-clamp': String(CLAMP_LINES), '-webkit-box-orient': 'vertical', overflow: 'hidden' } : {}),
          }}
        >
          {text()}
        </p>
        <Show when={long()}>
          <button type="button" aria-expanded={full()} class="mt-1 text-xs font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }} onClick={() => setFull((v) => !v)}>
            {full() ? 'Show less' : 'Read full report'}
          </button>
        </Show>
      </section>
    </Show>
  );
};

const short = (target: string) => (target.length > CHIP_CHARS ? `${target.slice(0, CHIP_CHARS - 1)}…` : target);

/** One tool: its name, how many times it was called, and what it was called on as chips. */
const ToolGroupRow: Component<{ group: ToolGroup; workspace: string | null }> = (props) => {
  const [all, setAll] = createSignal(false);
  const targets = () => (all() ? props.group.targets : props.group.targets.slice(0, TARGETS_SHOWN));
  const hidden = () => props.group.targets.length - TARGETS_SHOWN;
  return (
    <li data-testid="tool-group" class="space-y-1.5">
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>{props.group.tool}</span>
        <span data-testid="tool-count" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          {props.group.count} {props.group.count === 1 ? 'call' : 'calls'}
        </span>
        <Show when={props.group.failed > 0}>
          <Badge tone="danger">{props.group.failed} failed</Badge>
        </Show>
      </div>
      <Show when={props.group.targets.length > 0}>
        <ul class="flex flex-wrap gap-1.5">
          <For each={targets()}>
            {(t) => {
              const shown = () => inProject(t.target, props.workspace);
              return (
                <li
                  data-testid="tool-target"
                  title={`${shown()}${t.failed ? ' (failed)' : ''}`}
                  class="max-w-full break-all rounded-full px-2.5 py-0.5 text-micro"
                  style={{
                    'font-family': 'var(--font-mono)',
                    'background-color': t.failed ? 'var(--color-danger-100)' : 'var(--color-bg-subtle)',
                    color: t.failed ? 'var(--color-danger-700)' : 'var(--color-text-secondary)',
                  }}
                >
                  {short(shown())}
                  <Show when={t.failed}><span class="sr-only"> (failed)</span></Show>
                </li>
              );
            }}
          </For>
        </ul>
        <Show when={hidden() > 0}>
          <button type="button" class="text-xs font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }} aria-expanded={all()} onClick={() => setAll((v) => !v)}>
            {all() ? 'Show fewer' : `+${hidden()} more`}
          </button>
        </Show>
      </Show>
    </li>
  );
};

/** What the agent did: its steps grouped by tool with the targets as chips, or the run's timeline. */
export const AgentDid: Component<{
  requestId: string;
  attemptNumber: number;
  /** The steps read from the run log; `undefined` when the log has none or is not read yet. */
  log: RunLog | undefined;
  /** The log is being read. */
  reading: boolean;
  terminal: boolean;
  workspace: string | null;
}> = (props) => {
  const [view, setView] = createSignal<'tool' | 'timeline'>('tool');
  const groups = () => groupByTool(props.log?.steps ?? []);
  const hasSteps = () => groups().length > 0;
  // Without steps the timeline is all there is to show.
  const shown = () => (hasSteps() ? view() : 'timeline');
  const noTools = () => props.log?.toolsOffered?.length === 0;
  const whyNoSteps = () =>
    !props.terminal
      ? 'The steps are listed by tool once the run ends'
      : props.reading
        ? 'Reading the run log…'
        : "This agent's log does not name its steps";
  const segment = (id: 'tool' | 'timeline', label: string, disabled = false, reason?: string) => (
    <button
      type="button"
      data-testid={`did-${id}`}
      aria-pressed={shown() === id}
      disabled={disabled}
      title={disabled ? reason : undefined}
      onClick={() => setView(id)}
      class="rounded-full px-3 py-1 text-xs font-semibold focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-60"
      style={{
        'background-color': shown() === id ? 'var(--color-bg-app)' : 'transparent',
        color: shown() === id ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
        'box-shadow': shown() === id ? 'var(--shadow-sm)' : 'none',
      }}
    >
      {label}
    </button>
  );
  return (
    <section data-testid="agent-did" class="space-y-2">
      <Show when={noTools()}>
        <p class="rounded-item px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
          The agent had no tools in this run, so it could not read or change files.
        </p>
      </Show>
      <div class="flex flex-wrap items-center gap-3">
        <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the agent did</h4>
        <div role="group" aria-label="How to show what the agent did" class="inline-flex gap-0.5 rounded-full p-0.5" style={{ 'background-color': 'var(--color-bg-subtle)' }}>
          {segment('tool', 'By tool', !hasSteps(), whyNoSteps())}
          {segment('timeline', 'Timeline')}
        </div>
      </div>
      <Show when={!hasSteps() && !props.reading && props.terminal}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>{whyNoSteps()}.</p>
      </Show>
      <Show when={!(props.terminal && props.reading)} fallback={<p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>Reading the run log…</p>}>
      <Show
        when={shown() === 'tool'}
        fallback={
          <EventTimeline requestId={props.requestId} attemptNumber={props.attemptNumber} live={!props.terminal} workspace={props.workspace} />
        }
      >
        <ul data-testid="agent-steps" class="space-y-3">
          <For each={groups()}>{(group) => <ToolGroupRow group={group} workspace={props.workspace} />}</For>
        </ul>
      </Show>
      </Show>
    </section>
  );
};

const ROLE_WORDS: Record<string, string> = { lead: 'Lead', implementer: 'Implementer', reviewer: 'Reviewer' };
const CLASS_WORDS: Record<string, string> = { cheap: 'a small model', strong: 'a strong model' };
const SCOPE_WORDS: Record<string, string> = {
  shipped: 'shipped with docket',
  pod: "the team's",
  repository: "this repository's",
  operator: 'your settings',
  task: "this task's",
};
const LAYER_WORDS: Record<string, string> = { identity: 'identity', task: 'task', hop: 'hop' };

/** A role in words: Lead, Implementer, Reviewer, or a custom role's name capitalised. */
export function roleWords(role: string | null, n: number): string {
  if (!role) return `Step ${n}`;
  return ROLE_WORDS[role] ?? role.charAt(0).toUpperCase() + role.slice(1);
}

/** A model class in words; a class this screen does not know reads as its own name. */
export function classWords(model: string | null, modelClass: string | null): string | null {
  const phrase = modelClass ? (CLASS_WORDS[modelClass] ?? `a ${modelClass} model`) : null;
  return [model, phrase].filter((x): x is string => !!x).join(' · ') || null;
}

/** Where docket found a prompt, in plain words; a scope this screen does not know reads as "other". */
export function scopeWords(scope: string): string {
  return SCOPE_WORDS[scope] ?? 'other';
}

const copyWhole = (value: string) => {
  void navigator.clipboard.writeText(value).then(
    () => toast.success('Fingerprint copied.'),
    () => toast.error(`Couldn't copy the fingerprint: ${value}`),
  );
};

/** One prompt a step ran: `name version`, with where it came from in plain words. Its whole
 *  fingerprint is copied on click; the title shows it shortened. */
const PromptChip: Component<{ prompt: RunStepPrompt }> = (props) => {
  const label = () => (props.prompt.version ? `${props.prompt.name} ${props.prompt.version}` : props.prompt.name);
  const layer = () => LAYER_WORDS[props.prompt.layer] ?? props.prompt.layer;
  const print = () => (props.prompt.sha256 ? props.prompt.sha256.slice(0, 12) : 'none');
  return (
    <li data-testid="step-prompt" class="max-w-full">
      <button
        type="button"
        title={`${layer()} prompt, ${scopeWords(props.prompt.scope)}, fingerprint ${print()}`}
        disabled={!props.prompt.sha256}
        onClick={() => copyWhole(props.prompt.sha256)}
        class="max-w-full break-all rounded-full px-2.5 py-0.5 text-left text-micro disabled:cursor-default"
        style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
      >
        <span style={{ 'font-family': 'var(--font-mono)' }}>{label()}</span>{' '}
        <span style={{ color: 'var(--color-text-tertiary)' }}>{scopeWords(props.prompt.scope)}</span>
      </button>
    </li>
  );
};

/** One hop: who it was, the model it ran on, the verdict, its prompts, and what it assumed and asked. */
const StepRow: Component<{ step: RunStep; n: number }> = (props) => {
  const model = () => classWords(props.step.model, props.step.class);
  const list = (title: string, items: string[]) => (
    <Show when={items.length > 0}>
      <div class="space-y-1">
        <h5 class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>{title}</h5>
        <ul class="list-disc space-y-0.5 pl-5 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          <For each={items}>{(item) => <li class="break-words">{item}</li>}</For>
        </ul>
      </div>
    </Show>
  );
  return (
    <li data-testid="run-step" class="space-y-2 rounded-item px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>{roleWords(props.step.role, props.n)}</span>
        <Show when={model()}>
          {(m) => <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>{m()}</span>}
        </Show>
        <Show when={props.step.verdict}>
          {(v) => <Badge tone="neutral">Verdict: {v()}</Badge>}
        </Show>
      </div>
      <Show
        when={props.step.prompts.length > 0}
        fallback={<p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>This step recorded no prompt.</p>}
      >
        <ul data-testid="step-prompts" class="flex flex-wrap gap-1.5">
          <For each={props.step.prompts}>{(p) => <PromptChip prompt={p} />}</For>
        </ul>
      </Show>
      {list('The agent assumed', props.step.assumptions)}
      {list('The agent asked', props.step.questions)}
    </li>
  );
};

/** What each step ran: one row per hop of a docket task, in order. */
export const StepsRan: Component<{ steps: RunStep[] }> = (props) => (
  <section data-testid="run-steps" class="space-y-2">
    <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What each step ran</h4>
    <ol class="space-y-2">
      <For each={props.steps}>{(step, i) => <StepRow step={step} n={i() + 1} />}</For>
    </ol>
  </section>
);

/** A cell as a person reads it: nothing for a null, the text as it is, a structure as JSON. */
const cellText = (value: unknown): string => {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
};

/** What the team recorded: one small table per record set that holds rows. Columns whose names
 *  start with an underscore are the team's bookkeeping and are not shown. */
export const TeamRecords: Component<{ sets: RunRecordSet[] }> = (props) => (
  <section data-testid="team-records" class="space-y-3">
    <h4 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>What the team recorded</h4>
    <For each={props.sets.filter((s) => s.rows.length > 0)}>
      {(set) => {
        const columns = () => set.columns.filter((c) => !c.startsWith('_'));
        return (
          <div data-testid="record-set" class="space-y-1.5">
            <h5 class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>{set.set}</h5>
            <div class="max-w-full overflow-x-auto rounded-item" style={{ 'background-color': 'var(--color-bg-panel)' }}>
              <table class="w-full text-left text-xs">
                <thead>
                  <tr>
                    <For each={columns()}>
                      {(c) => <th class="px-3 py-1.5 font-semibold" style={{ color: 'var(--color-text-secondary)' }}>{c}</th>}
                    </For>
                  </tr>
                </thead>
                <tbody>
                  <For each={set.rows}>
                    {(row) => (
                      <tr>
                        <For each={columns()}>
                          {(c) => <td class="break-words px-3 py-1.5" style={{ color: 'var(--color-text-primary)' }}>{cellText(row[c])}</td>}
                        </For>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
            <Show when={set.cut}>
              <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>More rows are kept by the team in docket.</p>
            </Show>
          </div>
        );
      }}
    </For>
  </section>
);
