import { For, type Component } from 'solid-js';
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
    role={props.toast.type === 'error' ? 'alert' : undefined}
    style={{
      display: 'flex', 'align-items': 'center', gap: '9px',
      padding: '10px 18px', 'border-radius': '999px',
      background: 'var(--color-text-primary)', color: 'var(--color-bg-app)',
      'font-size': '0.8125rem', 'font-weight': 600, 'box-shadow': 'var(--shadow-lg)',
      animation: 'tk-toast .2s ease', 'pointer-events': 'auto',
    }}
  >
    <span aria-hidden="true" style={{ width: '8px', height: '8px', 'border-radius': '99px', background: dotColor(props.toast.type), 'flex-shrink': 0 }} />
    <span style={{ flex: 1, 'min-width': 0 }}>{props.toast.message}</span>
    <button
      onClick={() => removeToast(props.toast.id)}
      type="button"
      aria-label="Dismiss notification"
      style={{ background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--color-bg-app)', 'font-size': '0.9375rem', 'line-height': 1, padding: 0 }}
    >
      ×
    </button>
  </div>
);

const ToastContainer: Component = () => {
  const toasts = useToasts();

  // The live region stays mounted even with no toasts: a screen reader only
  // announces what is added to a region it already knows about. Errors carry
  // role="alert" on top, so they interrupt instead of waiting their turn.
  return (
    <Portal>
      <div
        aria-live="polite"
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
  );
};

export default ToastContainer;
