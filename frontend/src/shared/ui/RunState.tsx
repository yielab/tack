import { type Component, type JSX, Match, Show, Switch } from 'solid-js';
import { IconClock, IconClose } from './icons';

// One language for where a run is. Each state has a fixed glyph and a fixed
// token pair, so the state is never carried by colour alone, and the same
// words mean the same thing on the board, the item page and the run.

export type RunStateName = 'queued' | 'running' | 'needs' | 'review' | 'done' | 'failed' | 'cancelled' | 'notstarted';

/** The docket words a run can carry. Each one is kept as the second line. */
export type DocketRunWord = 'Needs a person' | 'Needs your review' | 'Cancellation requested' | 'no changes recorded';

export type RunStateSize = 'dot' | 'chip' | 'banner';

export interface RunStateProps {
  state: RunStateName;
  /**
   * What docket says about the same run. A word that names a state sets the
   * state shown (a run that waits for a person is Needs you, whatever the
   * caller's state was); every word is shown as the second line.
   */
  docket?: DocketRunWord;
  /** `dot` icon only, `chip` a pill, `banner` a full-width row. Defaults to `chip`. */
  size?: RunStateSize;
}

/**
 * The words each state is shown under. They are a task's Work, so none of them may equal a status name
 * (a test checks them against the workflow templates): a finished run reads Finished, never the column Done.
 */
export const RUN_STATE_LABEL: Record<RunStateName, string> = {
  queued: 'Queued',
  running: 'Running',
  needs: 'Needs you',
  review: 'Ready for review',
  done: 'Finished',
  failed: 'Failed',
  cancelled: 'Cancelled',
  // Turned away before the agent did anything: neither a failure of the work nor a stop by a person.
  notstarted: 'Not started',
};

/** Which state a docket word puts a run in. "Cancellation requested" is still a run going on, so it stays Running. */
const DOCKET_STATE: Record<DocketRunWord, RunStateName | undefined> = {
  'Needs a person': 'needs',
  'Needs your review': 'review',
  'Cancellation requested': 'running',
  'no changes recorded': undefined,
};

// Glyphs are built fresh on every call: a DOM node can sit in one place only.
const frame = (size: number, body: JSX.Element, cls?: string): JSX.Element => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    class={cls}
    aria-hidden="true"
  >
    {body}
  </svg>
);

const GLYPH: Record<RunStateName, (size: number) => JSX.Element> = {
  queued: (size) => <IconClock size={size} />,
  running: (size) => frame(size, <path d="M8 2.5a5.5 5.5 0 1 1-5.5 5.5" />, 'run-state-spin'),
  needs: (size) =>
    frame(
      size,
      <path d="M5 8.2V4.6a1 1 0 0 1 2 0v3M7 7.4V3.2a1 1 0 0 1 2 0v4.2M9 7.4V4a1 1 0 0 1 2 0v4.6M11 6.2a1 1 0 0 1 2 0v3.4c0 2.3-1.6 4-3.9 4H8c-1.5 0-2.4-.6-3.2-1.8L3.3 9.4a1 1 0 0 1 1.5-1.3L5.5 9" />,
    ),
  review: (size) =>
    frame(
      size,
      <>
        <path d="M1.8 8s2.2-4 6.2-4 6.2 4 6.2 4-2.2 4-6.2 4-6.2-4-6.2-4z" />
        <circle cx="8" cy="8" r="1.9" />
      </>,
    ),
  done: (size) => frame(size, <path d="M3.4 8.6l3 3 6.2-6.6" />),
  failed: (size) => <IconClose size={size} />,
  notstarted: (size) => frame(size, <circle cx="8" cy="8" r="5.6" stroke-dasharray="2.4 2.4" />),
  cancelled: (size) =>
    frame(
      size,
      <>
        <circle cx="8" cy="8" r="5.6" />
        <path d="M4.1 11.9l7.8-7.8" />
      </>,
    ),
};

/** The state a run is in, and the second line of words, if any. */
export function runStateView(props: Pick<RunStateProps, 'state' | 'docket'>): { state: RunStateName; word?: string } {
  const docketState = props.docket ? DOCKET_STATE[props.docket] : undefined;
  const word = props.docket ? props.docket.charAt(0).toUpperCase() + props.docket.slice(1) : undefined;
  return { state: docketState ?? props.state, word };
}

/**
 * A run's state as an icon and a label. `dot` is the icon alone, with the label
 * as its `aria-label` and tooltip; `chip` is a pill; `banner` is a full-width
 * row that also carries the docket word on a second line.
 */
const RunState: Component<RunStateProps> = (props) => {
  const view = () => runStateView(props);
  const text = () => (view().word ? `${RUN_STATE_LABEL[view().state]} — ${view().word}` : RUN_STATE_LABEL[view().state]);
  const ink = () => `var(--run-${view().state}-ink)`;

  return (
    <Switch>
      <Match when={props.size === 'dot'}>
        <span
          role="img"
          aria-label={text()}
          title={text()}
          class="inline-flex flex-none items-center"
          style={{ color: ink() }}
        >
          {GLYPH[view().state](16)}
        </span>
      </Match>
      <Match when={props.size === 'banner'}>
        <div
          class="flex w-full min-w-0 items-start gap-2.5 rounded-item px-3 py-2"
          style={{
            'background-color': `var(--run-${view().state}-soft)`,
            color: ink(),
            'box-shadow': `inset 0 0 0 1px var(--run-${view().state}-line)`,
          }}
        >
          <span class="mt-px inline-flex flex-none items-center">{GLYPH[view().state](16)}</span>
          <div class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>
            <p class="text-sm font-semibold">{RUN_STATE_LABEL[view().state]}</p>
            <Show when={view().word}>
              {(word) => <p class="text-xs">{word()}</p>}
            </Show>
          </div>
        </div>
      </Match>
      <Match when={true}>
        <span
          title={text()}
          class="inline-flex items-center gap-1.5 whitespace-nowrap rounded-pill px-2.5 py-0.5 text-xs font-semibold"
          style={{
            'background-color': `var(--run-${view().state}-soft)`,
            color: ink(),
            'box-shadow': `inset 0 0 0 1px var(--run-${view().state}-line)`,
          }}
        >
          {GLYPH[view().state](13)}
          {RUN_STATE_LABEL[view().state]}
        </span>
      </Match>
    </Switch>
  );
};

export default RunState;
