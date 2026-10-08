import {
  splitProps,
  createSignal,
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
  'caret-color': 'var(--color-primary-600)',
};

// Single-line controls are pills; multi-line ones use the item radius.
const controlClass =
  'w-full min-h-9 rounded-full border px-3.5 py-1.5 text-sm transition-colors ' +
  'focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';

const ringStyle = { '--tw-ring-color': 'var(--color-focus-ring)' } as JSX.CSSProperties;

export interface FieldHelp {
  text: string;
  href?: string;
}

/** The `(?)` next to a label: the text as a native title, and on click a small popover with a "More" link. */
export const HelpHint: Component<{ label?: string; help: FieldHelp }> = (props) => {
  const [open, setOpen] = createSignal(false);
  return (
    <span class="relative inline-flex align-middle">
      <button
        type="button"
        aria-label={`Help: ${props.label ?? ''}`}
        aria-expanded={open()}
        title={props.help.text}
        onClick={() => setOpen(!open())}
        class="text-xs"
        style={{ color: 'var(--color-text-tertiary)', cursor: 'pointer' }}
      >
        (?)
      </button>
      <Show when={open()}>
        <span
          role="note"
          class="absolute left-0 top-full z-50 mt-1 w-64 rounded-xl border p-2.5 text-xs"
          style={{
            'background-color': 'var(--color-bg-elevated)',
            'border-color': 'var(--color-border-medium)',
            color: 'var(--color-text-secondary)',
          }}
        >
          {props.help.text}
          <Show when={props.help.href}>
            {' '}
            <a href={props.help.href} target="_blank" rel="noreferrer" class="underline">
              More
            </a>
          </Show>
        </span>
      </Show>
    </span>
  );
};

export interface FieldShellProps {
  help?: FieldHelp;
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
      <div class="flex items-center gap-1">
        <label
          for={props.for}
          class="text-xs"
          style={{ color: 'var(--color-text-secondary)' }}
        >
          {props.label}
          <Show when={props.required}>
            <span style={{ color: 'var(--color-danger-600)' }}> *</span>
          </Show>
        </label>
        <Show when={props.help}>{(h) => <HelpHint label={props.label} help={h()} />}</Show>
      </div>
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
  help?: FieldHelp;
}

/** Labeled text input. Colors come only from tokens. */
const Field: Component<FieldProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'label',
    'error',
    'hint',
    'help',
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
      help={local.help}
      for={id}
      class={local.class}
    >
      <input
        {...rest}
        id={id}
        required={local.required}
        aria-invalid={local.error ? 'true' : undefined}
        class={controlClass}
        style={{
          ...controlStyle,
          ...ringStyle,
          ...(typeof local.style === 'object' ? local.style : {}),
        }}
      />
    </FieldShell>
  );
};

export default Field;
