import { type Component, type JSX } from 'solid-js';

export interface RadioRowProps {
  name: string;
  value?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
  children: JSX.Element;
}

/**
 * One option of a native radio group, drawn as a pill row: the input stays a
 * real, focusable `<input type="radio">` (visually hidden, so the browser's
 * own arrow-key group navigation and every `input[type="radio"]` query keep
 * working) and a drawn dot plus a 2px accent ring mark the selected row.
 */
const RadioRow: Component<RadioRowProps> = (props) => (
  <label
    class="flex cursor-pointer items-center gap-2.5 rounded-full border-2 px-4 py-2.5 text-sm transition-colors has-[:disabled]:cursor-not-allowed"
    style={{
      'background-color': 'var(--color-bg-app)',
      'border-color': props.checked ? 'var(--color-primary-600)' : 'transparent',
      color: 'var(--color-text-primary)',
      opacity: props.disabled ? 0.6 : 1,
    }}
  >
    <input
      type="radio"
      class="peer sr-only"
      name={props.name}
      value={props.value}
      checked={props.checked}
      disabled={props.disabled}
      onChange={() => props.onChange()}
    />
    <span
      aria-hidden="true"
      class="h-4 w-4 flex-none rounded-full peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2"
      style={{
        border: `1.5px solid ${props.checked ? 'var(--color-primary-600)' : 'var(--color-border-medium)'}`,
        'background-color': props.checked ? 'var(--color-primary-600)' : 'transparent',
        'box-shadow': props.checked ? 'inset 0 0 0 3px var(--color-bg-app)' : 'none',
        'outline-color': 'var(--color-focus-ring)',
      }}
    />
    <span class="min-w-0">{props.children}</span>
  </label>
);

export default RadioRow;
