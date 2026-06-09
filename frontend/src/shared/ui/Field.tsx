import {
  splitProps,
  createUniqueId,
  Show,
  type Component,
  type JSX,
} from 'solid-js';
import clsx from 'clsx';

const controlStyle: JSX.CSSProperties = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
};

const controlClass =
  'w-full rounded-lg border px-3 py-2 transition-colors ' +
  'focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';

const ringStyle = { '--tw-ring-color': 'var(--color-focus-ring)' } as JSX.CSSProperties;

export interface FieldShellProps {
  label?: string;
  required?: boolean;
  error?: string;
  hint?: string;
  for?: string;
  class?: string;
  children: JSX.Element;
}

/** Label + hint/error frame shared by every form control. */
export const FieldShell: Component<FieldShellProps> = (props) => (
  <div class={clsx('flex flex-col gap-1', props.class)}>
    <Show when={props.label}>
      <label
        for={props.for}
        class="text-sm font-medium"
        style={{ color: 'var(--color-text-primary)' }}
      >
        {props.label}
        <Show when={props.required}>
          <span style={{ color: 'var(--color-danger-600)' }}> *</span>
        </Show>
      </label>
    </Show>
    {props.children}
    <Show when={props.error}>
      <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
        {props.error}
      </p>
    </Show>
    <Show when={props.hint && !props.error}>
      <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.hint}
      </p>
    </Show>
  </div>
);

export interface FieldProps
  extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  hint?: string;
}

/** Labeled text input. Colors come only from tokens. */
const Field: Component<FieldProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'label',
    'error',
    'hint',
    'required',
    'class',
    'id',
    'style',
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
      <input
        {...rest}
        id={id}
        required={local.required}
        aria-invalid={local.error ? 'true' : undefined}
        class={controlClass}
        style={{ ...controlStyle, ...ringStyle }}
      />
    </FieldShell>
  );
};

export default Field;
