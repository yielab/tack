import type { Component, JSX } from 'solid-js';

/** A line that says what is not so, kept visible: muted, announced as disabled. */
const DisabledLine: Component<{ children: JSX.Element; reported?: boolean; testid?: string }> = (props) => (
  <span
    data-testid={props.testid}
    data-reported={props.reported === false ? 'no' : undefined}
    aria-disabled="true"
    class="text-small"
    style={{ color: 'var(--color-text-tertiary)' }}
  >
    {props.children}
  </span>
);

export default DisabledLine;
