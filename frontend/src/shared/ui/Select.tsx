import {
  splitProps,
  createUniqueId,
  For,
  type Component,
  type JSX,
} from 'solid-js';
import { FieldShell } from './Field';

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps
  extends JSX.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
  hint?: string;
  options?: SelectOption[];
}

const controlClass =
  'w-full rounded-lg border px-3 py-2 transition-colors ' +
  'focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';

const controlStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
  '--tw-ring-color': 'var(--color-focus-ring)',
} as JSX.CSSProperties;

/** Labeled <select>. Pass `options` or `children`. */
const Select: Component<SelectProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'label',
    'error',
    'hint',
    'required',
    'options',
    'children',
    'class',
    'id',
  ]);
  const id = local.id ?? createUniqueId();
  return (
    <FieldShell
      label={local.label}
      required={local.required}
      error={local.error}
      hint={local.hint}
      for={id}
      class={local.class}
    >
      <select
        {...rest}
        id={id}
        required={local.required}
        aria-invalid={local.error ? 'true' : undefined}
        class={controlClass}
        style={controlStyle}
      >
        {local.options ? (
          <For each={local.options}>
            {(o) => <option value={o.value}>{o.label}</option>}
          </For>
        ) : (
          local.children
        )}
      </select>
    </FieldShell>
  );
};

export default Select;
