import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export interface KbdHintProps {
  class?: string;
  children: JSX.Element;
}

/** Monospace keycap hint (e.g. ⌃K, esc, ↑↓). */
const KbdHint: Component<KbdHintProps> = (props) => (
  <span
    class={clsx('inline-flex items-center', props.class)}
    style={{
      'font-family': 'var(--font-mono)',
      'font-size': '10px',
      color: 'var(--color-text-tertiary)',
      border: '1px solid var(--color-border-light)',
      'border-radius': '5px',
      padding: '1px 5px',
    }}
  >
    {props.children}
  </span>
);

export default KbdHint;
