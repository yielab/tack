import { type Component } from 'solid-js';
import { isNearLimit, spaced } from './runLimit';

export interface UsageMeterProps {
  /** Tokens the run has counted so far. */
  used: number;
  /** The run's token limit. */
  limit: number;
  testId?: string;
}

/** How far into its token limit a run is, as a bar. The sentence is the bar's accessible name; the
 *  figures beside it are the same numbers. The bar turns amber at the share where the count does. */
const UsageMeter: Component<UsageMeterProps> = (props) => {
  const near = () => isNearLimit(props.used, props.limit);
  const share = () => (props.limit > 0 ? Math.min(1, Math.max(0, props.used / props.limit)) : 0);
  return (
    <div class="flex min-w-0 items-center gap-2">
      <div
        role="meter"
        data-testid={props.testId}
        data-near-limit={near()}
        aria-label={`Used ${spaced(props.used)} of ${spaced(props.limit)} tokens`}
        aria-valuemin={0}
        aria-valuemax={props.limit}
        aria-valuenow={Math.min(props.used, props.limit)}
        class="h-2 min-w-0 flex-1 overflow-hidden rounded-pill"
        style={{ 'background-color': 'var(--color-bg-subtle)', 'box-shadow': 'inset 0 0 0 1px var(--color-border-light)' }}
      >
        <div
          class="h-full rounded-pill"
          style={{
            width: `${Math.round(share() * 100)}%`,
            'background-color': near() ? 'var(--color-warning-600)' : 'var(--color-primary-600)',
          }}
        />
      </div>
      <span aria-hidden="true" class="flex-none text-xs tabular-nums" style={{ color: near() ? 'var(--color-warning-600)' : 'var(--color-text-secondary)' }}>
        {spaced(props.used)} / {spaced(props.limit)}
      </span>
    </div>
  );
};

export default UsageMeter;
