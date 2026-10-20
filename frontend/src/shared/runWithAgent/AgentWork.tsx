import { type Component, For, type JSX, Show, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Badge } from '../ui';
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
          class="whitespace-pre-wrap rounded-[20px] px-4 py-3 text-sm"
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
                  class="max-w-full break-all rounded-full px-2.5 py-0.5 text-[0.6875rem]"
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
        <p class="rounded-[20px] px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
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
