import { type Component, Show } from 'solid-js';
import clsx from 'clsx';
import type { Progress } from '../types';

const number = (n: number): string => String(Math.round(n * 100) / 100);

/** How far the work under a group is, in words: "12 of 20 points" when the work has estimates, else "3 of 7 done". */
export function progressText(progress: Progress, pointsWord = 'points'): string {
  if (progress.estimate_total != null) {
    return `${number(progress.estimate_done ?? 0)} of ${number(progress.estimate_total)} ${pointsWord}`;
  }
  return `${progress.done} of ${progress.total} done`;
}

/** What waits for a person under a group: "1 needs you", "3 need you". Null when nothing does. */
export function needsYouText(progress: Progress): string | null {
  const n = progress.needs_you;
  return n > 0 ? `${n} ${n === 1 ? 'needs' : 'need'} you` : null;
}

export interface ProgressBarProps {
  /** The server's `Progress` of a group (`ItemCard.progress`); nothing is drawn without work under it. */
  progress: Progress | null | undefined;
  /** The project's word for points, when it has its own ("points" otherwise). */
  pointsWord?: string;
  /** A shorter bar, for a lane. */
  compact?: boolean;
  class?: string;
}

/**
 * The server's `Progress` as a bar and a sentence, in points when the work has estimates and in
 * items otherwise, with how many need a person beside it when any do. The bar is a drawing of the
 * sentence, which stays on screen, so colour is never the only cue.
 */
const ProgressBar: Component<ProgressBarProps> = (props) => {
  const fraction = () => {
    const p = props.progress;
    if (!p) return 0;
    const [done, total] = p.estimate_total != null ? [p.estimate_done ?? 0, p.estimate_total] : [p.done, p.total];
    return total > 0 ? Math.min(1, Math.max(0, done / total)) : 0;
  };
  return (
    <Show when={props.progress && props.progress.total > 0 ? props.progress : undefined}>
      {(progress) => (
        <span data-testid="progress" class={clsx('inline-flex min-w-0 items-center gap-2 text-xs', props.class)}>
          <span
            role="progressbar"
            aria-label={progressText(progress(), props.pointsWord)}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(fraction() * 100)}
            aria-valuetext={progressText(progress(), props.pointsWord)}
            class="relative inline-block flex-none overflow-hidden rounded-full"
            style={{
              width: props.compact ? '56px' : '96px',
              height: '6px',
              'background-color': 'var(--color-chip)',
            }}
          >
            <span
              data-testid="progress-fill"
              class="absolute inset-y-0 left-0 rounded-full"
              style={{ width: `${fraction() * 100}%`, 'background-color': 'var(--color-success-600)' }}
            />
          </span>
          <span data-testid="progress-text" class="whitespace-nowrap" style={{ color: 'var(--color-text-secondary)' }}>
            {progressText(progress(), props.pointsWord)}
          </span>
          <Show when={needsYouText(progress())}>
            {(text) => (
              <span data-testid="progress-needs" class="whitespace-nowrap font-semibold" style={{ color: 'var(--run-needs-ink)' }}>
                {text()}
              </span>
            )}
          </Show>
        </span>
      )}
    </Show>
  );
};

export default ProgressBar;
