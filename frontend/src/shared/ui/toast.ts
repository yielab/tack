import { createSignal } from 'solid-js';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}

// Global toast state
const [toasts, setToasts] = createSignal<Toast[]>([]);

let toastIdCounter = 0;

export function useToasts() {
  return toasts;
}

export function showToast(
  message: string,
  type: ToastType = 'info',
  duration: number = 4000
) {
  const id = `toast-${++toastIdCounter}`;
  const toast: Toast = { id, type, message, duration };

  setToasts((prev) => [...prev, toast]);

  // Auto-remove after duration
  if (duration > 0) {
    setTimeout(() => {
      removeToast(id);
    }, duration);
  }

  return id;
}

export function removeToast(id: string) {
  setToasts((prev) => prev.filter((t) => t.id !== id));
}

// Convenience methods
export const toast = {
  success: (message: string, duration?: number) =>
    showToast(message, 'success', duration),
  error: (message: string, duration?: number) =>
    showToast(message, 'error', duration),
  warning: (message: string, duration?: number) =>
    showToast(message, 'warning', duration),
  info: (message: string, duration?: number) =>
    showToast(message, 'info', duration),
};
