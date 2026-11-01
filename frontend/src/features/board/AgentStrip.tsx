import { type Component, For, Show } from 'solid-js';
import { RunState, type RunStateName } from '../../shared/ui';
import { filterOf, totalRuns, waitingCount, type RunCounts, type RunFilter } from './boardRuns';

/** The words for each state, as the board prints them next to a count. */
export const RUN_STATE_WORDS: Record<RunStateName, string> = {
  queued: 'Queued',
  running: 'Running',
  needs: 'Needs you',
  review: 'Ready for review',
  done: 'Done',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

const WAITING_NOTE = 'Runs that need you plus runs ready for review. Failed runs are counted under Failed.';

const COUNTERS: Array<{ filter: Exclude<RunFilter, 'waiting'>; state: RunStateName }> = [
  { filter: 'queued', state: 'queued' },
  { filter: 'running', state: 'running' },
  { filter: 'needs', state: 'needs' },
  { filter: 'review', state: 'review' },
  { filter: 'failed', state: 'failed' },
];

/**
 * One row under the board title that counts the board's runs by state. Each counter
 * is a toggle that narrows the board to the cards in that state; "Waiting for you"
 * comes first and is accented, because it is what the person has to act on. Hidden
 * when the project has never run anything.
 */
const AgentStrip: Component<{
  counts: RunCounts;
  active: ReadonlySet<RunFilter>;
  onToggle: (filter: RunFilter) => void;
}> = (props) => {
  const counter = (filter: RunFilter, count: () => number, label: string, state?: RunStateName, note?: string) => {
    const on = () => props.active.has(filter);
    // A counter at zero has nothing to narrow to, unless it is already narrowing the board.
    const unusable = () => count() === 0 && !on();
    const accented = filter === 'waiting';
    return (
      <button
        type="button"
        aria-pressed={on()}
        disabled={unusable()}
        title={unusable() ? `No runs are ${label.toLowerCase()}` : note}
        data-testid={`strip-${filter}`}
        onClick={() => props.onToggle(filter)}
        class="inline-flex min-h-7 items-center gap-1.5 rounded-pill px-3 text-xs font-semibold focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-60"
        style={{
          'background-color': on()
            ? 'var(--color-accent-soft)'
            : accented && count() > 0
              ? 'var(--run-needs-soft)'
              : 'var(--color-bg-subtle)',
          color: accented && count() > 0 ? 'var(--run-needs-ink)' : on() ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
          'box-shadow': on()
            ? 'inset 0 0 0 2px var(--color-accent-line)'
            : accented && count() > 0
              ? 'inset 0 0 0 1px var(--run-needs-line)'
              : 'none',
        }}
      >
        <Show when={state}>
          {(s) => (
            <span aria-hidden="true" class="inline-flex">
              <RunState state={s()} size="dot" />
            </span>
          )}
        </Show>
        <span class="text-sm font-bold tabular-nums">{count()}</span>{' '}
        {label}
        <Show when={note}>
          <span class="sr-only">{`. ${note}`}</span>
        </Show>
      </button>
    );
  };

  return (
    <Show when={totalRuns(props.counts) > 0}>
      <div
        role="group"
        aria-label="Agent runs on this board"
        data-testid="agent-strip"
        class="flex flex-wrap items-center gap-2"
        style={{ padding: '0 22px 10px' }}
      >
        {counter('waiting', () => waitingCount(props.counts), 'Waiting for you', undefined, WAITING_NOTE)}
        <For each={COUNTERS}>
          {(entry) => (
            counter(entry.filter, () => props.counts[entry.state], RUN_STATE_WORDS[entry.state], entry.state)
          )}
        </For>
      </div>
    </Show>
  );
};

/**
 * A column's runs as one stacked bar, a segment per state in proportion to its cards.
 * Each segment is a button that sets the same filter the strip does. The group's
 * label spells the bar out in words, so it is not read by colour or length.
 */
export const RunMiniBar: Component<{
  counts: RunCounts;
  active: ReadonlySet<RunFilter>;
  onToggle: (filter: RunFilter) => void;
}> = (props) => {
  const segments = () =>
    (Object.keys(RUN_STATE_WORDS) as RunStateName[])
      .filter((state) => props.counts[state] > 0)
      .map((state) => ({ state, count: props.counts[state], filter: filterOf(state) }));
  const summary = () => segments().map((s) => `${s.count} ${RUN_STATE_WORDS[s.state].toLowerCase()}`).join(', ');

  return (
    <Show when={segments().length > 0}>
      <div
        role="group"
        aria-label={`Runs in this column: ${summary()}`}
        data-testid="run-mini-bar"
        class="flex flex-none items-stretch gap-px overflow-hidden rounded-pill"
        style={{ width: '56px', height: '8px' }}
      >
        <For each={segments()}>
          {(segment) => {
            const label = `${segment.count} ${RUN_STATE_WORDS[segment.state].toLowerCase()}`;
            const filter = segment.filter;
            return filter ? (
              <button
                type="button"
                aria-label={label}
                aria-pressed={props.active.has(filter)}
                title={label}
                onClick={(e) => { e.stopPropagation(); props.onToggle(filter); }}
                class="min-w-1 border-0 p-0 focus:outline-none focus-visible:ring-2"
                style={{
                  flex: `${segment.count} 1 0`,
                  cursor: 'pointer',
                  'background-color': `var(--run-${segment.state}-ink)`,
                  opacity: props.active.size > 0 && !props.active.has(filter) ? 0.4 : 1,
                }}
              />
            ) : (
              <span
                role="img"
                aria-label={label}
                title={label}
                class="min-w-1"
                style={{ flex: `${segment.count} 1 0`, 'background-color': `var(--run-${segment.state}-ink)` }}
              />
            );
          }}
        </For>
      </div>
    </Show>
  );
};

export default AgentStrip;
