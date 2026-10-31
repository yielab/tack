import { type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  name?: string;
  /** Names the switch when the visible text should not (default: the text). */
  ariaLabel?: string;
  /** The visible text; it names the switch. */
  children: JSX.Element;
  /** Layout classes for the row (the base is a flex row, at least 24px tall, with a small gap). */
  class?: string;
  style?: JSX.CSSProperties;
}

/** An on/off switch with its text. A native checkbox with role="switch", so the keyboard and the
 *  screen reader see a switch, and a disabled one stays disabled with its reason shown beside it. */
const Switch: Component<SwitchProps> = (props) => (
  <label class={clsx('flex min-h-6 items-center gap-2 text-sm', props.class)} style={props.style}>
    <input
      type="checkbox"
      role="switch"
      name={props.name}
      aria-label={props.ariaLabel}
      class="h-4 w-4"
      style={{ 'accent-color': 'var(--color-primary-600)' }}
      checked={props.checked}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.currentTarget.checked)}
    />
    {props.children}
  </label>
);

export default Switch;
