import { For, Show, type Component } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useToasts, removeToast, type Toast } from '../lib/toast';

const ToastItem: Component<{ toast: Toast }> = (props) => {
  const getIcon = () => {
    switch (props.toast.type) {
      case 'success':
        return (
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M5 13l4 4L19 7"
            />
          </svg>
        );
      case 'error':
        return (
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        );
      case 'warning':
        return (
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
            />
          </svg>
        );
      case 'info':
      default:
        return (
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
        );
    }
  };

  const getColorClasses = () => {
    switch (props.toast.type) {
      case 'success':
        return 'bg-green-50 dark:bg-green-900/20 border-green-200 dark:border-green-800 text-green-800 dark:text-green-200';
      case 'error':
        return 'bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800 text-red-800 dark:text-red-200';
      case 'warning':
        return 'bg-yellow-50 dark:bg-yellow-900/20 border-yellow-200 dark:border-yellow-800 text-yellow-800 dark:text-yellow-200';
      case 'info':
      default:
        return 'bg-blue-50 dark:bg-blue-900/20 border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-200';
    }
  };

  return (
    <div
      class={`flex items-start gap-3 p-4 rounded-lg border shadow-lg backdrop-blur-sm transition-all duration-300 ease-in-out ${getColorClasses()}`}
      style={{
        animation: 'slideInRight 0.3s ease-out, fadeIn 0.3s ease-out',
      }}
    >
      <div class="flex-shrink-0 mt-0.5">{getIcon()}</div>
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium break-words">{props.toast.message}</p>
      </div>
      <button
        onClick={() => removeToast(props.toast.id)}
        class="flex-shrink-0 ml-2 hover:opacity-70 transition-opacity"
        aria-label="Close notification"
      >
        <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      </button>
    </div>
  );
};

const ToastContainer: Component = () => {
  const toasts = useToasts();

  return (
    <Show when={toasts().length > 0}>
      <Portal>
        <div
          class="fixed top-4 right-4 z-[9999] flex flex-col gap-3 max-w-md w-full pointer-events-none"
          style={{
            'max-height': 'calc(100vh - 2rem)',
            'overflow-y': 'auto',
          }}
        >
          <For each={toasts()}>
            {(toast) => (
              <div class="pointer-events-auto">
                <ToastItem toast={toast} />
              </div>
            )}
          </For>
        </div>
      </Portal>
    </Show>
  );
};

export default ToastContainer;
