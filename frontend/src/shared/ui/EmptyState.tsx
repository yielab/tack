import { type Component, type JSX, Show } from 'solid-js';

export interface EmptyStateProps {
  icon?: JSX.Element;
  title: string;
  description?: string;
  action?: JSX.Element;
}

/** Empty-state placeholder: a soft round badge, a display-face title, one
 *  line of guidance, an optional action. Token-driven. */
const EmptyState: Component<EmptyStateProps> = (props) => (
  <div class="flex flex-col items-center justify-center py-12 text-center">
    <Show when={props.icon}>
      <div
        class="mb-4 grid h-16 w-16 place-items-center rounded-full text-3xl"
        style={{ background: 'var(--color-accent2-soft)' }}
        aria-hidden="true"
      >
        {props.icon}
      </div>
    </Show>
    <p class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
      {props.title}
    </p>
    <Show when={props.description}>
      <p class="mt-1 max-w-sm text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        {props.description}
      </p>
    </Show>
    <Show when={props.action}>
      <div class="mt-4">{props.action}</div>
    </Show>
  </div>
);

export default EmptyState;
