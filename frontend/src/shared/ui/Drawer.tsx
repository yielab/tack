import {
  type Component,
  type JSX,
  Show,
  createEffect,
  onCleanup,
} from 'solid-js';
import { Portal } from 'solid-js/web';
import clsx from 'clsx';

export interface DrawerProps {
  isOpen: boolean;
  onClose: () => void;
  title?: string;
  /** Drawer width on desktop (Tailwind max-w-*). Full-screen sheet on mobile. */
  width?: 'md' | 'lg' | 'xl';
  children: JSX.Element;
}

const WIDTH: Record<NonNullable<DrawerProps['width']>, string> = {
  md: 'sm:max-w-md',
  lg: 'sm:max-w-lg',
  xl: 'sm:max-w-2xl',
};

/** Right-side drawer; full-screen sheet on mobile. ESC + overlay close, focus
 * trap entry, returns focus to the opener. Token-driven. */
const Drawer: Component<DrawerProps> = (props) => {
  let panel: HTMLDivElement | undefined;
  let previouslyFocused: HTMLElement | null = null;

  createEffect(() => {
    if (!props.isOpen) return;
    previouslyFocused = document.activeElement as HTMLElement | null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        props.onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    queueMicrotask(() => panel?.focus());

    onCleanup(() => {
      document.removeEventListener('keydown', onKey);
      previouslyFocused?.focus?.();
    });
  });

  const handleBackdrop = (e: MouseEvent) => {
    if (e.target === e.currentTarget) props.onClose();
  };

  return (
    <Show when={props.isOpen}>
      <Portal>
        <div
          class="fixed inset-0 z-50 flex justify-end"
          style={{ 'background-color': 'var(--color-bg-overlay)', animation: 'tk-overlay .15s ease' }}
          onClick={handleBackdrop}
        >
          <div
            ref={panel}
            role="dialog"
            aria-modal="true"
            aria-label={props.title}
            tabindex={-1}
            class={clsx(
              'flex h-full w-full flex-col focus:outline-none',
              WIDTH[props.width ?? 'lg']
            )}
            style={{
              'background-color': 'var(--color-bg-app)',
              'border-left': '1px solid var(--color-border-light)',
              'box-shadow': 'var(--shadow-lg)',
              animation: 'tk-drawer .22s cubic-bezier(.2,.7,.3,1)',
            }}
          >
            <Show when={props.title}>
              <div
                class="flex items-center justify-between border-b p-4"
                style={{ 'border-color': 'var(--color-border-light)' }}
              >
                <h2
                  class="text-lg font-semibold"
                  style={{ color: 'var(--color-text-primary)' }}
                >
                  {props.title}
                </h2>
                <button
                  onClick={props.onClose}
                  aria-label="Close"
                  class="rounded p-1 transition-colors focus:outline-none focus-visible:ring-2"
                  style={{
                    color: 'var(--color-text-tertiary)',
                    '--tw-ring-color': 'var(--color-focus-ring)',
                  }}
                >
                  <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>
            </Show>
            <div class="flex-1 overflow-y-auto p-4">{props.children}</div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default Drawer;
