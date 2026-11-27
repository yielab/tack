import { type Component, Show } from 'solid-js';
import { keyOf } from '../api/folder';

/** A row's key in grey: the name the project folder knows it by, which a rename never changes.
 *  Nothing is shown for a row with none. */
const KeyLabel: Component<{ row: { key?: string | null }; class?: string }> = (props) => (
  <Show when={keyOf(props.row)}>
    {(key) => (
      <span
        data-testid="row-key"
        title="Its key in the project folder. A rename keeps it."
        class={`font-mono text-micro ${props.class ?? ''}`}
        style={{ color: 'var(--color-text-tertiary)' }}
      >
        {key()}
      </span>
    )}
  </Show>
);

export default KeyLabel;
