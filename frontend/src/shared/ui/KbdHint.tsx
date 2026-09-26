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
      'font-size': '10.5px',
      color: 'var(--color-text-tertiary)',
      background: 'var(--color-bg-app)',
      'border-radius': '6px',
      padding: '1px 6px',
    }}
  >
    {props.children}
  </span>
);

export default KbdHint;
