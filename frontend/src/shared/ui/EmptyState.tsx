import { type Component, type JSX, Show } from 'solid-js';

export interface EmptyStateProps {
  icon?: JSX.Element;
  title: string;
  description?: string;
  action?: JSX.Element;
}

/** Centered empty-state placeholder. Token-driven. */
const EmptyState: Component<EmptyStateProps> = (props) => (
  <div class="flex flex-col items-center justify-center py-12 text-center">
    <Show when={props.icon}>
      <div class="mb-4 text-4xl" aria-hidden="true">
        {props.icon}
      </div>
    </Show>
    <p class="text-lg font-medium" style={{ color: 'var(--color-text-primary)' }}>
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
