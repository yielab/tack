import { type Component, For } from 'solid-js';
import { RunState, type RunStateName } from '../ui';
import type { StateTone } from './shared';

export interface RunChip {
  id: string;
  /** What the chip says: "Run 3", "Attempt 2". */
  label: string;
  /** The state tone the run's words were given. */
  tone: StateTone;
  /** The state in words, for the tooltip and the screen reader. */
  state: string;
}

/**
 * The run-state glyph a chip shows. Tone alone cannot tell Queued from
 * Cancelled, so the words decide first; the tone settles the rest.
 */
export function chipState(tone: StateTone, words: string): RunStateName {
  if (/needs your review|no changes recorded/i.test(words)) return 'review';
  if (/needs a person|needs operator/i.test(words)) return 'needs';
  if (/cancel/i.test(words)) return 'cancelled';
  if (/queued/i.test(words)) return 'queued';
  switch (tone) {
    case 'success':
      return 'done';
    case 'danger':
      return 'failed';
    case 'warning':
      return 'needs';
    case 'primary':
    case 'info':
      return 'running';
    default:
      return 'cancelled';
  }
}

/** A row of chips that picks one run (or one attempt): the picked chip is pressed. */
const RunChips: Component<{
  chips: RunChip[];
  active: string;
  onPick: (id: string) => void;
  label: string;
  testId: string;
}> = (props) => (
  <div role="group" aria-label={props.label} data-testid={props.testId} class="flex flex-wrap items-center gap-1.5">
    <For each={props.chips}>
      {(chip) => {
        const on = () => chip.id === props.active;
        return (
          <button
            type="button"
            aria-pressed={on()}
            title={`${chip.label}: ${chip.state}`}
            data-testid="run-chip"
            onClick={() => props.onPick(chip.id)}
            class="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-semibold focus:outline-none focus-visible:ring-2"
            style={{
              'background-color': on() ? 'var(--color-accent-soft)' : 'var(--color-bg-subtle)',
              color: on() ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
              'box-shadow': on() ? '0 0 0 2px var(--color-accent-line)' : 'none',
            }}
          >
            <span aria-hidden="true" class="inline-flex flex-none items-center">
              <RunState state={chipState(chip.tone, chip.state)} size="dot" />
            </span>
            {chip.label}
            <span class="sr-only">{`, ${chip.state}`}</span>
          </button>
        );
      }}
    </For>
  </div>
);

export default RunChips;
