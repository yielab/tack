import { For, Show, type Component } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useToasts, removeToast, type Toast } from './toast';

/** Accent dot color per toast type. */
function dotColor(type: Toast['type']): string {
  switch (type) {
    case 'success': return 'var(--color-success-600)';
    case 'error': return 'var(--color-danger-600)';
    case 'warning': return 'var(--color-warning-600)';
    case 'info':
    default: return 'var(--color-primary-600)';
  }
}

const ToastItem: Component<{ toast: Toast }> = (props) => (
  <div
    style={{
      display: 'flex', 'align-items': 'center', gap: '9px',
      padding: '10px 15px', 'border-radius': '11px',
      background: 'var(--color-text-primary)', color: 'var(--color-bg-app)',
      'font-size': '12.5px', 'font-weight': 600, 'box-shadow': 'var(--shadow-lg)',
      animation: 'tk-toast .2s ease', 'pointer-events': 'auto',
    }}
  >
    <span style={{ width: '7px', height: '7px', 'border-radius': '99px', background: dotColor(props.toast.type), 'flex-shrink': 0 }} />
    <span style={{ flex: 1, 'min-width': 0 }}>{props.toast.message}</span>
    <button
      onClick={() => removeToast(props.toast.id)}
      aria-label="Dismiss"
      style={{ background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--color-bg-app)', opacity: 0.6, 'font-size': '15px', 'line-height': 1, padding: 0 }}
    >
      ×
    </button>
  </div>
);

const ToastContainer: Component = () => {
  const toasts = useToasts();

  return (
    <Show when={toasts().length > 0}>
      <Portal>
        <div
          style={{
            position: 'fixed', bottom: '22px', left: '50%', transform: 'translateX(-50%)',
            'z-index': 80, display: 'flex', 'flex-direction': 'column-reverse', gap: '8px',
            'align-items': 'center', 'pointer-events': 'none',
          }}
        >
          <For each={toasts()}>
            {(toast) => <ToastItem toast={toast} />}
          </For>
        </div>
      </Portal>
    </Show>
  );
};

export default ToastContainer;
