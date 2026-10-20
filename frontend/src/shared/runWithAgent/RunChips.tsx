import { type Component, For } from 'solid-js';
import type { StateTone } from './shared';

export interface RunChip {
  id: string;
  /** What the chip says: "Run 3", "Attempt 2". */
  label: string;
  /** The state colour of the dot. */
  tone: StateTone;
  /** The state in words, for the tooltip and the screen reader. */
  state: string;
}

const DOT: Record<StateTone, string> = {
  neutral: 'var(--color-text-tertiary)',
  primary: 'var(--color-primary-600)',
  success: 'var(--color-success-600)',
  warning: 'var(--color-warning-600)',
  danger: 'var(--color-danger-600)',
  info: 'var(--color-info-600)',
};

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
            <span aria-hidden="true" class="h-2 w-2 flex-none rounded-full" style={{ 'background-color': DOT[chip.tone] }} />
            {chip.label}
            <span class="sr-only">{`, ${chip.state}`}</span>
          </button>
        );
      }}
    </For>
  </div>
);

export default RunChips;
