import { Show, type Component } from 'solid-js';
import Button from './Button';

export interface LoadErrorProps {
  /** What could not be loaded, as a noun phrase: "the autonomy settings". */
  what: string;
  error?: unknown;
  onRetry: () => void;
}

/** A failed load, said as one: what could not load, why when the server said, and a way to try again.
 *  Stands where the empty state would have been, so a failure is never read as "nothing here". */
const LoadError: Component<LoadErrorProps> = (props) => {
  const reason = () => (props.error instanceof Error ? props.error.message : '');
  return (
    <div role="alert" class="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-sm" data-testid="load-error">
      <p style={{ color: 'var(--color-danger-600)' }}>
        Could not load {props.what}.
        <Show when={reason()}>
          {' '}
          <span style={{ color: 'var(--color-text-secondary)' }}>{reason()}</span>
        </Show>
      </p>
      <Button size="sm" variant="secondary" onClick={props.onRetry}>
        Try again
      </Button>
    </div>
  );
};

export default LoadError;
